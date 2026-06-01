#!/usr/bin/env bash
# =============================================================================
#  proxy-panel 一键安装 & 管理脚本 (Linux / macOS)
#  从 GitHub Releases 下载预编译二进制，无需本地安装 Rust / Node.js
#
#  首次安装:
#    curl -fsSL https://raw.githubusercontent.com/cuijianzhuang/proxy-panel/main/install.sh | bash
#  或克隆后本地运行:
#    chmod +x install.sh && ./install.sh
#
#  子命令:
#    ./install.sh                交互菜单（默认）
#    ./install.sh install        下载/更新二进制并启动
#    ./install.sh update         检查并更新到最新版本
#    ./install.sh start          启动服务（前台）
#    ./install.sh start -d       启动服务（后台守护进程）
#    ./install.sh stop           停止后台服务
#    ./install.sh restart        重启
#    ./install.sh status         查看运行状态和版本
#    ./install.sh config         配置向导（写/更新 .env）
#    ./install.sh logs           查看日志（tail -f）
#    ./install.sh uninstall      卸载（删除二进制，保留数据）
# =============================================================================
set -euo pipefail

# ── 路径约定 ──────────────────────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="${PANEL_INSTALL_DIR:-$SCRIPT_DIR}"
BIN_DIR="$INSTALL_DIR/bin"
BIN="$BIN_DIR/panel-server"
DATA_DIR="$INSTALL_DIR/data"
PID_FILE="$INSTALL_DIR/.proxy-panel.pid"
LOG_FILE="$DATA_DIR/panel.log"
ENV_FILE="$INSTALL_DIR/.env"
VERSION_FILE="$BIN_DIR/.version"

# ── GitHub 仓库 ───────────────────────────────────────────────────────────────
REPO="cuijianzhuang/proxy-panel"
GITHUB_API="https://api.github.com/repos/${REPO}/releases/latest"
GITHUB_RELEASE="https://github.com/${REPO}/releases/latest"

# ── 颜色 ──────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; MAGENTA='\033[0;35m'; BOLD='\033[1m'; NC='\033[0m'

_info()    { echo -e "${CYAN}▶ $*${NC}"; }
_ok()      { echo -e "${GREEN}✓ $*${NC}"; }
_warn()    { echo -e "${YELLOW}⚠ $*${NC}"; }
_err()     { echo -e "${RED}✗ $*${NC}"; exit 1; }
_section() { echo -e "\n${BOLD}${MAGENTA}══ $* ══${NC}"; }

# ── .env 读取 ─────────────────────────────────────────────────────────────────
load_env() {
  [ -f "$ENV_FILE" ] || return 0
  while IFS= read -r line || [ -n "$line" ]; do
    # 剥离 Windows CRLF 的 \r
    line="${line%$'\r'}"
    # 忽略注释行和空行
    case "$line" in \#*|"") continue ;; esac
    # 只处理包含 = 的行
    case "$line" in *=*) ;; *) continue ;; esac
    local key="${line%%=*}"
    local val="${line#*=}"
    # 去掉值两端引号（单/双）
    val="${val#\"}" ; val="${val%\"}"
    val="${val#\'}" ; val="${val%\'}"
    # trim 前后空白（含 \r）
    val="${val#"${val%%[! $'\t'$'\r']*}"}"
    val="${val%"${val##*[! $'\t'$'\r']}"}"
    # trim key
    key="${key#"${key%%[! ]*}"}"
    key="${key%"${key##*[! ]}"}"
    [ -z "$key" ] && continue
    export "$key=$val"
  done < "$ENV_FILE"
}

# ── PANEL_BIND 格式校验 ──────────────────────────────────────────────────────
_validate_bind() {
  local bind="${PANEL_BIND:-}"
  if [ -n "$bind" ] && ! echo "$bind" | grep -qE '^[^:]+:[0-9]+$'; then
    echo -e "${RED}✗ PANEL_BIND 格式无效: '${bind}'${NC}"
    echo -e "${YELLOW}  可能原因: .env 存在 Windows CRLF 或多余引号${NC}"
    echo -e "${YELLOW}  修复: sed -i 's/\\r//' ${ENV_FILE}${NC}"
    echo -e "${YELLOW}  正确格式: PANEL_BIND=0.0.0.0:8080${NC}"
    exit 1
  fi
}

apply_defaults() {
  : "${PANEL_BIND:=0.0.0.0:8080}"
  : "${DATABASE_URL:=sqlite://${DATA_DIR}/panel.db}"
  : "${PANEL_REMOTE_MODE:=dry-run}"
  : "${PANEL_SSH_KEY:=}"
  : "${PANEL_ADMIN_PASSWORD:=}"
  : "${PANEL_COOKIE_SECURE:=0}"
  : "${RUST_LOG:=info,sqlx=warn}"
  export PANEL_BIND DATABASE_URL PANEL_REMOTE_MODE PANEL_SSH_KEY \
         PANEL_ADMIN_PASSWORD PANEL_COOKIE_SECURE RUST_LOG
}

load_env
apply_defaults
_validate_bind

# ── 参数解析 ──────────────────────────────────────────────────────────────────
SUBCMD=""
DAEMON=0
for arg in "$@"; do
  case "$arg" in
    install|update|start|stop|restart|status|config|logs|uninstall|menu) SUBCMD="$arg" ;;
    -d|--daemon) DAEMON=1 ;;
    *) ;;
  esac
