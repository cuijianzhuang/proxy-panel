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
#    ./start.sh install      仅安装依赖（Rust + Node.js）
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
  if [ -f "$ENV_FILE" ]; then
    while IFS= read -r line || [ -n "$line" ]; do
      [[ "$line" =~ ^#.*$ || -z "$line" ]] && continue
      key="${line%%=*}"; val="${line#*=}"
      val="${val%\"}"; val="${val#\"}"; val="${val%\'}"; val="${val#\'}"
      export "$key=$val"
    done < "$ENV_FILE"
  fi
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

# ── 构建模式 ─────────────────────────────────────────────────────────────────
PROFILE="dev"
BUILD_FLAGS=""
SKIP_BUILD=0
SUBCMD=""
for arg in "$@"; do
  case "$arg" in
    --release)              PROFILE="release"; BUILD_FLAGS="--release" ;;
    --skip-build|-s)        SKIP_BUILD=1 ;;
    start|stop|restart|status|config|build|logs|menu|install) SUBCMD="$arg" ;;
    *) ;;
  esac
done

BIN_DIR="target/$PROFILE"
BIN="$BIN_DIR/panel-server"

# ── Rust 自动安装 ─────────────────────────────────────────────────────────────
install_rust() {
  _info "正在通过 rustup 自动安装 Rust..."
  if ! curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path; then
    _err "Rust 安装失败，请手动访问 https://rustup.rs 安装"
    exit 1
  fi
  # 加载到当前 shell
  source_cargo_env
  _ok "Rust 安装完成: $(rustc --version 2>/dev/null || echo '请重新打开终端')"
}

