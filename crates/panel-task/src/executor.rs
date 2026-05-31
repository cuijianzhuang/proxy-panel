//! Per-task execution logic. The worker hands off here once a task is claimed.

use std::sync::Arc;

use panel_core::{CoreAdapter, InboundContext, NodeConfigContext};
use panel_core_singbox::SingBoxAdapter;
use panel_core_xray::XrayAdapter;
use panel_domain::{ChainProxyRepo, CoreKind, ListenerRepo, NodeRepo, ProxyUserRepo};
use panel_node::NodeRemote;

use crate::model::{Task, TaskKind};
use crate::repo::TaskRepo;

/// Minimal adapter registry the executor needs. Same shape as the panel-server
/// registry, kept here to avoid the executor depending on the server crate.
#[derive(Clone, Default)]
pub struct Adapters {
    xray:    Arc<XrayAdapter>,
    singbox: Arc<SingBoxAdapter>,
}

impl Adapters {
    pub fn new() -> Self {
        Self::default()
    }
    fn for_core(&self, kind: CoreKind) -> &dyn CoreAdapter {
        match kind {
            CoreKind::Xray => self.xray.as_ref(),
            CoreKind::Singbox => self.singbox.as_ref(),
        }
    }
    fn config_path(kind: CoreKind) -> &'static str {
        match kind {
            CoreKind::Xray => "/etc/xray/config.json",
            CoreKind::Singbox => "/etc/sing-box/config.json",
        }
    }
    fn systemd_unit(kind: CoreKind) -> &'static str {
        match kind {
            CoreKind::Xray => "xray",
            CoreKind::Singbox => "sing-box",
        }
    }
}

/// All collaborators a task execution needs.
#[derive(Clone)]
pub struct ExecCtx {
    pub tasks:         TaskRepo,
    pub nodes:         NodeRepo,
    pub listeners:     ListenerRepo,
    pub proxy_users:   ProxyUserRepo,
    pub chain_proxies: ChainProxyRepo,
    pub adapters:      Adapters,
    pub remote:        Arc<dyn NodeRemote>,
}

/// Run one task. Returns `Ok(())` on success, `Err(msg)` on failure — caller
/// is responsible for updating the task status row.
pub async fn run_task(ctx: &ExecCtx, task: &Task) -> Result<(), String> {
    match task.kind {
        TaskKind::ApplyConfig => apply_config(ctx, task).await,
        TaskKind::Restart     => restart(ctx, task).await,
        TaskKind::CheckHealth => check_health(ctx, task).await,
        TaskKind::Provision   => provision(ctx, task).await,
    }
}

// ---------------------------------------------------------------------------
// apply_config: render → upload → reload
// ---------------------------------------------------------------------------

