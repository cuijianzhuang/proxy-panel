#!/usr/bin/env bash
# =============================================================================
#  proxy-panel 启动管理脚本 (Linux / macOS)
#
#  用法:
#    ./start.sh              交互菜单（默认）
#    ./start.sh start        直接启动（读 .env）
#    ./start.sh stop         停止后台服务
#    ./start.sh restart      重启
#    ./start.sh status       查看状态
#    ./start.sh config       配置向导（写/更新 .env）
#    ./start.sh build        仅构建（不启动）
#    ./start.sh logs         查看日志（tail -f）
#    ./start.sh --release    release 模式构建后启动
# =============================================================================
set -euo pipefail
cd "$(dirname "$0")"

PANEL_PID_FILE="./.proxy-panel.pid"
PANEL_LOG_FILE="./data/panel.log"
ENV_FILE="./.env"

# ── 颜色 ─────────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; MAGENTA='\033[0;35m'; BOLD='\033[1m'; NC='\033[0m'

_info()    { echo -e "${CYAN}▶ $*${NC}"; }
_ok()      { echo -e "${GREEN}✓ $*${NC}"; }
_warn()    { echo -e "${YELLOW}⚠ $*${NC}"; }
_err()     { echo -e "${RED}✗ $*${NC}"; }
_section() { echo -e "\n${BOLD}${MAGENTA}$*${NC}"; }

# ── .env 读取 ─────────────────────────────────────────────────────────────────
load_env() {
  [ -f "$ENV_FILE" ] || return 0
  while IFS= read -r line || [ -n "$line" ]; do
    # 去掉 Windows CRLF 的 \r
    line="${line%$'\r'}"
    # 忽略注释行和空行
    case "$line" in \#*|"") continue ;; esac
    # 只处理包含 = 的行
    case "$line" in *=*) ;; *) continue ;; esac
    key="${line%%=*}"
    val="${line#*=}"
    # 去掉值两端的引号（单引号 / 双引号）和前后空白
    val="${val#\"}" ; val="${val%\"}"
    val="${val#\'}" ; val="${val%\'}"
    # trim 前后空白（含 \r）
    val="${val#"${val%%[! $'\t'$'\r']*}"}"
    val="${val%"${val##*[! $'\t'$'\r']}"}"
    # key 也 trim
    key="${key#"${key%%[! ]*}"}"
    key="${key%"${key##*[! ]}"}"
    [ -z "$key" ] && continue
    export "$key=$val"
  done < "$ENV_FILE"
}

# ── 默认值（.env 可覆盖，环境变量最终优先）──────────────────────────────────
apply_defaults() {
  : "${PANEL_BIND:=127.0.0.1:8080}"
  : "${DATABASE_URL:=sqlite://./data/panel.db}"
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

# ── 快速校验关键变量（防止 CRLF / 引号残留导致 Rust 报解析错误）────────────
_validate_env() {
  local bind="$PANEL_BIND"
  # 必须形如 host:port，port 为纯数字
  if ! echo "$bind" | grep -qE '^[^:]+:[0-9]+$'; then
    _err "PANEL_BIND 格式无效: '$bind'"
    _err "请检查 .env 文件是否存在 Windows 换行符(CRLF)或多余引号"
    _err "正确格式示例: PANEL_BIND=127.0.0.1:8080"
    exit 1
  fi
}
_validate_env

# ── 构建模式 ─────────────────────────────────────────────────────────────────
PROFILE="dev"
BUILD_FLAGS=""
SKIP_BUILD=0
SUBCMD=""
for arg in "$@"; do
  case "$arg" in
    --release)              PROFILE="release"; BUILD_FLAGS="--release" ;;
    --skip-build|-s)        SKIP_BUILD=1 ;;
    start|stop|restart|status|config|build|logs|menu) SUBCMD="$arg" ;;
    *) ;;
  esac
done

BIN_DIR="target/$PROFILE"
BIN="$BIN_DIR/panel-server"