done

# ── 平台检测 ──────────────────────────────────────────────────────────────────
detect_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Linux)
      case "$arch" in
        x86_64|amd64)  echo "linux-x86_64" ;;
        aarch64|arm64) echo "linux-aarch64" ;;
        *) _err "不支持的 Linux 架构: $arch（仅支持 x86_64 / aarch64）" ;;
      esac
      ;;
    Darwin)
      case "$arch" in
        x86_64)        echo "darwin-x86_64" ;;
        arm64)         echo "darwin-aarch64" ;;
        *) _err "不支持的 macOS 架构: $arch" ;;
      esac
      ;;
    *) _err "不支持的操作系统: $os（仅支持 Linux / macOS）" ;;
  esac
}

PLATFORM="$(detect_platform)"

# ── 下载工具检测 ──────────────────────────────────────────────────────────────
http_get() {
  local url="$1" out="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 --retry-delay 2 -o "$out" "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --tries=3 --waitretry=2 -O "$out" "$url"
  else
    _err "未找到 curl 或 wget，请先安装其中一个"
  fi
}

http_get_stdout() {
  local url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 3 "$url"
  elif command -v wget >/dev/null 2>&1; then
    wget -qO- --tries=3 "$url"
  else
    _err "未找到 curl 或 wget"
  fi
}

# ── 获取最新版本信息 ──────────────────────────────────────────────────────────
fetch_latest_version() {
  local json tag
  # 优先使用 GitHub API（返回详细信息）
  if json="$(http_get_stdout "$GITHUB_API" 2>/dev/null)"; then
    # 从 JSON 提取 tag_name，不依赖 jq
    tag="$(echo "$json" | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
    [ -n "$tag" ] && echo "$tag" && return 0
  fi
  _err "无法获取最新版本信息，请检查网络或访问: ${GITHUB_RELEASE}"
}

current_version() {
  [ -f "$VERSION_FILE" ] && cat "$VERSION_FILE" || echo "（未安装）"
}

# ── 校验和验证 ────────────────────────────────────────────────────────────────
verify_sha256() {
  local file="$1" expected="$2"
  local actual
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$file" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    actual="$(shasum -a 256 "$file" | awk '{print $1}')"
  else
    _warn "未找到 sha256sum / shasum，跳过校验"
    return 0
  fi
  if [ "$actual" = "$expected" ]; then
    _ok "SHA256 校验通过"
  else
    echo ""
    _err "SHA256 校验失败！\n  期望: $expected\n  实际: $actual\n文件可能已损坏，请重试。"
  fi
}