async fn apply_config(ctx: &ExecCtx, task: &Task) -> Result<(), String> {
    let log = |line: String| {
        let repo = ctx.tasks.clone();
        let id = task.id;
        async move {
            let _ = repo.append_log(id, &line).await;
        }
    };

    log(format!("[apply_config] task {} start", task.id)).await;

    let node = ctx
        .nodes
        .find(task.node_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("node {} not found", task.node_id))?;

    log(format!("[apply_config] node = {} ({}:{})", node.name, node.addr, node.ssh_port)).await;

    // Render the full config from current DB state.
    let listeners = ctx
        .listeners
        .list_for_node(node.id)
        .await
        .map_err(|e| e.to_string())?;
    let mut owned_clients = Vec::with_capacity(listeners.len());
    for l in &listeners {
        let clients = ctx
            .proxy_users
            .list_for_listener(l.id)
            .await
            .map_err(|e| e.to_string())?;
        owned_clients.push(clients);
    }

    // Resolve chain proxies exactly like /api/nodes/:id/config does, so the
    // config we actually push to the VPS matches the panel's preview. Pull the
    // enabled pool once, then map each listener's params.chain_proxy_id.
    let chain_pool = ctx.chain_proxies.list_enabled().await.unwrap_or_default();
    let resolve_chain = |l: &panel_domain::Listener| -> Option<&panel_domain::ChainProxy> {
        let id = l.params.get("chain_proxy_id")?.as_i64()?;
        chain_pool.iter().find(|c| c.id == id)
    };

    let inbounds: Vec<InboundContext> = listeners
        .iter()
        .zip(owned_clients.iter())
        .map(|(l, c)| InboundContext {
            listener: l,
            clients:  c.as_slice(),
            chain:    resolve_chain(l),
        })
        .collect();

    let adapter = ctx.adapters.for_core(node.core);
    let config = adapter
        .render_node_config(&NodeConfigContext {
            node:     &node,
            inbounds: &inbounds,
        })
        .map_err(|e| e.to_string())?;
    let bytes = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
    log(format!(
        "[apply_config] rendered config: {} inbounds, {} bytes",
        listeners.len(),
        bytes.len()
    ))
    .await;

    // Reachability check before doing anything destructive.
    let ident = ctx.remote.ping(&node).await.map_err(|e| e.to_string())?;
    log(format!("[ssh] {}", ident)).await;

    // TOFU host-key pinning: if this node had no pinned fingerprint and the
    // connect just observed one, persist it now. From here on, a changed host
    // key makes `connect` fail (MITM / reinstall protection).
    if node.ssh_host_fingerprint.is_none() {
        if let Some(fp) = ctx.remote.observed_host_key(node.id).await {
            if ctx.nodes.set_host_fingerprint(node.id, &fp).await.is_ok() {
                log(format!("[ssh] pinned host key fingerprint SHA256:{fp}")).await;
            }
        }
    }

    let path = Adapters::config_path(node.core);
    log(format!("[ssh] upload → {}", path)).await;
    ctx.remote
        .write_file(&node, path, &bytes)
        .await
        .map_err(|e| e.to_string())?;

    let unit = Adapters::systemd_unit(node.core);
    let cmd = format!("systemctl reload {unit} || systemctl restart {unit}");
    log(format!("[ssh] exec → {}", cmd)).await;
    let out = ctx.remote.exec(&node, &cmd).await.map_err(|e| e.to_string())?;
    if !out.stdout.is_empty() {
        log(format!("[ssh stdout] {}", out.stdout)).await;
    }
    if !out.stderr.is_empty() {
        log(format!("[ssh stderr] {}", out.stderr)).await;
    }
    if !out.success() {
        return Err(format!("reload exited {}", out.exit_code));
    }

    log("[apply_config] done".to_string()).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// restart
// ---------------------------------------------------------------------------

async fn restart(ctx: &ExecCtx, task: &Task) -> Result<(), String> {
    let node = ctx
        .nodes
        .find(task.node_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("node {} not found", task.node_id))?;
    let unit = Adapters::systemd_unit(node.core);
    let cmd = format!("systemctl restart {unit}");
    ctx.tasks.append_log(task.id, &format!("[ssh] exec → {}", cmd)).await.ok();
    let out = ctx.remote.exec(&node, &cmd).await.map_err(|e| e.to_string())?;
    if !out.success() {
        return Err(format!("restart exited {}", out.exit_code));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// check_health
// ---------------------------------------------------------------------------

async fn check_health(ctx: &ExecCtx, task: &Task) -> Result<(), String> {
    let node = ctx
        .nodes
        .find(task.node_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("node {} not found", task.node_id))?;
    let ident = ctx.remote.ping(&node).await.map_err(|e| e.to_string())?;
    ctx.tasks.append_log(task.id, &format!("[ssh] {}", ident)).await.ok();
    let unit = Adapters::systemd_unit(node.core);
    let cmd = format!("systemctl is-active {unit}");
    let out = ctx.remote.exec(&node, &cmd).await.map_err(|e| e.to_string())?;
    ctx.tasks
        .append_log(task.id, &format!("[ssh] {} → {}", cmd, out.stdout.trim()))
        .await
        .ok();
    if !out.success() {
        return Err("service not active".into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// provision: install core + systemd service + write config + start
// ---------------------------------------------------------------------------
//
// Execution is split into 5 short SSH exec calls so each phase has a clean
// log line — avoids dumping the entire install script into the log.
//
// Phase 1: pre-flight  — root check, OS/arch detection, curl/wget check
// Phase 2: install     — idempotent install of the core binary
// Phase 3: service     — create/enable systemd unit
// Phase 4: config      — render + upload config.json (same as apply_config)
// Phase 5: start       — restart + verify service is active
//
// Both install scripts:
//   • detect architecture (x86_64 / aarch64 / armv7l)
//   • detect package manager (apt / dnf / yum) as an install fallback
//   • try the official install script first, then GitHub release as a fallback
//   • skip if binary already present (idempotent, safe to re-run)
//   • use --connect-timeout / --max-time on curl to avoid hanging forever

async fn provision(ctx: &ExecCtx, task: &Task) -> Result<(), String> {
    macro_rules! plog {
        ($log:expr, $($arg:tt)*) => {
            $log(format!($($arg)*)).await
        };
    }

    let log = |line: String| {
        let repo = ctx.tasks.clone();
        let id = task.id;
        async move { let _ = repo.append_log(id, &line).await; }
    };

    plog!(log, "[provision] task {} start", task.id);

    let node = ctx.nodes.find(task.node_id).await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("node {} not found", task.node_id))?;

    let core_name = match node.core {
        panel_domain::CoreKind::Xray    => "xray",
        panel_domain::CoreKind::Singbox => "sing-box",
    };
    plog!(log, "[provision] node={} ({}:{})  core={}", node.name, node.addr, node.ssh_port, core_name);

    // ---- phase 0: reachability + TOFU pinning ----
    plog!(log, "[phase 0/5] SSH reachability check...");
    let ident = ctx.remote.ping(&node).await
        .map_err(|e| format!("[phase 0/5] SSH failed: {e}"))?;
    plog!(log, "[phase 0/5] connected: {}", ident.trim());

    if node.ssh_host_fingerprint.is_none() {
        if let Some(fp) = ctx.remote.observed_host_key(node.id).await {
            if ctx.nodes.set_host_fingerprint(node.id, &fp).await.is_ok() {
                plog!(log, "[phase 0/5] host key pinned SHA256:{}", fp);
            }
        }
    }

    // ---- phase 1: pre-flight + auto-install env ----
    plog!(log, "[phase 1/5] pre-flight + environment setup...");
    let preflight = r#"#!/bin/bash
set -e

# ── Root check ────────────────────────────────────────────────────
[ "$(id -u)" -eq 0 ] || { echo "ERROR: must run as root (try sudo -i)"; exit 1; }

# ── System info ───────────────────────────────────────────────────
echo "OS:     $(. /etc/os-release 2>/dev/null && echo "$PRETTY_NAME" || uname -s)"
echo "Kernel: $(uname -r)"
echo "Arch:   $(uname -m)"

# ── Package manager detection ─────────────────────────────────────
PKG_MGR=""
if   command -v apt-get  >/dev/null 2>&1; then PKG_MGR="apt"
elif command -v dnf      >/dev/null 2>&1; then PKG_MGR="dnf"
elif command -v yum      >/dev/null 2>&1; then PKG_MGR="yum"
elif command -v pacman   >/dev/null 2>&1; then PKG_MGR="pacman"
elif command -v apk      >/dev/null 2>&1; then PKG_MGR="apk"
fi
echo "PackageManager: ${PKG_MGR:-none}"

# ── Auto-install missing tools ────────────────────────────────────
_need() {
    local pkg="$1" cmd="${2:-$1}"
    command -v "$cmd" >/dev/null 2>&1 && return 0
    echo "  installing $pkg via $PKG_MGR..."
    case "$PKG_MGR" in
        apt)    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq "$pkg" 2>&1 | tail -3;;
        dnf)    dnf install -y -q "$pkg" 2>&1 | tail -3;;
        yum)    yum install -y -q "$pkg" 2>&1 | tail -3;;
        pacman) pacman -Sy --noconfirm --quiet "$pkg" 2>&1 | tail -3;;
        apk)    apk add -q "$pkg" 2>&1 | tail -3;;
        *)      echo "  WARN: no package manager found, $cmd may be missing";;
    esac
}