# ── 工具检查 ─────────────────────────────────────────────────────────────────
check_deps() {
  command -v cargo >/dev/null 2>&1 || {
    _err "未找到 cargo，请先安装 Rust: https://rustup.rs"
    exit 1
  }
}

# ── 前端构建 ─────────────────────────────────────────────────────────────────
build_frontend() {
  [ -f "web/dist/index.html" ] && return 0
  if command -v npm >/dev/null 2>&1; then
    _info "构建前端 (web/) ..."
    ( cd web && { [ -d node_modules ] || npm install; } && npm run build )
    _ok "前端构建完成"
  else
    _warn "未找到 npm，跳过前端构建 — 面板 UI 将不可用（API 仍可用）"
  fi
}

# ── 后端构建 ─────────────────────────────────────────────────────────────────
build_backend() {
  _info "构建后端 (cargo build $BUILD_FLAGS) ..."
  cargo build $BUILD_FLAGS -p panel-server
  _ok "后端构建完成 → $BIN"
}

# ── 显示当前配置 ─────────────────────────────────────────────────────────────
show_config() {
  HOST="${PANEL_BIND%%:*}"; PORT="${PANEL_BIND##*:}"
  [ "$HOST" = "0.0.0.0" ] && DISPLAY_HOST="127.0.0.1" || DISPLAY_HOST="$HOST"

  DBFILE=""
  case "$DATABASE_URL" in sqlite://*) DBFILE="${DATABASE_URL#sqlite://}" ;; esac
  FIRST_RUN=0
  [ -n "$DBFILE" ] && [ ! -f "$DBFILE" ] && FIRST_RUN=1

  echo -e "\n${BOLD}${MAGENTA}  🌸  proxy-panel${NC}"
  echo    "  ─────────────────────────────────────────────────────"
  printf  "  %-14s %s\n" "面板地址"   "http://${DISPLAY_HOST}:${PORT}"
  printf  "  %-14s %s\n" "数据库"     "${DATABASE_URL}"
  printf  "  %-14s %s\n" "远程模式"   "${PANEL_REMOTE_MODE}"
  printf  "  %-14s %s\n" "构建模式"   "${PROFILE}"
  printf  "  %-14s %s\n" "日志文件"   "${PANEL_LOG_FILE}"
  if [ "$PANEL_REMOTE_MODE" = "ssh" ] && [ -n "$PANEL_SSH_KEY" ]; then
    printf "  %-14s %s\n" "SSH 密钥"  "${PANEL_SSH_KEY}"
  fi
  if [ $FIRST_RUN -eq 1 ]; then
    if [ -n "$PANEL_ADMIN_PASSWORD" ]; then
      printf "  %-14s %s\n" "管理员" "admin / (来自 .env / 环境变量)"
    else
      printf "  %-14s %s\n" "管理员" "admin / (首次启动自动生成，见日志)"
    fi
  else
    printf  "  %-14s %s\n" "管理员" "admin / (沿用已有密码)"
  fi
  echo    "  ─────────────────────────────────────────────────────"
}

# ── 启动 ─────────────────────────────────────────────────────────────────────
do_start() {
  check_deps
  if [ "$SKIP_BUILD" -eq 0 ]; then
    build_frontend
    build_backend
  fi
  [ -x "$BIN" ] || { _err "找不到二进制 $BIN，请先构建"; exit 1; }
  mkdir -p data

  show_config
  echo -e "  ${CYAN}Ctrl+C 停止  |  日志: tail -f ${PANEL_LOG_FILE}${NC}\n"

  # 后台模式
  if [ "${PANEL_BACKGROUND:-0}" = "1" ]; then
    nohup "$BIN" >> "$PANEL_LOG_FILE" 2>&1 &
    echo $! > "$PANEL_PID_FILE"
    _ok "已在后台启动 (PID $(cat $PANEL_PID_FILE))"
  else
    exec "$BIN"
  fi
}