# ── 下载并安装二进制 ──────────────────────────────────────────────────────────
do_download() {
  local version="${1:-}"
  [ -z "$version" ] && version="$(fetch_latest_version)"

  local archive="panel-server-${PLATFORM}.tar.gz"
  local sha_file="panel-server-${PLATFORM}.tar.gz.sha256"
  local base_url="https://github.com/${REPO}/releases/download/${version}"

  _section "下载 proxy-panel ${version} [${PLATFORM}]"

  mkdir -p "$BIN_DIR"
  local tmp_dir
  tmp_dir="$(mktemp -d)"
  trap 'rm -rf "$tmp_dir"' EXIT

  # 下载压缩包
  _info "正在下载 ${archive} ..."
  http_get "${base_url}/${archive}" "${tmp_dir}/${archive}"

  # 下载并验证校验和
  _info "验证 SHA256 ..."
  if http_get "${base_url}/${sha_file}" "${tmp_dir}/${sha_file}" 2>/dev/null; then
    expected="$(awk '{print $1}' "${tmp_dir}/${sha_file}")"
    verify_sha256 "${tmp_dir}/${archive}" "$expected"
  else
    _warn "未能下载校验文件，跳过 SHA256 验证"
  fi

  # 解压
  _info "解压 ..."
  tar xzf "${tmp_dir}/${archive}" -C "${tmp_dir}"
  install -m 755 "${tmp_dir}/panel-server-${PLATFORM}" "$BIN"

  # 记录版本
  echo "$version" > "$VERSION_FILE"

  trap - EXIT
  rm -rf "$tmp_dir"

  _ok "二进制已安装: $BIN  (版本: $version)"
}

# ── 安装/更新入口 ─────────────────────────────────────────────────────────────
do_install() {
  _section "安装 proxy-panel"

  local latest
  latest="$(fetch_latest_version)"
  local current
  current="$(current_version)"

  if [ -f "$BIN" ] && [ "$current" = "$latest" ]; then
    _ok "已是最新版本: $latest，无需重新下载"
    _ok "如需强制重装，请先运行: rm -f ${BIN}"
  else
    [ "$current" != "（未安装）" ] && _info "当前版本: $current  →  最新版本: $latest"
    do_download "$latest"
  fi

  mkdir -p "$DATA_DIR"

  # 首次安装时引导配置
  if [ ! -f "$ENV_FILE" ]; then
    echo ""
    _info "检测到首次安装，进入配置向导..."
    do_config
  fi

  echo ""
  _ok "安装完成！运行以下命令启动服务："
  echo ""
  echo "    $0 start       # 前台运行"
  echo "    $0 start -d    # 后台守护进程"
  echo ""
}

do_update() {
  _section "检查更新"
  local latest current
  latest="$(fetch_latest_version)"
  current="$(current_version)"

  if [ "$current" = "$latest" ]; then
    _ok "已是最新版本: $latest"
    return 0
  fi

  _info "发现新版本: $current → $latest"
  printf "  是否立即更新？[Y/n] "
  read -r answer
  answer="${answer:-Y}"
  if [[ "$answer" =~ ^[Yy]$ ]]; then
    # 更新前停止服务
    if is_running; then
      _info "停止当前服务..."
      _do_stop_quiet
      local was_running=1
    else
      local was_running=0
    fi

    do_download "$latest"

    if [ "${was_running:-0}" -eq 1 ]; then
      _info "重新启动服务..."
      _do_start_bg
    fi
    _ok "更新完成: $latest"
  else
    _info "已取消更新"
  fi
}

# ── 运行状态辅助 ─────────────────────────────────────────────────────────────
is_running() {
  local pid=""
  [ -f "$PID_FILE" ] && pid="$(cat "$PID_FILE" 2>/dev/null)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    return 0
  fi
  pid="$(pgrep -x panel-server 2>/dev/null | head -1 || true)"
  [ -n "$pid" ]
}

get_pid() {
  local pid=""
  [ -f "$PID_FILE" ] && pid="$(cat "$PID_FILE" 2>/dev/null)"
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    echo "$pid"; return
  fi
  pgrep -x panel-server 2>/dev/null | head -1 || true
}

# ── 前置检查 ──────────────────────────────────────────────────────────────────
check_binary() {
  if [ ! -f "$BIN" ]; then
    _err "未找到可执行文件: $BIN\n请先运行安装: $0 install"
  fi
  [ -x "$BIN" ] || chmod +x "$BIN"
}