# Update package index once (apt only, silently)
if [ "$PKG_MGR" = "apt" ]; then
    apt-get update -qq 2>/dev/null || true
fi

# Core tools
_need curl
_need wget
_need unzip
_need tar
_need ca-certificates ca-update-extract    # Debian/Ubuntu alias
command -v ca-update-extract >/dev/null 2>&1 || _need ca-certificates update-ca-certificates

echo "  curl:  $(command -v curl  2>/dev/null || echo 'missing')"
echo "  wget:  $(command -v wget  2>/dev/null || echo 'missing')"
echo "  unzip: $(command -v unzip 2>/dev/null || echo 'missing')"

# Confirm at least one downloader
if ! command -v curl >/dev/null 2>&1 && ! command -v wget >/dev/null 2>&1; then
    echo "ERROR: could not install curl or wget"; exit 1
fi

# ── Systemd check ─────────────────────────────────────────────────
if ! command -v systemctl >/dev/null 2>&1; then
    echo "ERROR: systemctl not found — is this a systemd system?"; exit 1
fi
echo "Systemd: $(systemctl --version | head -1)"

# ── Kernel tuning ────────────────────────────────────────────────
# Apply performance settings persistently (idempotent — same values overwrite).
cat > /etc/sysctl.d/99-proxy-panel.conf << 'SYSCTL'
# Socket backlog
net.core.somaxconn = 32768
net.ipv4.tcp_max_syn_backlog = 32768
# File descriptor limits
fs.file-max = 1048576
# TIME_WAIT recycling (disabled by default on modern kernels, enabling is safe for servers)
net.ipv4.tcp_tw_reuse = 1
# TCP fast open
net.ipv4.tcp_fastopen = 3
# BBR congestion control (requires kernel >= 4.9)
net.core.default_qdisc = fq
net.ipv4.tcp_congestion_control = bbr
SYSCTL
sysctl -p /etc/sysctl.d/99-proxy-panel.conf 2>&1 | grep -v "^sysctl:" || true
echo "Kernel tuning: applied"

