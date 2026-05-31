//! `NodeRemote` backed by a real SSH client (russh).
//!
//! Authentication: pubkey (preferred) or password. The auth material is
//! injected once at startup via `SshConfig`; per-node overrides will land
//! when the `nodes` table grows credential columns.
//!
//! `write_file` uses a base64 round-trip + atomic rename:
//!   1. `echo "<base64>" | base64 -d > /tmp/<rand>.tmp`
//!   2. `mv /tmp/<rand>.tmp <path> && chmod 0644 <path>`
//!
//! This avoids the complexity of SFTP / channel stdin streaming while
//! handling binary content cleanly. Config files we push are at most a few
//! KB, so the 33% base64 inflation is irrelevant.
//!
//! **Host key verification**: v1 accepts every host key (TOFU/pinning is a
//! follow-up). Run only over networks you trust until that lands.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use panel_domain::Node;
use russh::client::{self, Handle, Handler};
use russh::keys::key::{KeyPair, PublicKey};
use russh::{ChannelMsg, Disconnect};

use crate::{Error, ExecOutput, NodeRemote, Result, StatUser, UserDelta};

/// SSH credentials. The `SshConfig` value is the panel-wide default; each
/// node may override it via its own `ssh_auth_method` + stored secret (see
/// `credential_for`).
#[derive(Debug, Clone)]
pub enum SshCredential {
    /// PEM/OpenSSH-format private key on disk. No passphrase support yet.
    KeyFile(PathBuf),
    /// Inline PEM/OpenSSH private key text (per-node, stored in the DB).
    InlineKey(String),
    /// Plain password.
    Password(String),
}

#[derive(Debug, Clone)]
pub struct SshConfig {
    pub cred:           SshCredential,
    /// Hard upper bound on a single `exec` / `write_file`.
    pub command_timeout: Duration,
    /// Connection + handshake timeout.
    pub connect_timeout: Duration,
}

impl SshConfig {
    pub fn from_key_path(path: impl Into<PathBuf>) -> Self {
        Self {
            cred:            SshCredential::KeyFile(path.into()),
            command_timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(10),
        }
    }

    pub fn from_password(password: impl Into<String>) -> Self {
        Self {
            cred:            SshCredential::Password(password.into()),
            command_timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(10),
        }
    }
}

pub struct SshRemote {
    config:   SshConfig,
    /// node id → fingerprint observed on the most recent connect. Lets the
    /// caller (which owns the DB) persist a first-use pin via
    /// `observed_host_key`.
    observed: Arc<std::sync::Mutex<std::collections::HashMap<i64, String>>>,
}

impl SshRemote {
    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            observed: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    async fn connect(&self, node: &Node) -> Result<Handle<PinningHandler>> {
        let addr = (node.addr.as_str(), node.ssh_port as u16);
        let client_cfg = Arc::new(client::Config {
            inactivity_timeout: Some(self.config.command_timeout),
            ..Default::default()
        });

        let slot: Arc<std::sync::Mutex<Option<String>>> = Arc::new(std::sync::Mutex::new(None));
        let handler = PinningHandler {
            expected: node.ssh_host_fingerprint.clone(),
            slot:     slot.clone(),
        };

        let connect = client::connect(client_cfg, addr, handler);
        let res = tokio::time::timeout(self.config.connect_timeout, connect)
            .await
            .map_err(|_| Error::Connect("ssh connect timed out".into()))?;

        // Record whatever fingerprint the handshake saw (success or key reject),
        // keyed by node id, so `observed_host_key` can hand it back.
        if let Some(fp) = slot.lock().ok().and_then(|s| s.clone()) {
            if let Ok(mut map) = self.observed.lock() {
                map.insert(node.id, fp);
            }
        }

        let mut handle = res.map_err(|e| {
            // A rejected host key surfaces here as a handshake error; make the
            // message actionable rather than a generic connect failure.
            if node.ssh_host_fingerprint.is_some() {
                Error::Connect(format!(
                    "host key mismatch or handshake failed for {} (pinned fingerprint no longer matches): {e}",
                    node.addr
                ))
            } else {
                Error::Connect(format!("{e}"))
            }
        })?;

        // Per-node credential takes precedence over the panel-wide default.
        let cred = self.credential_for(node);
        match &cred {
            SshCredential::KeyFile(path) => {
                let key: KeyPair = russh::keys::load_secret_key(path, None)
                    .map_err(|e| Error::Auth(format!("load key {}: {e}", path.display())))?;
                let ok = handle
                    .authenticate_publickey(&node.ssh_user, Arc::new(key))
                    .await
                    .map_err(|e| Error::Auth(format!("{e}")))?;
                if !ok {
                    return Err(Error::Auth("pubkey auth rejected".into()));
                }
            }
            SshCredential::InlineKey(pem) => {
                let key: KeyPair = russh::keys::decode_secret_key(pem, None)
                    .map_err(|e| Error::Auth(format!("parse inline key: {e}")))?;
                let ok = handle
                    .authenticate_publickey(&node.ssh_user, Arc::new(key))
                    .await
                    .map_err(|e| Error::Auth(format!("{e}")))?;
                if !ok {
                    return Err(Error::Auth("pubkey auth rejected".into()));
                }
            }
            SshCredential::Password(pw) => {
                let ok = handle
                    .authenticate_password(&node.ssh_user, pw)
                    .await
                    .map_err(|e| Error::Auth(format!("{e}")))?;
                if !ok {
                    return Err(Error::Auth("password auth rejected".into()));
                }
            }
        }

        Ok(handle)
    }