# ── 显示配置摘要 ─────────────────────────────────────────────────────────────
show_config_summary() {
  local host port display_host
  host="${PANEL_BIND%%:*}"; port="${PANEL_BIND##*:}"
  [ "$host" = "0.0.0.0" ] && display_host="<本机所有IP>" || display_host="$host"

  local first_run=0
  local dbfile=""
  case "$DATABASE_URL" in sqlite://*) dbfile="${DATABASE_URL#sqlite://}" ;; esac
  [ -n "$dbfile" ] && [ ! -f "$dbfile" ] && first_run=1

  echo ""
  echo -e "${BOLD}${MAGENTA}  🌸  proxy-panel  $(current_version)${NC}"
  echo    "  ──────────────────────────────────────────────────────"
  printf  "  %-16s %s\n" "面板地址"   "http://${display_host}:${port}"
  printf  "  %-16s %s\n" "数据库"     "${DATABASE_URL}"
  printf  "  %-16s %s\n" "远程模式"   "${PANEL_REMOTE_MODE}"
  printf  "  %-16s %s\n" "日志文件"   "${LOG_FILE}"
  if [ "$PANEL_REMOTE_MODE" = "ssh" ] && [ -n "$PANEL_SSH_KEY" ]; then
    printf "  %-16s %s\n" "SSH 密钥"  "${PANEL_SSH_KEY}"
  fi
  if [ $first_run -eq 1 ]; then
    if [ -n "$PANEL_ADMIN_PASSWORD" ]; then
      printf "  %-16s %s\n" "管理员密码" "来自 .env"
    else
      printf "  %-16s %s\n" "管理员密码" "首次启动自动生成（见日志）"
    fi
  else
    printf  "  %-16s %s\n" "管理员密码" "沿用已有密码"
  fi
  echo    "  ──────────────────────────────────────────────────────"
}

# ── 后台启动（内部）─────────────────────────────────────────────────────────
_do_start_bg() {
  mkdir -p "$DATA_DIR"
  # 重定向：stdout+stderr 追加到日志，关闭 stdin（防止 nohup 的 "ignoring input" 提示）
  nohup "$BIN" </dev/null >>"$LOG_FILE" 2>&1 &
  local new_pid=$!
  echo "$new_pid" > "$PID_FILE"
  # 等待最多 2 秒确认进程存活
  local i=0
  while [ $i -lt 4 ]; do
    sleep 0.5
    kill -0 "$new_pid" 2>/dev/null && break
    i=$((i+1))
  done
  if kill -0 "$new_pid" 2>/dev/null; then
    _ok "已在后台启动 (PID ${new_pid})"
    _info "日志: tail -f ${LOG_FILE}"
  else
    _err "启动失败，请查看日志: ${LOG_FILE}"
  fi
}

_do_stop_quiet() {
  local pid
  pid="$(get_pid)"
  if [ -n "$pid" ]; then
    kill "$pid" 2>/dev/null || true
    # 等待最多 5 秒
    local i=0
    while kill -0 "$pid" 2>/dev/null && [ $i -lt 10 ]; do
      sleep 0.5; i=$((i+1))
    done
    rm -f "$PID_FILE"
  fi
}

# ── 启动 ─────────────────────────────────────────────────────────────────────
do_start() {
  check_binary
  mkdir -p "$DATA_DIR"
  show_config_summary

  if is_running; then
    _warn "服务已在运行中 (PID $(get_pid))"
    return 0
  fi

  if [ "$DAEMON" -eq 1 ]; then
    _do_start_bg
  else
    echo -e "  ${CYAN}Ctrl+C 停止  |  后台模式: $0 start -d${NC}\n"
    exec "$BIN"
  fi
}

# ── 停止 ─────────────────────────────────────────────────────────────────────
do_stop() {
  local pid
  pid="$(get_pid)"
  if [ -z "$pid" ]; then
    _warn "未找到运行中的 panel-server"
    return 0
  fi
  kill "$pid"
  rm -f "$PID_FILE"
  _ok "已停止 (PID $pid)"
}