# ── ulimits ──────────────────────────────────────────────────────
# Set system-wide open-file limit so the service inherits a large value.
grep -q "DefaultLimitNOFILE" /etc/systemd/system.conf 2>/dev/null || true
if ! grep -q "^DefaultLimitNOFILE=1048576" /etc/systemd/system.conf 2>/dev/null; then
    sed -i 's/^#DefaultLimitNOFILE=.*//' /etc/systemd/system.conf 2>/dev/null || true
    echo "DefaultLimitNOFILE=1048576" >> /etc/systemd/system.conf
    echo "ulimits: set DefaultLimitNOFILE=1048576"
fi

echo "pre-flight: OK"
"#;
    let out = ctx.remote.exec(&node, preflight.trim()).await.map_err(|e| e.to_string())?;
    for l in out.stdout.lines() { plog!(log, "  {}", l); }
    if !out.stderr.is_empty() { plog!(log, "[stderr] {}", out.stderr.trim()); }
    if !out.success() {
        return Err(format!("pre-flight failed (exit {}): {}", out.exit_code, out.stderr.trim()));
    }

    // ---- phase 2: install binary ----
    plog!(log, "[phase 2/5] installing {}...", core_name);

    let install_script: &str = match node.core {
        panel_domain::CoreKind::Xray => r#"#!/bin/bash
set -euo pipefail

XRAY_BIN=/usr/local/bin/xray
INSTALL_SCRIPT_URL="https://github.com/XTLS/Xray-install/raw/main/install-release.sh"

# ---- arch mapping ----
_arch() {
    case "$(uname -m)" in
        x86_64)         echo 64;;
        aarch64|arm64)  echo arm64-v8a;;
        armv7l)         echo arm32-v7a;;
        *)              echo "unsupported arch: $(uname -m)" >&2; exit 1;;
    esac
}

