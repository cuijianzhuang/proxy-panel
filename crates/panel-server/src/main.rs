mod adapters;
mod audit;
mod auth;
mod backups;
mod bootstrap;
mod cdn_endpoints;
mod chain_proxies;
mod config;
mod error;
mod listeners;
mod nodes;
mod notifications;
mod plans;
mod proxy_users;
mod spa;
mod state;
mod subscription;
mod subscription_page;
mod tasks;
mod traffic;
mod utils;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::{routing::{get, post}, Json, Router};
use panel_auth::{PanelUserRepo, SessionRepo};
use panel_domain::{
    CdnEndpointRepo, ChainProxyRepo, ListenerRepo, NodeRepo, NotificationChannelRepo,
    NotificationRuleRepo, PlanRepo, ProxyUserRepo,
};
use panel_node::{DryRunRemote, NodeRemote, SshConfig, SshRemote};
use panel_persistence::Database;
use panel_task::{spawn_worker, TaskRepo};
use serde::Serialize;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

#[derive(Serialize)]
struct Health {
    status:  &'static str,
    name:    &'static str,
    version: &'static str,
    db:      DbHealth,
}

#[derive(Serialize)]
struct DbHealth {
    kind: &'static str,
    ping: &'static str,
}

async fn healthz(State(state): State<AppState>) -> Json<Health> {
    let ping = match state.db.ping().await {
        Ok(()) => "ok",
        Err(err) => {
            tracing::warn!(%err, "db ping failed");
            "error"
        }
    };

    Json(Health {
        status:  "ok",
        name:    "proxy-panel",
        version: env!("CARGO_PKG_VERSION"),
        db:      DbHealth {
            kind: state.db.kind().as_str(),
            ping,
        },
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Lightweight CLI: `--healthcheck` probes a running instance over plain
    // TCP (no deps, no port grab) and exits 0/1 — used by the Docker
    // HEALTHCHECK so the runtime image needs no curl/wget. `--version` prints
    // and exits.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("proxy-panel {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--healthcheck") {
        let bind = std::env::var("PANEL_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
        // 0.0.0.0 isn't connectable — probe loopback on the same port.
        let target = bind.replace("0.0.0.0", "127.0.0.1");
        std::process::exit(match healthcheck(&target) {
            Ok(true) => 0,
            _ => 1,
        });
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = Config::from_env()?;
    tracing::info!(bind = %cfg.bind, db_url = %redact(&cfg.database_url), "starting panel-server");

    let db = Database::connect(&cfg.database_url).await?;
    db.migrate().await?;

    let users = PanelUserRepo::new(db.clone());
    let sessions = SessionRepo::new(db.clone());
    let listeners_repo = ListenerRepo::new(db.clone());
    let plans_repo = PlanRepo::new(db.clone());
    let proxy_users_repo = ProxyUserRepo::new(db.clone());
    let nodes_repo = NodeRepo::new(db.clone());
    let tasks_repo = TaskRepo::new(db.clone());
    let cdn_endpoints_repo = CdnEndpointRepo::new(db.clone());
    let chain_proxies_repo = ChainProxyRepo::new(db.clone());
    let channels_repo = NotificationChannelRepo::new(db.clone());
    let notification_rules_repo = NotificationRuleRepo::new(db.clone());

    bootstrap::ensure_admin(&users, cfg.bootstrap_admin_password.as_deref()).await?;

    // Background task worker. Remote impl is chosen by env (`PANEL_REMOTE_MODE`):
    //   - dry-run (default): logs SSH calls without touching the network.
    //   - ssh: real russh client using PANEL_SSH_KEY_PATH or PANEL_SSH_PASSWORD.
    let remote: Arc<dyn NodeRemote> = match &cfg.remote_mode {
        config::RemoteMode::DryRun => {
            tracing::info!("remote mode: dry-run (no SSH will be attempted)");
            Arc::new(DryRunRemote::new())
        }
        config::RemoteMode::Ssh { key_path, password } => {
            let ssh_cfg = if let Some(p) = key_path {
                tracing::info!(key = %p.display(), "remote mode: ssh (pubkey)");
                SshConfig::from_key_path(p.clone())
            } else if let Some(pw) = password {
                tracing::info!("remote mode: ssh (password)");
                let _ = pw; // not logged
                SshConfig::from_password(pw.clone())
            } else {
                unreachable!("Config::from_env enforces one of key/password");
            };
            Arc::new(SshRemote::new(ssh_cfg))
        }
    };
    let exec_ctx = panel_task::ExecCtx::new(
        tasks_repo.clone(),
        nodes_repo.clone(),
        listeners_repo.clone(),
        proxy_users_repo.clone(),
        chain_proxies_repo.clone(),
        remote.clone(),
    );
    let _worker = spawn_worker(exec_ctx, Duration::from_millis(500));
    tracing::info!("task worker spawned");

    let state = AppState {
        db: db.clone(),
        users,
        sessions,
        listeners: listeners_repo,
        plans: plans_repo,
        proxy_users: proxy_users_repo,
        nodes: nodes_repo,
        tasks: tasks_repo,
        cdn_endpoints: cdn_endpoints_repo,
        chain_proxies: chain_proxies_repo,
        channels: channels_repo,
        notification_rules: notification_rules_repo,
        stats: panel_domain::StatsRepo::new(db),
        remote,
        adapters: adapters::Adapters::new(),
        cookie_secure: cfg.cookie_secure,
        public_host: cfg.public_host.clone(),
    };

    // Background traffic collector. Polls every 60s in dry-run; production
    // would tighten this and gate on node.status == online.
    {
        let collector_state = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            tick.tick().await; // consume the immediate first tick
            loop {
                tick.tick().await;
                let s = collector_state.collect_traffic().await;
                if s.samples_written > 0 || s.users_cut_off > 0 {
                    tracing::info!(
                        nodes = s.nodes_polled, samples = s.samples_written,
                        cut_off = s.users_cut_off, "traffic collected"
                    );
                }
            }
        });
        tracing::info!("traffic collector spawned (60s)");
    }

    let app = Router::new()
        .route("/api/healthz", get(healthz))
        .route("/api/login", post(auth::login))
        .route("/api/logout", post(auth::logout))
        .route("/api/me", get(auth::me))
        .route("/api/me/password", post(auth::change_password))
        .route("/api/listeners", get(listeners::list).post(listeners::create))
        .route(
            "/api/listeners/:id",
            get(listeners::get_one)
                .put(listeners::update)
                .delete(listeners::delete),
        )
        .route("/api/listeners/:id/preview", get(listeners::preview))
        .route(
            "/api/listeners/:id/clients",
            get(proxy_users::list_clients).post(proxy_users::attach),
        )
        .route(
            "/api/listeners/:id/clients/:user_id",
            axum::routing::delete(proxy_users::detach),
        )
        .route("/api/plans", get(plans::list).post(plans::create))
        .route(
            "/api/plans/:id",
            get(plans::get_one).put(plans::update).delete(plans::delete),
        )
        .route(
            "/api/proxy-users",
            get(proxy_users::list).post(proxy_users::create),
        )
        .route(
            "/api/proxy-users/:id",
            get(proxy_users::get_one)
                .put(proxy_users::update)
                .delete(proxy_users::delete),
        )
        .route("/api/nodes", get(nodes::list).post(nodes::create))
        .route(
            "/api/nodes/:id",
            get(nodes::get_one)
                .put(nodes::update)
                .delete(nodes::delete),
        )
        .route("/api/nodes/:id/config", get(nodes::render_config))
        .route("/api/nodes/:id/apply", post(tasks::apply_node))
        .route("/api/nodes/:id/restart", post(tasks::restart_node))
        .route("/api/nodes/:id/health-check", post(tasks::health_check_node))
        .route("/api/tasks", get(tasks::list))
        .route("/api/tasks/:id", get(tasks::get_one))
        .route("/api/audit", get(audit::list))
        .route("/api/backups", get(backups::list).post(backups::create))
        .route(
            "/api/backups/:id",
            axum::routing::delete(backups::delete),
        )
        .route("/api/backups/:id/download", get(backups::download))
        .route("/api/utils/reality-keypair", get(utils::reality_keypair))
        .route("/api/utils/random-id",       get(utils::random_id))
        .route("/api/utils/random-port",     get(utils::random_port))
        .route("/api/cdn-endpoints",
               get(cdn_endpoints::list).post(cdn_endpoints::create))
        .route("/api/cdn-endpoints/:id",
               get(cdn_endpoints::get_one)
                   .put(cdn_endpoints::update)
                   .delete(cdn_endpoints::delete))
        .route("/api/chain-proxies",
               get(chain_proxies::list).post(chain_proxies::create))
        .route("/api/chain-proxies/:id",
               get(chain_proxies::get_one)
                   .put(chain_proxies::update)
                   .delete(chain_proxies::delete))
        .route("/api/notifications",
               get(notifications::list_channels).post(notifications::create_channel))
        .route("/api/notifications/:id",
               axum::routing::put(notifications::update_channel)
                   .delete(notifications::delete_channel))
        .route("/api/notifications/:id/test", post(notifications::test_channel))
        .route("/api/notification-rules", get(notifications::list_rules))
        .route("/api/notification-rules/:event_type", axum::routing::put(notifications::upsert_rule))
        .route("/api/traffic", get(traffic::summary))
        .route("/api/traffic/series", get(traffic::series))
        .route("/api/traffic/collect", post(traffic::collect_now))
        .route("/sub/:token", get(subscription::subscribe))
        .route("/s/:token",   get(subscription_page::page))
        // Audit middleware before with_state so it sees AppState.
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            audit::middleware,
        ))
        .with_state(state)
        // Anything not matched above is presumed to be a frontend route —
        // serve the embedded SPA (with index.html fallback for hard refreshes).
        .fallback(spa::handler)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    tracing::info!(addr = %cfg.bind, "panel-server listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// Blocking, dependency-free liveness probe: open a TCP connection to `addr`,
/// send a minimal HTTP/1.0 GET /api/healthz, and return whether the status
/// line is `200`. Used by `--healthcheck`.
fn healthcheck(addr: &str) -> std::io::Result<bool> {
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    let mut stream = TcpStream::connect(addr)?;
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let host = addr.rsplit_once(':').map(|(h, _)| h).unwrap_or(addr);
    let req = format!(
        "GET /api/healthz HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(req.as_bytes())?;
    let mut buf = String::new();
    stream.take(256).read_to_string(&mut buf)?;
    Ok(buf.starts_with("HTTP/1.") && buf.contains(" 200"))
}

/// Hide credentials when logging the DB URL.
fn redact(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let (scheme, rest) = url.split_at(scheme_end + 3);
        if let Some(at) = rest.find('@') {
            return format!("{scheme}***@{}", &rest[at + 1..]);
        }
    }
    url.to_string()
}