# ── 状态 ─────────────────────────────────────────────────────────────────────
do_status() {
  local host port
  host="${PANEL_BIND%%:*}"; port="${PANEL_BIND##*:}"
  [ "$host" = "0.0.0.0" ] && host="127.0.0.1"

  echo ""
  if is_running; then
    _ok "运行中  PID=$(get_pid)  版本=$(current_version)"
    if command -v curl >/dev/null 2>&1; then
      local code
      code="$(curl -s -o /dev/null -w "%{http_code}" \
               --max-time 3 "http://${host}:${port}/api/healthz" 2>/dev/null || echo "000")"
      [ "$code" = "200" ] \
        && _ok "HTTP 健康检查: OK  →  http://${host}:${port}" \
        || _warn "HTTP 健康检查: HTTP $code"
    fi
  else
    _warn "未运行"
  fi

  echo ""
  if [ -f "$BIN" ]; then
    _ok "二进制: $BIN  ($(du -sh "$BIN" 2>/dev/null | cut -f1))  版本: $(current_version)"
  else
    _warn "二进制未安装，运行: $0 install"
  fi

  # 检查是否有新版本可用
  if [ -f "$BIN" ] && command -v curl >/dev/null 2>&1; then
    local latest
    latest="$(fetch_latest_version 2>/dev/null || true)"
    local current
    current="$(current_version)"
    if [ -n "$latest" ] && [ "$latest" != "$current" ]; then
      echo ""
      _warn "发现新版本: $latest  (当前: $current)"
      _info "运行 $0 update 可一键升级"
    fi
  fi
  echo ""
}

# ── 配置向导 ─────────────────────────────────────────────────────────────────
do_config() {
  _section "配置向导  →  ${ENV_FILE}"
  echo "  (直接回车保留当前值)"
  echo ""

  # 注意: printf 必须重定向到 /dev/tty（或 >&2），否则提示文字会被 $() 一起捕获
  _prompt() {
    local desc="$1" cur="$2" hidden="${3:-}"
    if [ "$hidden" = "hidden" ]; then
      printf "  %-24s [%s] : " "$desc" "***" > /dev/tty
      read -rs val < /dev/tty; printf "\n" > /dev/tty
    else
      printf "  %-24s [%s] : " "$desc" "$cur" > /dev/tty
      read -r val < /dev/tty
    fi
    printf "%s" "${val:-$cur}"
  }

  local bind db remote sshkey pw secure
  bind=$(_prompt   "监听地址:端口"           "${PANEL_BIND}")
  db=$(_prompt     "数据库 URL"              "${DATABASE_URL}")
  remote=$(_prompt "远程模式(dry-run/ssh)"   "${PANEL_REMOTE_MODE}")
  sshkey=""
  if [ "$remote" = "ssh" ]; then
    sshkey=$(_prompt "SSH 私钥路径"          "${PANEL_SSH_KEY:-~/.ssh/id_ed25519}")
  fi
  pw=$(_prompt     "管理员密码(留空=自动)"   "${PANEL_ADMIN_PASSWORD}" hidden)
  secure=$(_prompt "HTTPS Cookie 安全(0/1)" "${PANEL_COOKIE_SECURE}")

  mkdir -p "$(dirname "$ENV_FILE")"
  cat > "$ENV_FILE" <<ENVEOF
# proxy-panel 配置文件 — 由 install.sh 生成于 $(date '+%Y-%m-%d %H:%M:%S')
# 可手动编辑，下次启动自动读取

PANEL_BIND=${bind}
DATABASE_URL=${db}
PANEL_REMOTE_MODE=${remote}
PANEL_SSH_KEY=${sshkey}
PANEL_ADMIN_PASSWORD=${pw}
PANEL_COOKIE_SECURE=${secure}
RUST_LOG=info,sqlx=warn
ENVEOF

  _ok "配置已保存: ${ENV_FILE}"
  echo ""
  cat "$ENV_FILE"
  echo ""
  load_env
  apply_defaults
}

# ── 日志 ─────────────────────────────────────────────────────────────────────
do_logs() {
  mkdir -p "$DATA_DIR"
  if [ -f "$LOG_FILE" ]; then
    _info "按 Ctrl+C 退出"
    tail -f "$LOG_FILE"
  else
    _warn "日志文件不存在: $LOG_FILE"
    _info "服务以后台模式 ($0 start -d) 运行后日志会写入该文件"
  fi
}

# ── 卸载 ─────────────────────────────────────────────────────────────────────
do_uninstall() {
  _section "卸载 proxy-panel"
  echo ""
  _warn "此操作将删除二进制文件，但 保留 data/ 目录和 .env 配置文件"
  printf "  确认卸载？[y/N] "
  read -r ans
  [[ "$ans" =~ ^[Yy]$ ]] || { _info "已取消"; return 0; }

  if is_running; then
    _info "停止服务..."
    _do_stop_quiet
  fi

  rm -f "$BIN" "$VERSION_FILE"
  rmdir "$BIN_DIR" 2>/dev/null || true
  rm -f "$PID_FILE"

  _ok "卸载完成"
  _info "数据保留在: $DATA_DIR"
  _info "配置保留在: $ENV_FILE"
  _info "重新安装: $0 install"
}

