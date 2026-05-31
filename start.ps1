# =============================================================================
#  proxy-panel 启动管理脚本 (Windows / PowerShell 5.1+)
#
#  用法:
#    .\start.ps1              交互菜单（默认）
#    .\start.ps1 start        直接启动（读 .env）
#    .\start.ps1 stop         停止后台服务
#    .\start.ps1 restart      重启
#    .\start.ps1 status       查看状态
#    .\start.ps1 config       配置向导（写/更新 .env）
#    .\start.ps1 build        仅构建
#    .\start.ps1 logs         查看日志（Get-Content -Wait）
#    .\start.ps1 -Release     release 模式构建
# =============================================================================
param(
    [Parameter(Position=0)] [string]$Command = "menu",
    [switch]$Release,
    [switch]$SkipBuild
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot

$ENV_FILE      = Join-Path $PSScriptRoot ".env"
$PID_FILE      = Join-Path $PSScriptRoot ".proxy-panel.pid"
$LOG_FILE      = Join-Path $PSScriptRoot "data\panel.log"

# ── cargo 路径修复 ────────────────────────────────────────────────────────────
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}

# ── 颜色辅助 ─────────────────────────────────────────────────────────────────
function Write-Info($msg)    { Write-Host "▶ $msg" -ForegroundColor Cyan }
function Write-Ok($msg)      { Write-Host "✓ $msg" -ForegroundColor Green }
function Write-Warn($msg)    { Write-Host "⚠ $msg" -ForegroundColor Yellow }
function Write-Err($msg)     { Write-Host "✗ $msg" -ForegroundColor Red }
function Write-Section($msg) { Write-Host "`n$msg" -ForegroundColor Magenta }

# ── .env 读取 ─────────────────────────────────────────────────────────────────
function Load-Env {
    if (-not (Test-Path $ENV_FILE)) { return }
    Get-Content $ENV_FILE | ForEach-Object {
        $line = $_.Trim()
        if ($line -match '^#' -or $line -eq '') { return }
        $idx = $line.IndexOf('=')
        if ($idx -le 0) { return }
        $key = $line.Substring(0, $idx).Trim()
        $val = $line.Substring($idx + 1).Trim().Trim('"').Trim("'")
        [System.Environment]::SetEnvironmentVariable($key, $val, 'Process')
    }
}

# ── 默认值 ────────────────────────────────────────────────────────────────────
function Apply-Defaults {
    if (-not $env:PANEL_BIND)             { $env:PANEL_BIND = "127.0.0.1:8080" }
    if (-not $env:DATABASE_URL)           { $env:DATABASE_URL = "sqlite://./data/panel.db" }
    if (-not $env:PANEL_REMOTE_MODE)      { $env:PANEL_REMOTE_MODE = "dry-run" }
    if ($null -eq $env:PANEL_SSH_KEY)     { $env:PANEL_SSH_KEY = "" }
    if ($null -eq $env:PANEL_ADMIN_PASSWORD){ $env:PANEL_ADMIN_PASSWORD = "" }
    if (-not $env:PANEL_COOKIE_SECURE)    { $env:PANEL_COOKIE_SECURE = "0" }
    if (-not $env:RUST_LOG)               { $env:RUST_LOG = "info,sqlx=warn" }
}

Load-Env
Apply-Defaults

# ── 构建设置 ─────────────────────────────────────────────────────────────────
$Profile   = if ($Release) { "release" } else { "dev" }
$BinDir    = "target\$Profile"
$Bin       = "$BinDir\panel-server.exe"
$BuildArgs = @("build", "-p", "panel-server")
if ($Release) { $BuildArgs += "--release" }