# ---- download helper (curl → wget fallback) ----
_dl() {
    local url="$1" out="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --connect-timeout 15 --max-time 120 -o "$out" "$url"
    else
        wget -qO "$out" --timeout=120 "$url"
    fi
}

if [ -f "$XRAY_BIN" ]; then
    echo "already installed: $("$XRAY_BIN" version 2>/dev/null | head -1 || echo '(version check failed)')"
    exit 0
fi

echo "xray not found, installing..."

# Strategy 1: official installer script
if _dl "$INSTALL_SCRIPT_URL" /tmp/xray-install.sh 2>/dev/null; then
    echo "using official install script..."
    bash /tmp/xray-install.sh install
    rm -f /tmp/xray-install.sh
fi

# Strategy 2: direct GitHub release download
if [ ! -f "$XRAY_BIN" ]; then
    echo "official script unavailable, downloading release binary..."
    ARCH=$(_arch)
    API_URL="https://api.github.com/repos/XTLS/Xray-core/releases/latest"
    TAG=$(_dl "$API_URL" - 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/' || echo "")
    if [ -z "$TAG" ]; then
        echo "could not determine latest version, trying v25.5.0 ..."
        TAG="25.5.0"
    fi
    ZIPFILE="Xray-linux-${ARCH}.zip"
    DOWNLOAD_URL="https://github.com/XTLS/Xray-core/releases/download/v${TAG}/${ZIPFILE}"
    echo "downloading: $DOWNLOAD_URL"
    _dl "$DOWNLOAD_URL" "/tmp/${ZIPFILE}"
    command -v unzip >/dev/null 2>&1 || apt-get install -y unzip -qq 2>/dev/null || yum install -y unzip -q 2>/dev/null
    mkdir -p /tmp/xray-extract
    unzip -qo "/tmp/${ZIPFILE}" -d /tmp/xray-extract/
    install -m 755 /tmp/xray-extract/xray "$XRAY_BIN"
    # install geoip + geosite if bundled
    [ -f /tmp/xray-extract/geoip.dat ]   && install -m 644 /tmp/xray-extract/geoip.dat   /usr/local/share/xray/ 2>/dev/null || true
    [ -f /tmp/xray-extract/geosite.dat ] && install -m 644 /tmp/xray-extract/geosite.dat /usr/local/share/xray/ 2>/dev/null || true
    rm -rf /tmp/xray-extract "/tmp/${ZIPFILE}"
fi

if [ ! -f "$XRAY_BIN" ]; then
    echo "ERROR: all install strategies failed" >&2
    exit 1
fi

echo "installed: $("$XRAY_BIN" version | head -1)"
"#,
        panel_domain::CoreKind::Singbox => r#"#!/bin/bash
set -euo pipefail

SBOX_BIN=/usr/local/bin/sing-box
OFFICIAL_SCRIPT="https://sing-box.app/install.sh"

# ---- arch mapping ----
_arch() {
    case "$(uname -m)" in
        x86_64)         echo amd64;;
        aarch64|arm64)  echo arm64;;
        armv7l)         echo armv7;;
        *)              echo "unsupported arch: $(uname -m)" >&2; exit 1;;
    esac
}

# ---- download helper ----
_dl() {
    local url="$1" out="$2"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --connect-timeout 15 --max-time 120 -o "$out" "$url"
    else
        wget -qO "$out" --timeout=120 "$url"
    fi
}