    /// Resolve which credential to use for `node`:
    ///   - `ssh_auth_method == "password"` + a stored password → that password
    ///   - `ssh_auth_method == "key"`      + a stored PEM      → that inline key
    ///   - otherwise ("global", or method set but secret missing) → the
    ///     panel-wide default from `SshConfig`.
    fn credential_for(&self, node: &Node) -> SshCredential {
        match node.ssh_auth_method.as_str() {
            "password" => match node.ssh_password.as_deref().filter(|s| !s.is_empty()) {
                Some(pw) => SshCredential::Password(pw.to_string()),
                None => self.config.cred.clone(),
            },
            "key" => match node.ssh_private_key.as_deref().filter(|s| !s.is_empty()) {
                Some(pem) => SshCredential::InlineKey(pem.to_string()),
                None => self.config.cred.clone(),
            },
            _ => self.config.cred.clone(),
        }
    }

    /// Open a session channel, run `cmd`, drain all output until exit-status.
    async fn run(&self, handle: &Handle<PinningHandler>, cmd: &str) -> Result<ExecOutput> {
        let mut channel = handle
            .channel_open_session()
            .await
            .map_err(|e| Error::Io(format!("open channel: {e}")))?;
        channel
            .exec(true, cmd)
            .await
            .map_err(|e| Error::Io(format!("exec: {e}")))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code: Option<i32> = None;

        let drain = async {
            while let Some(msg) = channel.wait().await {
                match msg {
                    ChannelMsg::Data { ref data } => stdout.extend_from_slice(data),
                    ChannelMsg::ExtendedData { ref data, ext: 1 } => stderr.extend_from_slice(data),
                    ChannelMsg::ExitStatus { exit_status } => {
                        exit_code = Some(exit_status as i32);
                        // ExitStatus is followed by Eof/Close; keep draining for cleanliness.
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
            }
            Ok::<(), Error>(())
        };
        tokio::time::timeout(self.config.command_timeout, drain)
            .await
            .map_err(|_| Error::Io("command timed out".into()))??;

        let stdout = String::from_utf8_lossy(&stdout).into_owned();
        let stderr = String::from_utf8_lossy(&stderr).into_owned();
        Ok(ExecOutput {
            exit_code: exit_code.unwrap_or(-1),
            stdout,
            stderr,
        })
    }
}

#[async_trait]
impl NodeRemote for SshRemote {
    async fn ping(&self, node: &Node) -> Result<String> {
        let handle = self.connect(node).await?;
        let out = self.run(&handle, "uname -a 2>/dev/null || cmd /c ver").await?;
        let _ = handle
            .disconnect(Disconnect::ByApplication, "ping done", "en")
            .await;
        Ok(out.stdout.lines().next().unwrap_or("connected").to_string())
    }

    async fn write_file(&self, node: &Node, path: &str, content: &[u8]) -> Result<()> {
        // We assume a POSIX target. base64 lives in coreutils on every distro
        // we'd plausibly target.
        let b64 = base64::engine::general_purpose::STANDARD.encode(content);
        let tmp = format!("/tmp/proxy-panel-upload-{}.tmp", rand_token());
        let upload_cmd = format!(
            "umask 077 && printf '%s' '{b64}' | base64 -d > '{tmp}' && \
             mv -f '{tmp}' '{path}' && chmod 0644 '{path}'",
            b64 = b64,
            tmp = tmp,
            path = shell_escape(path),
        );

        let handle = self.connect(node).await?;
        let out = self.run(&handle, &upload_cmd).await?;
        let _ = handle
            .disconnect(Disconnect::ByApplication, "upload done", "en")
            .await;

        if !out.success() {
            return Err(Error::Command {
                exit:   out.exit_code,
                stderr: if out.stderr.is_empty() {
                    out.stdout
                } else {
                    out.stderr
                },
            });
        }
        Ok(())
    }

    async fn exec(&self, node: &Node, cmd: &str) -> Result<ExecOutput> {
        let handle = self.connect(node).await?;
        let out = self.run(&handle, cmd).await?;
        let _ = handle
            .disconnect(Disconnect::ByApplication, "exec done", "en")
            .await;
        Ok(out)
    }

    async fn fetch_stats(&self, node: &Node, users: &[StatUser]) -> Result<Vec<UserDelta>> {
        // No management port → stats collection is opted out for this node.
        if node.mgmt_port == 0 || users.is_empty() {
            return Ok(vec![]);
        }

        // Both cores expose an xray-compatible gRPC StatsService on the
        // management port (xray natively; sing-box via `experimental.v2ray_api`
        // which our renderer emits when mgmt_port > 0). We query it with the
        // `xray` CLI already present on the box and `-reset` so each poll
        // returns the delta since the previous poll.
        //
        // `statsquery` prints a JSON document:
        //   {"stat":[{"name":"user>>>alice>>>traffic>>>downlink","value":"123"}, ...]}
        // We only ask for the `user>>>` prefix so system/inbound counters
        // don't pollute the parse.
        let cmd = format!(
            "xray api statsquery --server=127.0.0.1:{port} -pattern 'user>>>' -reset 2>/dev/null",
            port = node.mgmt_port,
        );

        let handle = self.connect(node).await?;
        let out = self.run(&handle, &cmd).await?;
        let _ = handle
            .disconnect(Disconnect::ByApplication, "stats done", "en")
            .await;

        if !out.success() || out.stdout.trim().is_empty() {
            // A non-zero exit usually means the `xray` binary isn't on PATH or
            // the API port isn't reachable. Treat as "no data this cycle"
            // rather than failing the whole collection run.
            return Ok(vec![]);
        }

        Ok(map_stats_to_deltas(&parse_xray_statsquery(&out.stdout), users))
    }

    async fn observed_host_key(&self, node_id: i64) -> Option<String> {
        self.observed.lock().ok().and_then(|m| m.get(&node_id).cloned())
    }
}

/// Parse `xray api statsquery` JSON into `(email, uplink, downlink)` triples.
///
/// Stat names look like `user>>>EMAIL>>>traffic>>>uplink|downlink`. EMAIL may
/// itself contain `@`, but not `>>>`, so we split on the delimiter and read the
/// last segment for the direction.
fn parse_xray_statsquery(json: &str) -> Vec<(String, i64, i64)> {
    use std::collections::HashMap;

    let parsed: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return vec![],
    };
    let stats = match parsed.get("stat").and_then(|s| s.as_array()) {
        Some(s) => s,
        None => return vec![],
    };

    // email -> (up, down)
    let mut acc: HashMap<String, (i64, i64)> = HashMap::new();
    for item in stats {
        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
        // value can arrive as a JSON string ("123") or number; handle both.
        let value = item
            .get("value")
            .map(|v| match v {
                serde_json::Value::String(s) => s.parse::<i64>().unwrap_or(0),
                serde_json::Value::Number(n) => n.as_i64().unwrap_or(0),
                _ => 0,
            })
            .unwrap_or(0);

        let parts: Vec<&str> = name.split(">>>").collect();
        // ["user", EMAIL, "traffic", "uplink"|"downlink"]
        if parts.len() != 4 || parts[0] != "user" || parts[2] != "traffic" {
            continue;
        }
        let email = parts[1].to_string();
        let entry = acc.entry(email).or_insert((0, 0));
        match parts[3] {
            "uplink" => entry.0 += value,
            "downlink" => entry.1 += value,
            _ => {}
        }
    }

    acc.into_iter().map(|(e, (up, down))| (e, up, down)).collect()
}

/// Map parsed `(email, up, down)` triples onto the panel's user ids, dropping
/// any email we don't recognise and any all-zero rows (no traffic this cycle).
fn map_stats_to_deltas(parsed: &[(String, i64, i64)], users: &[StatUser]) -> Vec<UserDelta> {
    parsed
        .iter()
        .filter_map(|(email, up, down)| {
            if *up == 0 && *down == 0 {
                return None;
            }
            let id = users.iter().find(|u| u.email == *email)?.id;
            Some(UserDelta { id, up: *up, down: *down })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// SSH handler: TOFU host-key pinning.
//
// On first connect (`expected == None`) we accept whatever key the server
// presents and record its SHA256 fingerprint into `slot`, so the caller can
// persist it. On every later connect the stored fingerprint is passed in as
// `expected`; we accept only if the presented key matches, otherwise the
// handshake is rejected (defends against a swapped / MITM'd host).
// ---------------------------------------------------------------------------

pub struct PinningHandler {
    /// The fingerprint we expect (the pinned one). `None` on first use.
    expected: Option<String>,
    /// Where we write the fingerprint we actually observed, so the caller can
    /// persist it after a successful first connect.
    slot:     Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl Handler for PinningHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> std::result::Result<bool, Self::Error> {
        let fp = server_public_key.fingerprint();
        if let Ok(mut s) = self.slot.lock() {
            *s = Some(fp.clone());
        }
        match &self.expected {
            // Established pin: accept iff it matches.
            Some(pinned) => Ok(pinned == &fp),
            // First use (TOFU): accept and let the caller persist `fp`.
            None => Ok(true),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rand_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("{:08x}", nanos ^ std::process::id().wrapping_mul(2654435761))
}

/// Tiny single-quote shell escape: replace `'` with `'\''`.
fn shell_escape(s: &str) -> String {
    s.replace('\'', r"'\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_statsquery_pairs_up_and_down() {
        let json = r#"{
          "stat": [
            {"name": "user>>>alice>>>traffic>>>uplink",   "value": "100"},
            {"name": "user>>>alice>>>traffic>>>downlink", "value": "900"},
            {"name": "user>>>bob>>>traffic>>>downlink",   "value": "50"}
          ]
        }"#;
        let mut got = parse_xray_statsquery(json);
        got.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(got, vec![
            ("alice".to_string(), 100, 900),
            ("bob".to_string(),   0,   50),
        ]);
    }

    #[test]
    fn parse_handles_numeric_values_and_emails_with_at() {
        let json = r#"{"stat":[
            {"name":"user>>>a@b.com>>>traffic>>>uplink","value":42}
        ]}"#;
        let got = parse_xray_statsquery(json);
        assert_eq!(got, vec![("a@b.com".to_string(), 42, 0)]);
    }

    #[test]
    fn parse_ignores_non_user_and_malformed() {
        let json = r#"{"stat":[
            {"name":"inbound>>>api>>>traffic>>>uplink","value":"5"},
            {"name":"garbage","value":"7"},
            {"name":"user>>>x>>>traffic>>>uplink","value":"3"}
        ]}"#;
        let got = parse_xray_statsquery(json);
        assert_eq!(got, vec![("x".to_string(), 3, 0)]);
    }

    #[test]
    fn parse_empty_or_invalid_returns_empty() {
        assert!(parse_xray_statsquery("").is_empty());
        assert!(parse_xray_statsquery("not json").is_empty());
        assert!(parse_xray_statsquery(r#"{"stat":[]}"#).is_empty());
    }

    #[test]
    fn map_drops_unknown_emails_and_zero_rows() {
        let parsed = vec![
            ("alice".to_string(), 10, 20),
            ("ghost".to_string(), 5, 5),   // not in users → dropped
            ("bob".to_string(),   0, 0),   // zero → dropped
        ];
        let users = vec![
            StatUser { id: 1, email: "alice".into() },
            StatUser { id: 2, email: "bob".into() },
        ];
        let deltas = map_stats_to_deltas(&parsed, &users);
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].id, 1);
        assert_eq!((deltas[0].up, deltas[0].down), (10, 20));
    }
}