source_cargo_env() {
  if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck source=/dev/null
    . "$HOME/.cargo/env"
  elif [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
}

# ── Node.js 自动安装 ──────────────────────────────────────────────────────────
install_nodejs() {
  _info "正在自动安装 Node.js..."

  # 方案1: 系统包管理器
  if command -v apt-get >/dev/null 2>&1; then
    _info "使用 apt 安装 Node.js 18..."
    if command -v curl >/dev/null 2>&1; then
      curl -fsSL https://deb.nodesource.com/setup_18.x | bash - 2>/dev/null || true
    fi
    apt-get install -y nodejs 2>/dev/null || {
      _warn "apt 安装失败，尝试 nvm..."
      install_nodejs_via_nvm
    }
    return
  fi

  if command -v yum >/dev/null 2>&1; then
    _info "使用 yum 安装 Node.js 18..."
    if command -v curl >/dev/null 2>&1; then
      curl -fsSL https://rpm.nodesource.com/setup_18.x | bash - 2>/dev/null || true
    fi
    yum install -y nodejs 2>/dev/null || {
      _warn "yum 安装失败，尝试 nvm..."
      install_nodejs_via_nvm
    }
    return
  fi

  if command -v dnf >/dev/null 2>&1; then
    _info "使用 dnf 安装 Node.js..."
    dnf install -y nodejs npm 2>/dev/null || {
      _warn "dnf 安装失败，尝试 nvm..."
      install_nodejs_via_nvm
    }
    return
  fi

  if command -v brew >/dev/null 2>&1; then
    _info "使用 Homebrew 安装 Node.js..."
    brew install node || {
      _warn "brew 安装失败，尝试 nvm..."
      install_nodejs_via_nvm
    }
    return
  fi

  # 方案2: nvm
  install_nodejs_via_nvm
}

install_nodejs_via_nvm() {
  _info "通过 nvm 安装 Node.js LTS..."
  # 安装 nvm
  NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
  if [ ! -d "$NVM_DIR" ]; then
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.7/install.sh | bash || {
      _err "nvm 安装失败，请手动安装 Node.js: https://nodejs.org"
      return 1
    }
  fi
  # 加载 nvm
  export NVM_DIR
  # shellcheck source=/dev/null
  [ -s "$NVM_DIR/nvm.sh" ] && . "$NVM_DIR/nvm.sh"
  nvm install --lts
  nvm use --lts
  _ok "Node.js 已通过 nvm 安装: $(node --version 2>/dev/null || echo '请重新打开终端')"
}

load_nvm() {
  NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
  if [ -s "$NVM_DIR/nvm.sh" ]; then
    export NVM_DIR
    # shellcheck source=/dev/null
    . "$NVM_DIR/nvm.sh"
  fi
}

# ── 工具检查与自动安装 ────────────────────────────────────────────────────────
check_deps() {
  # 尝试加载 cargo 和 nvm 到当前 shell（应对首次安装后未重开终端的情况）
  source_cargo_env
  load_nvm

  local need_rust=0 need_node=0

  if ! command -v cargo >/dev/null 2>&1; then
    need_rust=1
  fi
  if ! command -v npm >/dev/null 2>&1; then
    need_node=1
  fi

  if [ $need_rust -eq 1 ] || [ $need_node -eq 1 ]; then
    echo ""
    _warn "检测到缺少依赖，将自动安装："
    [ $need_rust -eq 1 ] && echo "  • Rust (cargo/rustc) — 构建后端所需"
    [ $need_node -eq 1 ] && echo "  • Node.js (npm)      — 构建前端所需"
    echo ""
    printf "  是否自动安装？[Y/n] "
    read -r answer
    answer="${answer:-Y}"
    if [[ "$answer" =~ ^[Yy]$ ]]; then
      [ $need_rust -eq 1 ] && install_rust
      [ $need_node -eq 1 ] && install_nodejs
      # 重新加载环境
      source_cargo_env
      load_nvm
    else
      _err "已取消。请手动安装缺少的依赖后重试。"
      [ $need_rust -eq 1 ] && echo "  Rust:   https://rustup.rs"
      [ $need_node -eq 1 ] && echo "  Node.js: https://nodejs.org"
      exit 1
    fi
  fi

  # 最终校验
  if ! command -v cargo >/dev/null 2>&1; then
    _err "cargo 仍不可用。请重新打开终端后再试，或手动安装 Rust: https://rustup.rs"
    exit 1
  fi
}

# ── 仅安装依赖 ────────────────────────────────────────────────────────────────
do_install() {
  _section "  安装依赖"
  source_cargo_env
  load_nvm

  if command -v cargo >/dev/null 2>&1; then
    _ok "Rust 已安装: $(rustc --version)"
  else
    install_rust
    source_cargo_env
  fi

  if command -v npm >/dev/null 2>&1; then
    _ok "Node.js 已安装: $(node --version), npm $(npm --version)"
  else
    install_nodejs
    load_nvm
  fi

  if command -v cargo >/dev/null 2>&1; then
    _ok "Rust: $(rustc --version)"
  else
    _err "Rust 安装后仍不可用，请重新打开终端后运行 ./start.sh"
    exit 1
  fi
  if command -v npm >/dev/null 2>&1; then
    _ok "Node.js: $(node --version), npm $(npm --version)"
  else
    _warn "npm 安装后需重新打开终端，或运行: source ~/.nvm/nvm.sh"
  fi

  _ok "依赖安装完成！"
}

# ── 前端构建 ─────────────────────────────────────────────────────────────────
build_frontend() {
  if [ -f "web/dist/index.html" ]; then
    _ok "前端已构建，跳过 (web/dist/index.html 存在)"
    return 0
  fi
  load_nvm
  if command -v npm >/dev/null 2>&1; then
    _info "构建前端 (web/) ..."
    ( cd web && { [ -d node_modules ] || npm install; } && npm run build )
    _ok "前端构建完成"
  else
    _warn "未找到 npm，跳过前端构建 — 面板 UI 将不可用（API 仍可用）"
    _warn "运行 ./start.sh install 可自动安装 Node.js"
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

  if [ ! -x "$BIN" ]; then
    if [ -f "$BIN" ]; then
      chmod +x "$BIN"
    else
      _err "找不到可执行文件: $BIN"
      _err "请先运行构建: ./start.sh build"
      exit 1
    fi
  fi

  mkdir -p data
  show_config
  echo -e "  ${CYAN}Ctrl+C 停止  |  日志: tail -f ${PANEL_LOG_FILE}${NC}\n"

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
    if command -v curl >/dev/null 2>&1; then
      STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://${HOST}:${PORT}/api/healthz" 2>/dev/null || echo "000")
      [ "$STATUS" = "200" ] && _ok "HTTP 健康检查: OK (http://${HOST}:${PORT})" \
                              || _warn "HTTP 健康检查: $STATUS"
    fi
  else
    _warn "未运行"
  fi

  # 显示已编译的二进制信息
  source_cargo_env
  echo ""
  if [ -f "$BIN" ]; then
    _ok "二进制存在: $BIN ($(du -sh "$BIN" 2>/dev/null | cut -f1 || echo '?'))"
  else
    _warn "二进制不存在: $BIN (需要构建)"
    [ -d "target/release/panel-server" ] || \
    [ -f "target/release/panel-server" ] && _info "  Release 构建存在: target/release/panel-server"
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
  # 首次进入菜单时自动加载环境
  source_cargo_env
  load_nvm

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

    # 依赖状态
    RUST_OK=""; NODE_OK=""
    command -v cargo >/dev/null 2>&1 && RUST_OK="${GREEN}✓${NC}" || RUST_OK="${RED}✗ (未安装)${NC}"
    command -v npm   >/dev/null 2>&1 && NODE_OK="${GREEN}✓${NC}" || NODE_OK="${YELLOW}✗ (未安装，前端不可用)${NC}"
    echo -e "  Rust: ${RUST_OK}  Node.js: ${NODE_OK}"
    echo ""
    echo    "  ┌───────────────────────────────────────────┐"
    echo    "  │  1) 前台启动（日志直接输出）               │"
    echo    "  │  2) 后台启动（日志写入文件）               │"
    echo    "  │  3) 停止服务                               │"
    echo    "  │  4) 重启服务                               │"
    echo    "  │  5) 查看状态                               │"
    echo    "  │  6) 配置向导（写/更新 .env）               │"
    echo    "  │  7) 查看日志（tail -f）                    │"
    echo    "  │  8) 仅构建（不启动）                       │"
    echo    "  │  9) 安装依赖（Rust + Node.js）             │"
    echo    "  │  0) 退出                                   │"
    echo    "  └───────────────────────────────────────────┘"
    echo ""
    printf  "  请选择 [0-9]: "
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
      9)
        do_install
        source_cargo_env; load_nvm
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
  install) do_install ;;
  build)
    check_deps
    build_frontend
    build_backend
    ;;
  logs)    do_logs   ;;
  menu|"") do_menu   ;;
esac