if [ -f "$SBOX_BIN" ]; then
    echo "already installed: $("$SBOX_BIN" version 2>/dev/null | head -1 || echo '(version check failed)')"
    exit 0
fi

echo "sing-box not found, installing..."

# Strategy 1: official install script
if _dl "$OFFICIAL_SCRIPT" /tmp/sb-install.sh 2>/dev/null; then
    echo "using official install script..."
    bash /tmp/sb-install.sh
    rm -f /tmp/sb-install.sh
fi

# Strategy 2: apt/dnf package (Debian/Ubuntu/Fedora)
if [ ! -f "$SBOX_BIN" ]; then
    if command -v apt-get >/dev/null 2>&1; then
        echo "trying apt-get install sing-box..."
        # SagerNet APT repo
        if ! dpkg -l sing-box >/dev/null 2>&1; then
            _dl "https://pkg.sagernet.org/gpg.key" /tmp/sagernet.asc || true
            if [ -f /tmp/sagernet.asc ]; then
                install -D -m 644 /tmp/sagernet.asc /etc/apt/keyrings/sagernet.asc
                echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/sagernet.asc] https://pkg.sagernet.org/debian/ * *" \
                    > /etc/apt/sources.list.d/sagernet.sources
                apt-get update -qq && apt-get install -y -qq sing-box || true
                rm -f /tmp/sagernet.asc
            fi
        fi
    fi
fi