# ── 停止 ─────────────────────────────────────────────────────────────────────
do_stop() {
  if [ -f "$PANEL_PID_FILE" ]; then
    PID=$(cat "$PANEL_PID_FILE")
    if kill -0 "$PID" 2>/dev/null; then
      kill "$PID" && rm -f "$PANEL_PID_FILE"
      _ok "已停止 (PID $PID)"
    else
      _warn "PID $PID 不存在，清理 pid 文件"
      rm -f "$PANEL_PID_FILE"
    fi
  else
    # 尝试用进程名查
    PIDS=$(pgrep -x panel-server 2>/dev/null || true)
    if [ -n "$PIDS" ]; then
      echo "$PIDS" | xargs kill
      _ok "已停止进程: $PIDS"
    else
      _warn "未找到运行中的 panel-server"
    fi
  fi
}

# ── 状态 ─────────────────────────────────────────────────────────────────────
do_status() {
  HOST="${PANEL_BIND%%:*}"; PORT="${PANEL_BIND##*:}"
  [ "$HOST" = "0.0.0.0" ] && HOST="127.0.0.1"

  RUNNING=0
  PID=""
  if [ -f "$PANEL_PID_FILE" ]; then
    PID=$(cat "$PANEL_PID_FILE")
    kill -0 "$PID" 2>/dev/null && RUNNING=1
  fi
  [ $RUNNING -eq 0 ] && PID=$(pgrep -x panel-server 2>/dev/null | head -1 || true)
  [ -n "$PID" ] && RUNNING=1

  echo ""
  if [ $RUNNING -eq 1 ]; then
    _ok "运行中  PID=$PID"
    # HTTP 健康检查
    if command -v curl >/dev/null 2>&1; then
      STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://${HOST}:${PORT}/api/healthz" 2>/dev/null || echo "000")
      [ "$STATUS" = "200" ] && _ok "HTTP 健康检查: OK (http://${HOST}:${PORT})" \
                              || _warn "HTTP 健康检查: $STATUS"
    fi
  else
    _warn "未运行"
  fi
  echo ""
}

# ── 配置向导 ─────────────────────────────────────────────────────────────────
do_config() {
  _section "  配置向导 — 写入 ${ENV_FILE}"
  echo "  (直接回车保留当前值)"
  echo ""

  _prompt() {
    local key="$1" desc="$2" cur="$3" hidden="${4:-}"
    if [ "$hidden" = "hidden" ]; then
      printf "  %-22s [%s] : " "$desc" "***" && read -rs val; echo ""
    else
      printf "  %-22s [%s] : " "$desc" "$cur" && read -r val
    fi
    echo "${val:-$cur}"
  }

  BIND=$(_prompt    PANEL_BIND          "监听地址:端口"       "${PANEL_BIND}")
  DB=$(_prompt      DATABASE_URL        "数据库 URL"          "${DATABASE_URL}")
  REMOTE=$(_prompt  PANEL_REMOTE_MODE   "远程模式(dry-run/ssh)" "${PANEL_REMOTE_MODE}")
  SSHKEY=""
  if [ "$REMOTE" = "ssh" ]; then
    SSHKEY=$(_prompt PANEL_SSH_KEY      "SSH 私钥路径"        "${PANEL_SSH_KEY:-~/.ssh/id_ed25519}")
  fi
  PW=$(_prompt      PANEL_ADMIN_PASSWORD "管理员密码(留空=自动)" "${PANEL_ADMIN_PASSWORD}" hidden)
  SECURE=$(_prompt  PANEL_COOKIE_SECURE "HTTPS Cookie安全(0/1)" "${PANEL_COOKIE_SECURE}")

  # 写 .env
  cat > "$ENV_FILE" <<ENVEOF
# proxy-panel 配置文件 — 由 start.sh config 生成于 $(date '+%Y-%m-%d %H:%M:%S')
# 可手动编辑，start.sh 启动时自动读取

# 监听地址（0.0.0.0:8080 = 所有网卡）
PANEL_BIND=${BIND}

# 数据库（SQLite 推荐，PostgreSQL 格式: postgres://user:pass@host:5432/db）
DATABASE_URL=${DB}

# 远程推送模式: dry-run = 模拟不执行; ssh = 真实 SSH 推送
PANEL_REMOTE_MODE=${REMOTE}

# SSH 私钥路径（仅 PANEL_REMOTE_MODE=ssh 时有效）
PANEL_SSH_KEY=${SSHKEY}

# 管理员密码（留空 = 首次启动自动生成，打印到日志）
PANEL_ADMIN_PASSWORD=${PW}

# 部署在 HTTPS 后时设为 1（强制 Secure Cookie）
PANEL_COOKIE_SECURE=${SECURE}

# 日志级别
RUST_LOG=info,sqlx=warn
ENVEOF

  _ok "配置已写入 ${ENV_FILE}"
  echo ""
  cat "$ENV_FILE"
  echo ""
}