# ── 交互菜单 ─────────────────────────────────────────────────────────────────
do_menu() {
  while true; do
    clear
    echo -e "${BOLD}${MAGENTA}"
    echo "  ██████╗ ██████╗  ██████╗ ██╗  ██╗██╗   ██╗"
    echo "  ██╔══██╗██╔══██╗██╔═══██╗╚██╗██╔╝╚██╗ ██╔╝"
    echo "  ██████╔╝██████╔╝██║   ██║ ╚███╔╝  ╚████╔╝ "
    echo "  ██╔═══╝ ██╔══██╗██║   ██║ ██╔██╗   ╚██╔╝  "
    echo "  ██║     ██║  ██║╚██████╔╝██╔╝ ██╗   ██║   "
    echo "  ╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝   PANEL"
    echo -e "${NC}"

    # 状态栏
    if is_running; then
      echo -e "  状态:   ${GREEN}● 运行中${NC}  PID=$(get_pid)"
    else
      echo -e "  状态:   ${RED}○ 未运行${NC}"
    fi

    local ver; ver="$(current_version)"
    if [ -f "$BIN" ]; then
      echo -e "  版本:   ${GREEN}${ver}${NC}  [${PLATFORM}]"
    else
      echo -e "  版本:   ${RED}未安装${NC}"
    fi
    echo -e "  配置:   $([ -f "$ENV_FILE" ] && echo "${GREEN}${ENV_FILE}${NC}" || echo "${YELLOW}无（使用默认值）${NC}")"
    echo ""
    echo    "  ┌──────────────────────────────────────────────┐"
    echo    "  │  1) 前台启动                                  │"
    echo    "  │  2) 后台启动（守护进程）                       │"
    echo    "  │  3) 停止服务                                  │"
    echo    "  │  4) 重启服务                                  │"
    echo    "  │  5) 查看状态                                  │"
    echo    "  │  6) 配置向导                                  │"
    echo    "  │  7) 查看日志（tail -f）                       │"
    echo    "  │  8) 安装 / 检查最新版                         │"
    echo    "  │  9) 检查并更新                                │"
    echo    "  │  u) 卸载（保留数据）                          │"
    echo    "  │  0) 退出                                     │"
    echo    "  └──────────────────────────────────────────────┘"
    echo ""
    printf  "  请选择: "
    read -r choice

    case "$choice" in
      1) load_env; apply_defaults; DAEMON=0; do_start ;;
      2) load_env; apply_defaults; DAEMON=1; do_start
         read -rp "  按 Enter 继续..." ;;
      3) do_stop;   read -rp "  按 Enter 继续..." ;;
      4) do_stop; sleep 1; load_env; apply_defaults; DAEMON=1; do_start
         read -rp "  按 Enter 继续..." ;;
      5) do_status; read -rp "  按 Enter 继续..." ;;
      6) do_config; load_env; apply_defaults
         read -rp "  按 Enter 继续..." ;;
      7) do_logs ;;
      8) do_install; read -rp "  按 Enter 继续..." ;;
      9) do_update;  read -rp "  按 Enter 继续..." ;;
      u|U) do_uninstall; read -rp "  按 Enter 继续..." ;;
      0|q|Q) echo ""; _ok "再见！"; exit 0 ;;
      *) _warn "无效选项: $choice"; sleep 1 ;;
    esac
  done
}

# ── 路由 ──────────────────────────────────────────────────────────────────────
case "${SUBCMD:-menu}" in
  install)   do_install ;;
  update)    do_update  ;;
  start)     do_start   ;;
  stop)      do_stop    ;;
  restart)   do_stop; sleep 1; load_env; apply_defaults; DAEMON=1; do_start ;;
  status)    do_status  ;;
  config)    do_config  ;;
  logs)      do_logs    ;;
  uninstall) do_uninstall ;;
  menu|"")   do_menu    ;;
esac
