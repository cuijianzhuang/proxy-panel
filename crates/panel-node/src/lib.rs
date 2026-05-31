//! Remote node access abstraction.
//!
//! The panel never talks SSH directly from handler code. Instead it goes
//! through `NodeRemote` — a small trait with two implementations:
//!
//! - `DryRunRemote`: in-process, records every call and always succeeds.
//!   Used in dev / CI / smoke tests; lets the whole pipeline run without
//!   a real VPS.
//! - `SshRemote` (TODO): russh-backed SSH client. Stub today, real impl
//!   in a follow-up.
//!
//! A "deployer" implementation is just `Arc<dyn NodeRemote>`; the
//! `panel-task` worker picks whichever one is wired into `AppState`.

mod dry_run;
mod error;
mod ssh;

pub use dry_run::DryRunRemote;
pub use error::{Error, Result};
pub use ssh::{SshConfig, SshCredential, SshRemote};

use async_trait::async_trait;
use panel_domain::Node;

/// Output of a shell command run on a remote node.
#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout:    String,
    pub stderr:    String,
}

impl ExecOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

impl Default for ExecOutput {
    fn default() -> Self {
        Self { exit_code: 0, stdout: String::new(), stderr: String::new() }
    }
}

/// A proxy user the collector knows is attached to a node. The `email` is the
/// key xray/sing-box report stats under (we set it to the user's name when
/// rendering inbounds), so it's how real stats get matched back to an id.
#[derive(Debug, Clone)]
pub struct StatUser {
    pub id:    i64,
    pub email: String,
}

/// One collection cycle's traffic increment for a user, in bytes since the
/// last poll.
#[derive(Debug, Clone)]
pub struct UserDelta {
    pub id:   i64,
    pub up:   i64,
    pub down: i64,
}

/// What a remote node can be asked to do on behalf of the panel.
///
/// Implementations must be cheap to `&self`-call concurrently; the task
/// worker can issue several apply operations against different nodes at once.
#[async_trait]
pub trait NodeRemote: Send + Sync {
    /// Lightweight reachability check. Returns a short identifier (e.g. uname)
    /// useful for logs.
    async fn ping(&self, node: &Node) -> Result<String>;

    /// Atomically replace the file at `path` with `content`.
    /// Implementations are expected to write to a temp path + rename, never
    /// truncate the target in-place.
    async fn write_file(&self, node: &Node, path: &str, content: &[u8]) -> Result<()>;

    /// Run a shell command on the node and return its outputs.
    async fn exec(&self, node: &Node, cmd: &str) -> Result<ExecOutput>;

    /// Pull per-user traffic deltas since the last poll. `users` is the set
    /// of proxy users currently attached to the node (id + email), used both
    /// as a hint and to map core-reported emails back to ids. The real SSH
    /// impl would query the xray gRPC StatsService / sing-box Clash API and
    /// reset counters; the dry-run impl synthesises plausible numbers.
    async fn fetch_stats(&self, node: &Node, users: &[StatUser]) -> Result<Vec<UserDelta>>;

    /// The SSH host-key fingerprint observed on the most recent connect to
    /// `node_id`, if any. Callers use this to persist a first-use (TOFU) pin
    /// after a successful operation. Implementations that don't speak SSH
    /// (e.g. the dry-run remote) return `None`.
    async fn observed_host_key(&self, _node_id: i64) -> Option<String> {
        None
    }
}