# Strategy 3: direct GitHub release
if [ ! -f "$SBOX_BIN" ]; then
    echo "trying GitHub release binary..."
    ARCH=$(_arch)
    API_URL="https://api.github.com/repos/SagerNet/sing-box/releases/latest"
    VER=$(_dl "$API_URL" - 2>/dev/null | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/' || echo "")
    if [ -z "$VER" ]; then
        echo "could not determine latest version, trying 1.11.0 ..."
        VER="1.11.0"
    fi
    TARBALL="sing-box-${VER}-linux-${ARCH}.tar.gz"
    DOWNLOAD_URL="https://github.com/SagerNet/sing-box/releases/download/v${VER}/${TARBALL}"
    echo "downloading: $DOWNLOAD_URL"
    _dl "$DOWNLOAD_URL" "/tmp/${TARBALL}"
    tar -xzf "/tmp/${TARBALL}" -C /tmp/
    install -m 755 /tmp/sing-box-*/sing-box "$SBOX_BIN"
    rm -rf /tmp/sing-box-* "/tmp/${TARBALL}"
fi

if [ ! -f "$SBOX_BIN" ]; then
    echo "ERROR: all install strategies failed" >&2
    exit 1
fi

echo "installed: $("$SBOX_BIN" version | head -1)"
"#,
    };

    let out = ctx.remote.exec(&node, install_script.trim()).await.map_err(|e| e.to_string())?;
    for l in out.stdout.lines() { plog!(log, "  {}", l); }
    if !out.stderr.is_empty() {
        for l in out.stderr.lines() { plog!(log, "  [stderr] {}", l); }
    }
    if !out.success() {
        return Err(format!("[phase 2/5] install failed (exit {})", out.exit_code));
    }
    plog!(log, "[phase 2/5] {} binary ready ✓", core_name);

    // ---- phase 3: systemd service setup ----
    plog!(log, "[phase 3/5] setting up systemd service...");

    let service_script: &str = match node.core {
        panel_domain::CoreKind::Xray => r#"#!/bin/bash
set -euo pipefail
# Xray official installer already creates the service; just ensure it exists.
if ! systemctl cat xray >/dev/null 2>&1; then
    # Fallback: create minimal service unit
    mkdir -p /etc/xray /usr/local/share/xray
    cat > /etc/systemd/system/xray.service << 'EOF'
[Unit]
Description=Xray Service
Documentation=https://github.com/xtls
After=network.target nss-lookup.target

[Service]
User=root
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_BIND_SERVICE CAP_NET_RAW
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE CAP_NET_RAW
NoNewPrivileges=yes
ExecStart=/usr/local/bin/xray run -c /etc/xray/config.json
Restart=on-failure
RestartSec=5
LimitNOFILE=1048576

[Install]
WantedBy=multi-user.target
EOF
fi
mkdir -p /etc/xray && chmod 750 /etc/xray
systemctl daemon-reload
systemctl enable xray 2>&1 || true
echo "service=xray enabled=$(systemctl is-enabled xray 2>/dev/null || echo unknown)"
"#,
        panel_domain::CoreKind::Singbox => r#"#!/bin/bash
set -euo pipefail
if ! systemctl cat sing-box >/dev/null 2>&1; then
    cat > /etc/systemd/system/sing-box.service << 'EOF'
[Unit]
Description=sing-box Service
Documentation=https://sing-box.sagernet.org
After=network.target nss-lookup.target

[Service]
User=root
WorkingDirectory=/etc/sing-box
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_BIND_SERVICE CAP_SYS_PTRACE CAP_DAC_READ_SEARCH
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE CAP_SYS_PTRACE CAP_DAC_READ_SEARCH
ExecStart=/usr/local/bin/sing-box run -c /etc/sing-box/config.json
Restart=on-failure
RestartSec=5
LimitNOFILE=1048576

[Install]
WantedBy=multi-user.target
EOF
fi
mkdir -p /etc/sing-box && chmod 750 /etc/sing-box
systemctl daemon-reload
systemctl enable sing-box 2>&1 || true
echo "service=sing-box enabled=$(systemctl is-enabled sing-box 2>/dev/null || echo unknown)"
"#,
    };

    let out = ctx.remote.exec(&node, service_script.trim()).await.map_err(|e| e.to_string())?;
    for l in out.stdout.lines() { plog!(log, "  {}", l); }
    if !out.success() {
        return Err(format!("[phase 3/5] service setup failed (exit {})", out.exit_code));
    }
    plog!(log, "[phase 3/5] systemd service ready ✓");

    // ---- phase 4: render + upload config ----
    plog!(log, "[phase 4/5] rendering and uploading config...");
    let listeners = ctx.listeners.list_for_node(node.id).await.map_err(|e| e.to_string())?;
    let mut owned_clients = Vec::new();
    for l in &listeners {
        owned_clients.push(ctx.proxy_users.list_for_listener(l.id).await.map_err(|e| e.to_string())?);
    }
    let chain_pool = ctx.chain_proxies.list_enabled().await.unwrap_or_default();
    let resolve_chain = |l: &panel_domain::Listener| -> Option<&panel_domain::ChainProxy> {
        let cid = l.params.get("chain_proxy_id")?.as_i64()?;
        chain_pool.iter().find(|c| c.id == cid)
    };
    let inbounds: Vec<panel_core::InboundContext> = listeners.iter().zip(owned_clients.iter())
        .map(|(l, c)| panel_core::InboundContext { listener: l, clients: c.as_slice(), chain: resolve_chain(l) })
        .collect();

    // When there are no listeners yet, write a minimal "skeleton" config that
    // lets the core start and pass its self-test. This is essential on first
    // provision before the operator has added any listeners — without it the
    // service would immediately fail to start and show a misleading error.
    let config_path = Adapters::config_path(node.core);
    let (bytes, inbound_count) = if inbounds.is_empty() {
        plog!(log, "[phase 4/5] no listeners yet — writing default skeleton config");
        let skeleton = match node.core {
            panel_domain::CoreKind::Xray => serde_json::json!({
                "log": { "loglevel": "warning" },
                "api": { "tag": "api", "services": ["StatsService", "HandlerService"] },
                "stats": {},
                "policy": {
                    "system": { "statsInboundUplink": true, "statsInboundDownlink": true }
                },
                "inbounds": [],
                "outbounds": [
                    { "tag": "direct",  "protocol": "freedom"   },
                    { "tag": "blocked", "protocol": "blackhole" }
                ],
                "routing": { "domainStrategy": "AsIs", "rules": [] }
            }),
            panel_domain::CoreKind::Singbox => serde_json::json!({
                "log":      { "level": "warn", "timestamp": true },
                "inbounds": [],
                "outbounds": [
                    { "type": "direct", "tag": "direct" },
                    { "type": "block",  "tag": "block"  }
                ],
                "route": { "rules": [] }
            }),
        };
        let b = serde_json::to_vec_pretty(&skeleton).map_err(|e| e.to_string())?;
        (b, 0usize)
    } else {
        let adapter = ctx.adapters.for_core(node.core);
        let config  = adapter.render_node_config(&panel_core::NodeConfigContext { node: &node, inbounds: &inbounds })
            .map_err(|e| e.to_string())?;
        let count = inbounds.len();
        let b = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        (b, count)
    };

    ctx.remote.write_file(&node, config_path, &bytes).await.map_err(|e| e.to_string())?;
    if inbound_count == 0 {
        plog!(log, "[phase 4/5] skeleton config written → {} ({} bytes) — add listeners and run Apply to activate", config_path, bytes.len());
    } else {
        plog!(log, "[phase 4/5] config uploaded: {} inbound(s), {} bytes → {}", inbound_count, bytes.len(), config_path);
    }

    // ---- phase 5: start + verify ----
    plog!(log, "[phase 5/5] starting service...");
    let unit = Adapters::systemd_unit(node.core);
    // Validate config before restart.
    // Empty-inbounds skeleton configs pass xray/sing-box self-test fine, but we
    // log a reminder that listeners still need to be added.
    let validate = match node.core {
        panel_domain::CoreKind::Xray    => format!("xray -test -c {} 2>&1 && echo CONFIG_OK", config_path),
        panel_domain::CoreKind::Singbox => format!("sing-box check -c {} 2>&1 && echo CONFIG_OK", config_path),
    };
    if inbound_count == 0 {
        plog!(log, "[phase 5/5] validating skeleton config (no inbounds yet — add listeners via panel then run Apply)...");
    }
    let out = ctx.remote.exec(&node, &validate).await.map_err(|e| e.to_string())?;
    for l in out.stdout.lines() { plog!(log, "  {}", l); }
    if !out.success() {
        return Err(format!("[phase 5/5] config validation failed:\n{}", out.stdout.trim()));
    }

    let start_cmd = format!(
        "systemctl restart {unit} && sleep 1 && systemctl is-active {unit} && \
         echo 'PID='$(systemctl show -p MainPID --value {unit})"
    );
    let out = ctx.remote.exec(&node, &start_cmd).await.map_err(|e| e.to_string())?;
    for l in out.stdout.lines() { plog!(log, "  {}", l); }
    if !out.success() {
        // Collect service journal for diagnosis
        let journal = format!("journalctl -u {unit} -n 30 --no-pager 2>/dev/null || true");
        let j = ctx.remote.exec(&node, &journal).await.unwrap_or_default();
        for l in j.stdout.lines() { plog!(log, "  [journal] {}", l); }
        return Err(format!("[phase 5/5] {unit} failed to start (exit {})", out.exit_code));
    }
    plog!(log, "[phase 5/5] {} is active and running ✓", unit);

    plog!(log, "");
    plog!(log, "✅ provision complete — {} installed, config deployed, service running", core_name);
    Ok(())
}

/// Convenience: build an `Adapters` registry without typing the Arcs.
impl ExecCtx {
    pub fn new(
        tasks: TaskRepo,
        nodes: NodeRepo,
        listeners: ListenerRepo,
        proxy_users: ProxyUserRepo,
        chain_proxies: ChainProxyRepo,
        remote: Arc<dyn NodeRemote>,
    ) -> Self {
        Self {
            tasks,
            nodes,
            listeners,
            proxy_users,
            chain_proxies,
            adapters: Adapters::new(),
            remote,
        }
    }
}
