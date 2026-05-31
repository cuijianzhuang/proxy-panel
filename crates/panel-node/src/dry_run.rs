//! In-process `NodeRemote` that records every call and always succeeds.
//!
//! Useful for development and integration tests — exercises the entire
//! deploy pipeline (renderer, task worker, status transitions, log
//! aggregation) without touching a real VPS.

use std::sync::Mutex;

use async_trait::async_trait;
use panel_domain::Node;

use crate::{Error, ExecOutput, NodeRemote, Result, StatUser, UserDelta};

/// A single recorded interaction with the dry-run remote.
#[derive(Debug, Clone)]
pub enum Call {
    Ping {
        node_id: i64,
        addr:    String,
    },
    WriteFile {
        node_id: i64,
        path:    String,
        bytes:   usize,
    },
    Exec {
        node_id: i64,
        cmd:     String,
    },
}

#[derive(Default)]
pub struct DryRunRemote {
    /// Optional override for `exec()` outputs, keyed by exact command prefix.
    /// First match wins.
    overrides: Mutex<Vec<(String, ExecOutput)>>,
    /// Call log. Inspectable from tests.
    pub calls: Mutex<Vec<Call>>,
}

impl DryRunRemote {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make `exec()` return the given output whenever the command starts with
    /// `prefix`. Otherwise the default `exit_code = 0` is returned.
    pub fn stub_exec(&self, prefix: impl Into<String>, out: ExecOutput) {
        self.overrides.lock().unwrap().push((prefix.into(), out));
    }

    /// Snapshot of all calls observed so far.
    pub fn snapshot(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }
}

#[async_trait]
impl NodeRemote for DryRunRemote {
    async fn ping(&self, node: &Node) -> Result<String> {
        self.record(Call::Ping {
            node_id: node.id,
            addr:    node.addr.clone(),
        });
        Ok(format!("[dry-run] would ssh {}@{}", node.ssh_user, node.addr))
    }

    async fn write_file(&self, node: &Node, path: &str, content: &[u8]) -> Result<()> {
        self.record(Call::WriteFile {
            node_id: node.id,
            path:    path.to_string(),
            bytes:   content.len(),
        });
        Ok(())
    }

    async fn exec(&self, node: &Node, cmd: &str) -> Result<ExecOutput> {
        self.record(Call::Exec {
            node_id: node.id,
            cmd:     cmd.to_string(),
        });
        let overrides = self.overrides.lock().unwrap();
        for (prefix, out) in overrides.iter() {
            if cmd.starts_with(prefix.as_str()) {
                let out = out.clone();
                if !out.success() {
                    return Err(Error::Command {
                        exit:   out.exit_code,
                        stderr: out.stderr.clone(),
                    });
                }
                return Ok(out);
            }
        }
        Ok(ExecOutput {
            exit_code: 0,
            stdout:    format!("[dry-run] would exec: {cmd}"),
            stderr:    String::new(),
        })
    }

    async fn fetch_stats(&self, node: &Node, users: &[StatUser]) -> Result<Vec<UserDelta>> {
        self.record(Call::Exec {
            node_id: node.id,
            cmd:     format!("[dry-run] fetch_stats for {} users", users.len()),
        });
        // Synthesise plausible per-cycle traffic so the UI has something to
        // chart without a live node. Deterministic-ish from a time + id mix so
        // numbers vary between cycles but never explode.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as i64)
            .unwrap_or(0);
        Ok(users
            .iter()
            .map(|u| {
                let seed = (nanos ^ (u.id.wrapping_mul(2654435761))) & 0xff_ffff;
                let down = 256 * 1024 + (seed % (8 * 1024 * 1024)); // 256KiB..~8MiB
                let up = down / 6;
                UserDelta { id: u.id, up, down }
            })
            .collect())
    }
}
