# =============================================================================
# proxy-panel 一键启动 (Windows / PowerShell)
#
#   .\start.ps1                # 构建(如需)+ 启动
#   .\start.ps1 -Release       # release 模式构建
#   .\start.ps1 -SkipBuild     # 跳过构建,直接跑已编译二进制
#
# 可用环境变量覆盖默认值:
#   PANEL_BIND / DATABASE_URL / PANEL_PUBLIC_HOST / PANEL_ADMIN_PASSWORD /
#   PANEL_REMOTE_MODE  (同 start.sh)
# =============================================================================
param(
    [switch]$Release,
    [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

# cargo / rustup 可能装在 ~\.cargo\bin 但不在 PATH
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}

# --- 默认环境变量 ----------------------------------------------------------
if (-not $env:PANEL_BIND)        { $env:PANEL_BIND = "127.0.0.1:8080" }
if (-not $env:DATABASE_URL)      { $env:DATABASE_URL = "sqlite://./data/panel.db" }
if (-not $env:PANEL_REMOTE_MODE) { $env:PANEL_REMOTE_MODE = "dry-run" }
if (-not $env:RUST_LOG)          { $env:RUST_LOG = "info,sqlx=warn" }

# --- 工具链检查 ------------------------------------------------------------
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "✗ 未找到 cargo,请先安装 Rust: https://rustup.rs" -ForegroundColor Red
    exit 1
}

# --- 构建前端 (只在 dist 缺失时) -------------------------------------------
if (-not $SkipBuild -and -not (Test-Path "web\dist\index.html")) {
    if (Get-Command npm -ErrorAction SilentlyContinue) {
        Write-Host "▶ 构建前端 (web/) ..." -ForegroundColor Cyan
        Push-Location web
        if (-not (Test-Path node_modules)) { npm install }
        npm run build
        Pop-Location
    } else {
        Write-Host "⚠ 未找到 npm,跳过前端构建 —— 面板 UI 将不可用(API 仍可用)。" -ForegroundColor Yellow
    }
}

# --- 构建后端 --------------------------------------------------------------
$bin = "target\debug\panel-server.exe"
$buildArgs = @("build", "-p", "panel-server")
if ($Release) { $bin = "target\release\panel-server.exe"; $buildArgs += "--release" }
if (-not $SkipBuild) {
    Write-Host "▶ 构建后端 (cargo $($buildArgs -join ' ')) ..." -ForegroundColor Cyan
    & cargo @buildArgs
}
if (-not (Test-Path $bin)) {
    Write-Host "✗ 找不到二进制 $bin,请先构建。" -ForegroundColor Red
    exit 1
}

if (-not (Test-Path data)) { New-Item -ItemType Directory data | Out-Null }

# --- 基本信息 --------------------------------------------------------------
$parts = $env:PANEL_BIND.Split(":")
$h = $parts[0]; $port = $parts[1]
$displayHost = if ($h -eq "0.0.0.0") { "127.0.0.1" } else { $h }
$firstRun = $false
if ($env:DATABASE_URL -like "sqlite://*") {
    $dbfile = $env:DATABASE_URL.Substring("sqlite://".Length)
    $firstRun = -not (Test-Path $dbfile)
}

Write-Host ""
Write-Host "  🌸 proxy-panel" -ForegroundColor Magenta
Write-Host "  ─────────────────────────────────────────────"
Write-Host "   面板地址   http://${displayHost}:${port}"
Write-Host "   数据库     $($env:DATABASE_URL)"
Write-Host "   远程模式   $($env:PANEL_REMOTE_MODE)"
Write-Host "   构建模式   $(if ($Release) { 'release' } else { 'dev' })"
if ($firstRun) {
    if ($env:PANEL_ADMIN_PASSWORD) {
        Write-Host "   管理员     admin / (来自 PANEL_ADMIN_PASSWORD)"
    } else {
        Write-Host "   管理员     admin / (首次启动自动生成,见下方日志)"
    }
} else {
    Write-Host "   管理员     admin / (已存在,沿用原密码)"
}
Write-Host "  ─────────────────────────────────────────────"
Write-Host "   Ctrl+C 停止"
Write-Host ""

& $bin