# ── 显示当前配置 ─────────────────────────────────────────────────────────────
function Show-Config {
    $parts = $env:PANEL_BIND -split ':'
    $h = $parts[0]; $port = $parts[-1]
    $displayHost = if ($h -eq '0.0.0.0') { '127.0.0.1' } else { $h }

    $firstRun = $false
    if ($env:DATABASE_URL -like 'sqlite://*') {
        $dbfile = $env:DATABASE_URL.Substring('sqlite://'.Length)
        $firstRun = -not (Test-Path $dbfile)
    }

    Write-Host ""
    Write-Host "  🌸  proxy-panel" -ForegroundColor Magenta
    Write-Host "  ─────────────────────────────────────────────────────"
    Write-Host ("  {0,-14} http://{1}:{2}" -f "面板地址", $displayHost, $port)
    Write-Host ("  {0,-14} {1}"            -f "数据库", $env:DATABASE_URL)
    Write-Host ("  {0,-14} {1}"            -f "远程模式", $env:PANEL_REMOTE_MODE)
    Write-Host ("  {0,-14} {1}"            -f "构建模式", $Profile)
    Write-Host ("  {0,-14} {1}"            -f "日志文件", $LOG_FILE)
    if ($env:PANEL_REMOTE_MODE -eq 'ssh' -and $env:PANEL_SSH_KEY) {
        Write-Host ("  {0,-14} {1}"        -f "SSH 密钥", $env:PANEL_SSH_KEY)
    }
    if ($firstRun) {
        $pw = if ($env:PANEL_ADMIN_PASSWORD) { "来自 .env / 环境变量" } else { "首次启动自动生成，见日志" }
        Write-Host ("  {0,-14} admin / ({1})" -f "管理员", $pw)
    } else {
        Write-Host ("  {0,-14} admin / (沿用已有密码)" -f "管理员")
    }
    Write-Host "  ─────────────────────────────────────────────────────"
}

# ── 工具检查 ─────────────────────────────────────────────────────────────────
function Check-Deps {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Err "未找到 cargo，请先安装 Rust: https://rustup.rs"
        exit 1
    }
}

# ── 前端构建 ─────────────────────────────────────────────────────────────────
function Build-Frontend {
    if (Test-Path "web\dist\index.html") { return }
    if (Get-Command npm -ErrorAction SilentlyContinue) {
        Write-Info "构建前端 (web/) ..."
        Push-Location web
        if (-not (Test-Path node_modules)) { npm install }
        npm run build
        Pop-Location
        Write-Ok "前端构建完成"
    } else {
        Write-Warn "未找到 npm，跳过前端构建 — 面板 UI 将不可用（API 仍可用）"
    }
}

# ── 后端构建 ─────────────────────────────────────────────────────────────────
function Build-Backend {
    Write-Info "构建后端 (cargo $($BuildArgs -join ' ')) ..."
    & cargo @BuildArgs
    if ($LASTEXITCODE -ne 0) { Write-Err "构建失败"; exit 1 }
    Write-Ok "后端构建完成 → $Bin"
}

# ── 获取运行 PID ─────────────────────────────────────────────────────────────
function Get-PanelPid {
    if (Test-Path $PID_FILE) {
        $pid = Get-Content $PID_FILE -ErrorAction SilentlyContinue
        if ($pid -and (Get-Process -Id $pid -ErrorAction SilentlyContinue)) { return $pid }
        Remove-Item $PID_FILE -Force -ErrorAction SilentlyContinue
    }
    $proc = Get-Process "panel-server" -ErrorAction SilentlyContinue | Select-Object -First 1
    return $proc?.Id
}

# ── 启动 ─────────────────────────────────────────────────────────────────────
function Do-Start([bool]$background = $false) {
    Check-Deps
    if (-not $SkipBuild) {
        Build-Frontend
        Build-Backend
    }
    if (-not (Test-Path $Bin)) { Write-Err "找不到 $Bin，请先构建"; exit 1 }
    if (-not (Test-Path data)) { New-Item -ItemType Directory data | Out-Null }

    Show-Config

    if ($background) {
        Write-Info "在后台启动，日志 → $LOG_FILE"
        $proc = Start-Process -FilePath $Bin `
                              -RedirectStandardOutput $LOG_FILE `
                              -RedirectStandardError  $LOG_FILE `
                              -WindowStyle Hidden `
                              -PassThru
        $proc.Id | Out-File $PID_FILE -Encoding ascii
        Write-Ok "已在后台启动 (PID $($proc.Id))"
    } else {
        Write-Host "  Ctrl+C 停止  |  日志: Get-Content -Wait '$LOG_FILE'" -ForegroundColor Cyan
        Write-Host ""
        & $Bin
    }
}

