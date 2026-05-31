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
        TaskKind::Restart => restart(ctx, task).await,
        TaskKind::CheckHealth => check_health(ctx, task).await,
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
