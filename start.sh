#!/usr/bin/env bash
# =============================================================================
# proxy-panel 一键启动 (Linux / macOS)
#
#   ./start.sh                # 构建(如需)+ 启动
#   ./start.sh --release      # 用 release 模式构建(更快的运行时,首次编译较慢)
#   ./start.sh --skip-build   # 跳过构建,直接跑已编译的二进制
#
# 可用环境变量覆盖默认值:
#   PANEL_BIND            监听地址      (默认 127.0.0.1:8080)
#   DATABASE_URL          数据库        (默认 sqlite://./data/panel.db)
#   PANEL_PUBLIC_HOST     订阅 URI 主机  (默认取 bind 的 host)
#   PANEL_ADMIN_PASSWORD  首次启动的管理员密码(留空则自动生成并打印)
#   PANEL_REMOTE_MODE     dry-run | ssh (默认 dry-run)
# =============================================================================
set -euo pipefail
cd "$(dirname "$0")"

PROFILE="dev"
SKIP_BUILD=0
for arg in "$@"; do
  case "$arg" in
    --release)    PROFILE="release" ;;
    --skip-build) SKIP_BUILD=1 ;;
    *) echo "未知参数: $arg"; exit 2 ;;
  esac
done

# --- 默认环境变量 ----------------------------------------------------------
export PANEL_BIND="${PANEL_BIND:-127.0.0.1:8080}"
export DATABASE_URL="${DATABASE_URL:-sqlite://./data/panel.db}"
export PANEL_REMOTE_MODE="${PANEL_REMOTE_MODE:-dry-run}"
export RUST_LOG="${RUST_LOG:-info,sqlx=warn}"

# --- 工具链检查 ------------------------------------------------------------
command -v cargo >/dev/null 2>&1 || { echo "✗ 未找到 cargo,请先安装 Rust: https://rustup.rs"; exit 1; }

# --- 构建前端 (只在 dist 缺失时) -------------------------------------------
if [ "$SKIP_BUILD" -eq 0 ] && [ ! -f web/dist/index.html ]; then
  if command -v npm >/dev/null 2>&1; then
    echo "▶ 构建前端 (web/) ..."
    ( cd web && { [ -d node_modules ] || npm install; } && npm run build )
  else
    echo "⚠ 未找到 npm,跳过前端构建 —— 面板 UI 将不可用(API 仍可用)。"
  fi
fi

# --- 构建后端 --------------------------------------------------------------
BIN="target/debug/panel-server"
BUILD_FLAGS=""
if [ "$PROFILE" = "release" ]; then BIN="target/release/panel-server"; BUILD_FLAGS="--release"; fi
if [ "$SKIP_BUILD" -eq 0 ]; then
  echo "▶ 构建后端 (cargo build $BUILD_FLAGS) ..."
  cargo build $BUILD_FLAGS -p panel-server
fi
[ -x "$BIN" ] || { echo "✗ 找不到二进制 $BIN,请先构建。"; exit 1; }

mkdir -p data

# --- 基本信息 --------------------------------------------------------------
HOST="${PANEL_BIND%%:*}"; PORT="${PANEL_BIND##*:}"
[ "$HOST" = "0.0.0.0" ] && DISPLAY_HOST="127.0.0.1" || DISPLAY_HOST="$HOST"
FIRST_RUN=0
case "$DATABASE_URL" in sqlite://*) DBFILE="${DATABASE_URL#sqlite://}"; [ -f "$DBFILE" ] || FIRST_RUN=1 ;; esac

cat <<EOF

  🌸 proxy-panel
  ─────────────────────────────────────────────
   面板地址   http://${DISPLAY_HOST}:${PORT}
   数据库     ${DATABASE_URL}
   远程模式   ${PANEL_REMOTE_MODE}
   构建模式   ${PROFILE}
EOF
if [ "$FIRST_RUN" -eq 1 ]; then
  if [ -n "${PANEL_ADMIN_PASSWORD:-}" ]; then
    echo "   管理员     admin / (来自 PANEL_ADMIN_PASSWORD)"
  else
    echo "   管理员     admin / (首次启动自动生成,见下方日志)"
  fi
else
  echo "   管理员     admin / (已存在,沿用原密码)"
fi
echo "  ─────────────────────────────────────────────"
echo "   Ctrl+C 停止"
echo ""

exec "$BIN"
