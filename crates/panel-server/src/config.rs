use std::net::SocketAddr;
use std::path::PathBuf;

/// How task-worker SSH operations should be carried out.
#[derive(Debug, Clone)]
pub enum RemoteMode {
    /// In-process mock — every SSH call succeeds and is logged. Default in dev.
    DryRun,
    /// Real SSH via russh. Requires either a private key or password.
    Ssh {
        key_path: Option<PathBuf>,
        password: Option<String>,
    },
}

/// Server-level config. Sourced from env vars for now; a config file can come later.
#[derive(Debug, Clone)]
pub struct Config {
    pub bind:             SocketAddr,
    pub database_url:     String,
    /// Set the `Secure` flag on the session cookie. Default `false` for dev
    /// (so cookies work over plain http://localhost). Set to `true` in
    /// production (and put nginx/caddy in front for TLS).
    pub cookie_secure:    bool,
    /// Optional bootstrap admin password. If `panel_users` is empty at start,
    /// one user `admin` is created with this password. If unset, a random
    /// password is generated and printed once to stdout.
    pub bootstrap_admin_password: Option<String>,
    /// Public hostname used in generated subscription URIs. Defaults to the
    /// bind address host (so dev gets `127.0.0.1`). In production set this
    /// to the FQDN clients use to reach the proxy nodes.
    pub public_host: String,
    /// How the background worker talks to nodes.
    pub remote_mode: RemoteMode,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind: SocketAddr = std::env::var("PANEL_BIND")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()?;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "sqlite://./data/panel.db".to_string());

        let cookie_secure = std::env::var("PANEL_COOKIE_SECURE")
            .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);

        let bootstrap_admin_password = std::env::var("PANEL_ADMIN_PASSWORD").ok().filter(|s| !s.is_empty());

        let public_host = std::env::var("PANEL_PUBLIC_HOST")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| bind.ip().to_string());

        let remote_mode = match std::env::var("PANEL_REMOTE_MODE").as_deref() {
            Ok("ssh") => {
                let key_path = std::env::var("PANEL_SSH_KEY_PATH")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from);
                let password = std::env::var("PANEL_SSH_PASSWORD").ok().filter(|s| !s.is_empty());
                if key_path.is_none() && password.is_none() {
                    anyhow::bail!(
                        "PANEL_REMOTE_MODE=ssh but neither PANEL_SSH_KEY_PATH nor \
                         PANEL_SSH_PASSWORD is set"
                    );
                }
                RemoteMode::Ssh { key_path, password }
            }
            Ok("dry-run") | Ok("") | Err(_) => RemoteMode::DryRun,
            Ok(other) => {
                anyhow::bail!("PANEL_REMOTE_MODE: unknown value {other:?} (use 'dry-run' or 'ssh')")
            }
        };

        Ok(Self {
            bind,
            database_url,
            cookie_secure,
            bootstrap_admin_password,
            public_host,
            remote_mode,
        })
    }
}