# ── 日志 ─────────────────────────────────────────────────────────────────────
do_logs() {
  mkdir -p data
  if [ -f "$PANEL_LOG_FILE" ]; then
    tail -f "$PANEL_LOG_FILE"
  else
    _warn "日志文件不存在: $PANEL_LOG_FILE"
    _info "后台启动时日志会写入该文件；前台启动时日志直接输出到控制台"
  fi
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

    # 当前状态
    PIDS=$(pgrep -x panel-server 2>/dev/null || true)
    if [ -n "$PIDS" ]; then
      echo -e "  状态: ${GREEN}● 运行中${NC}  PID=$PIDS"
    else
      echo -e "  状态: ${RED}○ 未运行${NC}"
    fi
    echo -e "  配置: $([ -f $ENV_FILE ] && echo "${GREEN}已有 .env${NC}" || echo "${YELLOW}无 .env（使用默认值）${NC}")"
    echo ""
    echo    "  ┌─────────────────────────────────────┐"
    echo    "  │  1) 前台启动（日志直接输出）         │"
    echo    "  │  2) 后台启动（日志写入文件）          │"
    echo    "  │  3) 停止服务                         │"
    echo    "  │  4) 重启服务                         │"
    echo    "  │  5) 查看状态                         │"
    echo    "  │  6) 配置向导（写/更新 .env）          │"
    echo    "  │  7) 查看日志（tail -f）               │"
    echo    "  │  8) 仅构建（不启动）                 │"
    echo    "  │  0) 退出                             │"
    echo    "  └─────────────────────────────────────┘"
    echo ""
    printf  "  请选择 [0-8]: "
    read -r choice

    case "$choice" in
      1)
        load_env; apply_defaults
        do_start
        ;;
      2)
        load_env; apply_defaults
        export PANEL_BACKGROUND=1
        do_start
        read -rp "  按 Enter 继续..."
        ;;
      3)
        do_stop
        read -rp "  按 Enter 继续..."
        ;;
      4)
        do_stop; sleep 1
        load_env; apply_defaults
        export PANEL_BACKGROUND=1
        do_start
        read -rp "  按 Enter 继续..."
        ;;
      5)
        do_status
        read -rp "  按 Enter 继续..."
        ;;
      6)
        do_config
        load_env; apply_defaults
        read -rp "  按 Enter 继续..."
        ;;
      7)
        do_logs
        ;;
      8)
        check_deps; build_frontend; build_backend
        read -rp "  按 Enter 继续..."
        ;;
      0|q|Q)
        echo ""
        _ok "再见！"
        exit 0
        ;;
      *)
        _warn "无效选项: $choice"
        sleep 1
        ;;
    esac
  done
}

# ── 路由 ─────────────────────────────────────────────────────────────────────
case "${SUBCMD:-menu}" in
  start)   do_start  ;;
  stop)    do_stop   ;;
  restart) do_stop; sleep 1; do_start ;;
  status)  do_status ;;
  config)  do_config ;;
  build)
    check_deps
    build_frontend
    build_backend
    ;;
  logs)    do_logs   ;;
  menu|"") do_menu   ;;
esac