# ── 停止 ─────────────────────────────────────────────────────────────────────
function Do-Stop {
    $pid = Get-PanelPid
    if ($pid) {
        try {
            Stop-Process -Id $pid -Force
            Remove-Item $PID_FILE -Force -ErrorAction SilentlyContinue
            Write-Ok "已停止 (PID $pid)"
        } catch {
            Write-Warn "无法停止 PID $pid: $_"
        }
    } else {
        Write-Warn "未找到运行中的 panel-server"
    }
}

# ── 状态 ─────────────────────────────────────────────────────────────────────
function Do-Status {
    $parts = $env:PANEL_BIND -split ':'
    $h = $parts[0]; $port = $parts[-1]
    $displayHost = if ($h -eq '0.0.0.0') { '127.0.0.1' } else { $h }

    $pid = Get-PanelPid
    Write-Host ""
    if ($pid) {
        Write-Ok "运行中  PID=$pid"
        try {
            $r = Invoke-WebRequest "http://${displayHost}:${port}/api/healthz" -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
            if ($r.StatusCode -eq 200) {
                Write-Ok "HTTP 健康检查: OK  http://${displayHost}:${port}"
            }
        } catch {
            Write-Warn "HTTP 健康检查失败（服务可能还在启动）"
        }
    } else {
        Write-Warn "未运行"
    }
    Write-Host ""
}

# ── 配置向导 ─────────────────────────────────────────────────────────────────
function Do-Config {
    Write-Section "  配置向导 — 写入 $ENV_FILE"
    Write-Host "  (直接回车保留当前值)`n"

    function Prompt-Val([string]$key, [string]$desc, [string]$cur, [bool]$secret = $false) {
        $display = if ($secret) { '***' } else { $cur }
        Write-Host -NoNewline ("  {0,-22} [{1}] : " -f $desc, $display)
        if ($secret) {
            $ss = Read-Host -AsSecureString
            $bstr = [System.Runtime.InteropServices.Marshal]::SecureStringToBSTR($ss)
            $plain = [System.Runtime.InteropServices.Marshal]::PtrToStringAuto($bstr)
            [System.Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
            return if ($plain) { $plain } else { $cur }
        }
        $v = Read-Host
        return if ($v) { $v } else { $cur }
    }

    $bind   = Prompt-Val "PANEL_BIND"          "监听地址:端口"            $env:PANEL_BIND
    $db     = Prompt-Val "DATABASE_URL"        "数据库 URL"               $env:DATABASE_URL
    $remote = Prompt-Val "PANEL_REMOTE_MODE"   "远程模式(dry-run/ssh)"    $env:PANEL_REMOTE_MODE
    $sshkey = ""
    if ($remote -eq "ssh") {
        $sshkey = Prompt-Val "PANEL_SSH_KEY"   "SSH 私钥路径"             $env:PANEL_SSH_KEY
    }
    $pw     = Prompt-Val "PANEL_ADMIN_PASSWORD" "管理员密码(留空=自动)"   $env:PANEL_ADMIN_PASSWORD $true
    $secure = Prompt-Val "PANEL_COOKIE_SECURE" "HTTPS Cookie安全(0/1)"   $env:PANEL_COOKIE_SECURE

    $ts = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $content = @"
# proxy-panel 配置文件 — 由 start.ps1 config 生成于 $ts
# 可手动编辑，start.ps1 启动时自动读取

# 监听地址（0.0.0.0:8080 = 所有网卡）
PANEL_BIND=$bind

# 数据库（SQLite 推荐，PostgreSQL: postgres://user:pass@host:5432/db）
DATABASE_URL=$db

# 远程推送模式: dry-run = 模拟不执行; ssh = 真实 SSH 推送
PANEL_REMOTE_MODE=$remote

# SSH 私钥路径（仅 PANEL_REMOTE_MODE=ssh 时有效）
PANEL_SSH_KEY=$sshkey

# 管理员密码（留空 = 首次启动自动生成，打印到日志）
PANEL_ADMIN_PASSWORD=$pw

# 部署在 HTTPS 反向代理后时设为 1（强制 Secure Cookie）
PANEL_COOKIE_SECURE=$secure

# 日志级别
RUST_LOG=info,sqlx=warn
"@
    # 写入 UTF-8 with BOM（确保 PS5.1 能正确读回）
    [System.IO.File]::WriteAllText($ENV_FILE, $content, (New-Object System.Text.UTF8Encoding($true)))
    Write-Ok "配置已写入 $ENV_FILE"
    Write-Host ""
    Get-Content $ENV_FILE
    Write-Host ""
}

# ── 日志 ─────────────────────────────────────────────────────────────────────
function Do-Logs {
    if (-not (Test-Path (Split-Path $LOG_FILE))) {
        New-Item -ItemType Directory (Split-Path $LOG_FILE) | Out-Null
    }
    if (Test-Path $LOG_FILE) {
        Get-Content $LOG_FILE -Wait
    } else {
        Write-Warn "日志文件不存在: $LOG_FILE"
        Write-Info "后台启动时日志写入该文件；前台启动时日志直接输出到控制台"
    }
}

# ── 交互菜单 ─────────────────────────────────────────────────────────────────
function Do-Menu {
    while ($true) {
        Clear-Host
        Write-Host @"

  ██████╗ ██████╗  ██████╗ ██╗  ██╗██╗   ██╗
  ██╔══██╗██╔══██╗██╔═══██╗╚██╗██╔╝╚██╗ ██╔╝
  ██████╔╝██████╔╝██║   ██║ ╚███╔╝  ╚████╔╝
  ██╔═══╝ ██╔══██╗██║   ██║ ██╔██╗   ╚██╔╝
  ██║     ██║  ██║╚██████╔╝██╔╝ ██╗   ██║
  ╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═╝   ╚═╝   PANEL
"@ -ForegroundColor Magenta

        $pid = Get-PanelPid
        if ($pid) {
            Write-Host "  状态: " -NoNewline; Write-Host "● 运行中  PID=$pid" -ForegroundColor Green
        } else {
            Write-Host "  状态: " -NoNewline; Write-Host "○ 未运行" -ForegroundColor Red
        }
        $envStatus = if (Test-Path $ENV_FILE) { "已有 .env" } else { "无 .env（使用默认值）" }
        Write-Host "  配置: $envStatus"
        Write-Host ""
        Write-Host "  ┌─────────────────────────────────────┐"
        Write-Host "  │  1) 前台启动（日志直接输出）          │"
        Write-Host "  │  2) 后台启动（日志写入文件）          │"
        Write-Host "  │  3) 停止服务                         │"
        Write-Host "  │  4) 重启服务                         │"
        Write-Host "  │  5) 查看状态                         │"
        Write-Host "  │  6) 配置向导（写/更新 .env）          │"
        Write-Host "  │  7) 查看日志（实时 tail）             │"
        Write-Host "  │  8) 仅构建（不启动）                  │"
        Write-Host "  │  0) 退出                             │"
        Write-Host "  └─────────────────────────────────────┘"
        Write-Host ""
        $choice = Read-Host "  请选择 [0-8]"

        switch ($choice) {
            "1" {
                Load-Env; Apply-Defaults
                Do-Start $false
            }
            "2" {
                Load-Env; Apply-Defaults
                Do-Start $true
                Read-Host "  按 Enter 继续"
            }
            "3" {
                Do-Stop
                Read-Host "  按 Enter 继续"
            }
            "4" {
                Do-Stop; Start-Sleep 1
                Load-Env; Apply-Defaults
                Do-Start $true
                Read-Host "  按 Enter 继续"
            }
            "5" {
                Do-Status
                Read-Host "  按 Enter 继续"
            }
            "6" {
                Do-Config
                Load-Env; Apply-Defaults
                Read-Host "  按 Enter 继续"
            }
            "7" { Do-Logs }
            "8" {
                Check-Deps; Build-Frontend; Build-Backend
                Read-Host "  按 Enter 继续"
            }
            { $_ -in "0","q","Q" } {
                Write-Host ""
                Write-Ok "再见！"
                exit 0
            }
            default {
                Write-Warn "无效选项: $choice"
                Start-Sleep 1
            }
        }
    }
}

# ── 路由 ─────────────────────────────────────────────────────────────────────
switch ($Command.ToLower()) {
    "start"   { Load-Env; Apply-Defaults; Do-Start $false }
    "stop"    { Do-Stop }
    "restart" { Do-Stop; Start-Sleep 1; Load-Env; Apply-Defaults; Do-Start $true }
    "status"  { Do-Status }
    "config"  { Do-Config }
    "build"   { Check-Deps; Build-Frontend; Build-Backend }
    "logs"    { Do-Logs }
    default   { Do-Menu }
}
