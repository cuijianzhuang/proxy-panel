use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::Row;

use crate::error::{Error, Result};

// ============================================================================
// Enums
// ============================================================================

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoreKind {
    Xray,
    Singbox,
}

impl CoreKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::Singbox => "singbox",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "xray" => Ok(Self::Xray),
            "singbox" => Ok(Self::Singbox),
            other => Err(Error::invalid(format!(
                "unknown core: {other} (expected xray|singbox)"
            ))),
        }
    }
}

/// Known proxy protocols. Adding a new one is a one-line change.
/// We validate strictly to avoid garbage values landing in the DB.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    Vless,
    Vmess,
    Trojan,
    Shadowsocks,
    Hysteria2,
    Tuic,
}

impl Protocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vless => "vless",
            Self::Vmess => "vmess",
            Self::Trojan => "trojan",
            Self::Shadowsocks => "shadowsocks",
            Self::Hysteria2 => "hysteria2",
            Self::Tuic => "tuic",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "vless" => Ok(Self::Vless),
            "vmess" => Ok(Self::Vmess),
            "trojan" => Ok(Self::Trojan),
            "shadowsocks" => Ok(Self::Shadowsocks),
            "hysteria2" => Ok(Self::Hysteria2),
            "tuic" => Ok(Self::Tuic),
            other => Err(Error::invalid(format!("unknown protocol: {other}"))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Tcp,
    Ws,
    Grpc,
    Xhttp,
    Quic,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Ws => "ws",
            Self::Grpc => "grpc",
            Self::Xhttp => "xhttp",
            Self::Quic => "quic",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "tcp" => Ok(Self::Tcp),
            "ws" => Ok(Self::Ws),
            "grpc" => Ok(Self::Grpc),
            "xhttp" => Ok(Self::Xhttp),
            "quic" => Ok(Self::Quic),
            other => Err(Error::invalid(format!("unknown transport: {other}"))),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    None,
    Tls,
    Reality,
}

impl TlsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Tls => "tls",
            Self::Reality => "reality",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "none" => Ok(Self::None),
            "tls" => Ok(Self::Tls),
            "reality" => Ok(Self::Reality),
            other => Err(Error::invalid(format!("unknown tls_mode: {other}"))),
        }
    }
}

// ============================================================================
// Listener
// ============================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Listener {
    pub id:                 i64,
    pub node_id:            Option<i64>,
    pub name:               String,
    pub core:               CoreKind,
    pub protocol:           Protocol,
    pub transport:          Transport,
    pub tls_mode:           TlsMode,
    pub port:               i32,
    pub params:             serde_json::Value,
    pub enabled:            bool,
    pub source_listener_id: Option<i64>,
    pub created_at:         DateTime<Utc>,
    pub updated_at:         DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateListener {
    pub name:               String,
    pub core:               CoreKind,
    pub protocol:           Protocol,
    #[serde(default = "default_transport")]
    pub transport:          Transport,
    #[serde(default = "default_tls_mode")]
    pub tls_mode:           TlsMode,
    pub port:               i32,
    #[serde(default)]
    pub node_id:            Option<i64>,
    #[serde(default = "empty_object")]
    pub params:             serde_json::Value,
    #[serde(default = "yes")]
    pub enabled:            bool,
    #[serde(default)]
    pub source_listener_id: Option<i64>,
}

/// Partial update — `None` fields are left unchanged.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateListener {
    pub name:      Option<String>,
    pub core:      Option<CoreKind>,
    pub protocol:  Option<Protocol>,
    pub transport: Option<Transport>,
    pub tls_mode:  Option<TlsMode>,
    pub port:      Option<i32>,
    pub node_id:   Option<Option<i64>>,
    pub params:    Option<serde_json::Value>,
    pub enabled:   Option<bool>,
}

fn default_transport() -> Transport {
    Transport::Tcp
}
fn default_tls_mode() -> TlsMode {
    TlsMode::None
}
fn empty_object() -> serde_json::Value {
    serde_json::json!({})
}
fn yes() -> bool {
    true
}

impl CreateListener {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::invalid("name is required"));
        }
        if self.name.len() > 128 {
            return Err(Error::invalid("name too long (>128)"));
        }
        if !(1..=65535).contains(&self.port) {
            return Err(Error::invalid("port must be 1..=65535"));
        }
        if !self.params.is_object() {
            return Err(Error::invalid("params must be a JSON object"));
        }
        Ok(())
    }
}

// ============================================================================
// Repo
// ============================================================================

#[derive(Clone)]
pub struct ListenerRepo {
    db: Database,
}

const COLS: &str = "id, node_id, name, core, protocol, transport, tls_mode, port, params, \
                    enabled, source_listener_id, created_at, updated_at";

impl ListenerRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<Listener>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM listeners ORDER BY id");
                let rows = sqlx::query(&sql).fetch_all(pool).await?;
                rows.into_iter().map(map_sqlite).collect()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM listeners ORDER BY id");
                let rows = sqlx::query(&sql).fetch_all(pool).await?;
                rows.into_iter().map(map_postgres).collect()
            }
        }
    }

    /// All enabled listeners on a node, ordered by id. Used by node-level
    /// config rendering.
    pub async fn list_for_node(&self, node_id: i64) -> Result<Vec<Listener>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!(
                    "SELECT {COLS} FROM listeners WHERE node_id = ? AND enabled = 1 ORDER BY id"
                );
                let rows = sqlx::query(&sql).bind(node_id).fetch_all(pool).await?;
                rows.into_iter().map(map_sqlite).collect()
            }
            Database::Postgres(pool) => {
                let sql = format!(
                    "SELECT {COLS} FROM listeners WHERE node_id = $1 AND enabled = TRUE ORDER BY id"
                );
                let rows = sqlx::query(&sql).bind(node_id).fetch_all(pool).await?;
                rows.into_iter().map(map_postgres).collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<Listener>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM listeners WHERE id = ?");
                let opt = sqlx::query(&sql).bind(id).fetch_optional(pool).await?;
                opt.map(map_sqlite).transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM listeners WHERE id = $1");
                let opt = sqlx::query(&sql).bind(id).fetch_optional(pool).await?;
                opt.map(map_postgres).transpose()
            }
        }
    }

    pub async fn create(&self, input: CreateListener) -> Result<Listener> {
        input.validate()?;
        let id = match &self.db {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "INSERT INTO listeners \
                       (node_id, name, core, protocol, transport, tls_mode, port, params, \
                        enabled, source_listener_id) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
                )
                .bind(input.node_id)
                .bind(&input.name)
                .bind(input.core.as_str())
                .bind(input.protocol.as_str())
                .bind(input.transport.as_str())
                .bind(input.tls_mode.as_str())
                .bind(input.port)
                .bind(Json(&input.params))
                .bind(input.enabled)
                .bind(input.source_listener_id)
                .fetch_one(pool)
                .await?;
                row.try_get::<i64, _>("id")?
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "INSERT INTO listeners \
                       (node_id, name, core, protocol, transport, tls_mode, port, params, \
                        enabled, source_listener_id) \
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) RETURNING id",
                )
                .bind(input.node_id)
                .bind(&input.name)
                .bind(input.core.as_str())
                .bind(input.protocol.as_str())
                .bind(input.transport.as_str())
                .bind(input.tls_mode.as_str())
                .bind(input.port)
                .bind(Json(&input.params))
                .bind(input.enabled)
                .bind(input.source_listener_id)
                .fetch_one(pool)
                .await?;
                row.try_get::<i64, _>("id")?
            }
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(&self, id: i64, patch: UpdateListener) -> Result<Listener> {
        let Some(existing) = self.find(id).await? else {
            return Err(Error::NotFound);
        };

        // Merge: anything `Some` overrides the existing field.
        let next = Listener {
            id:                 existing.id,
            node_id:            patch.node_id.unwrap_or(existing.node_id),
            name:               patch.name.unwrap_or(existing.name),
            core:               patch.core.unwrap_or(existing.core),
            protocol:           patch.protocol.unwrap_or(existing.protocol),
            transport:          patch.transport.unwrap_or(existing.transport),
            tls_mode:           patch.tls_mode.unwrap_or(existing.tls_mode),
            port:               patch.port.unwrap_or(existing.port),
            params:             patch.params.unwrap_or(existing.params),
            enabled:            patch.enabled.unwrap_or(existing.enabled),
            source_listener_id: existing.source_listener_id,
            created_at:         existing.created_at,
            updated_at:         existing.updated_at,
        };

        if next.name.trim().is_empty() {
            return Err(Error::invalid("name is required"));
        }
        if !(1..=65535).contains(&next.port) {
            return Err(Error::invalid("port must be 1..=65535"));
        }
        if !next.params.is_object() {
            return Err(Error::invalid("params must be a JSON object"));
        }

        let now = Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE listeners SET \
                       node_id = ?, name = ?, core = ?, protocol = ?, transport = ?, \
                       tls_mode = ?, port = ?, params = ?, enabled = ?, updated_at = ? \
                     WHERE id = ?",
                )
                .bind(next.node_id)
                .bind(&next.name)
                .bind(next.core.as_str())
                .bind(next.protocol.as_str())
                .bind(next.transport.as_str())
                .bind(next.tls_mode.as_str())
                .bind(next.port)
                .bind(Json(&next.params))
                .bind(next.enabled)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE listeners SET \
                       node_id = $1, name = $2, core = $3, protocol = $4, transport = $5, \
                       tls_mode = $6, port = $7, params = $8, enabled = $9, updated_at = $10 \
                     WHERE id = $11",
                )
                .bind(next.node_id)
                .bind(&next.name)
                .bind(next.core.as_str())
                .bind(next.protocol.as_str())
                .bind(next.transport.as_str())
                .bind(next.tls_mode.as_str())
                .bind(next.port)
                .bind(Json(&next.params))
                .bind(next.enabled)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
        }
        self.find(id).await?.ok_or(Error::NotFound)
    }

    /// Attach a proxy user to this listener (idempotent — already-attached is OK).
    pub async fn attach_client(&self, listener_id: i64, proxy_user_id: i64) -> Result<()> {
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT OR IGNORE INTO listener_clients (listener_id, proxy_user_id) \
                     VALUES (?, ?)",
                )
                .bind(listener_id)
                .bind(proxy_user_id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO listener_clients (listener_id, proxy_user_id) \
                     VALUES ($1, $2) ON CONFLICT DO NOTHING",
                )
                .bind(listener_id)
                .bind(proxy_user_id)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Remove the attachment. Returns whether a row was actually deleted.
    pub async fn detach_client(&self, listener_id: i64, proxy_user_id: i64) -> Result<bool> {
        let n = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "DELETE FROM listener_clients WHERE listener_id = ? AND proxy_user_id = ?",
            )
            .bind(listener_id)
            .bind(proxy_user_id)
            .execute(pool)
            .await?
            .rows_affected(),
            Database::Postgres(pool) => sqlx::query(
                "DELETE FROM listener_clients WHERE listener_id = $1 AND proxy_user_id = $2",
            )
            .bind(listener_id)
            .bind(proxy_user_id)
            .execute(pool)
            .await?
            .rows_affected(),
        };
        Ok(n > 0)
    }

    /// Delete by id. Returns whether a row was actually removed.
    pub async fn delete(&self, id: i64) -> Result<bool> {
        let affected = match &self.db {
            Database::Sqlite(pool) => sqlx::query("DELETE FROM listeners WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
            Database::Postgres(pool) => sqlx::query("DELETE FROM listeners WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
        };
        Ok(affected > 0)
    }
}

// ============================================================================
// Row mapping (per-dialect, because SqliteRow / PgRow are distinct types)
// ============================================================================

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<Listener> {
    let core: String = row.try_get("core")?;
    let protocol: String = row.try_get("protocol")?;
    let transport: String = row.try_get("transport")?;
    let tls_mode: String = row.try_get("tls_mode")?;
    let params: Json<serde_json::Value> = row.try_get("params")?;
    Ok(Listener {
        id:                 row.try_get("id")?,
        node_id:            row.try_get("node_id")?,
        name:               row.try_get("name")?,
        core:               CoreKind::parse(&core)?,
        protocol:           Protocol::parse(&protocol)?,
        transport:          Transport::parse(&transport)?,
        tls_mode:           TlsMode::parse(&tls_mode)?,
        port:               row.try_get("port")?,
        params:             params.0,
        enabled:            row.try_get("enabled")?,
        source_listener_id: row.try_get("source_listener_id")?,
        created_at:         row.try_get("created_at")?,
        updated_at:         row.try_get("updated_at")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> Result<Listener> {
    let core: String = row.try_get("core")?;
    let protocol: String = row.try_get("protocol")?;
    let transport: String = row.try_get("transport")?;
    let tls_mode: String = row.try_get("tls_mode")?;
    let params: Json<serde_json::Value> = row.try_get("params")?;
    Ok(Listener {
        id:                 row.try_get("id")?,
        node_id:            row.try_get("node_id")?,
        name:               row.try_get("name")?,
        core:               CoreKind::parse(&core)?,
        protocol:           Protocol::parse(&protocol)?,
        transport:          Transport::parse(&transport)?,
        tls_mode:           TlsMode::parse(&tls_mode)?,
        port:               row.try_get("port")?,
        params:             params.0,
        enabled:            row.try_get("enabled")?,
        source_listener_id: row.try_get("source_listener_id")?,
        created_at:         row.try_get("created_at")?,
        updated_at:         row.try_get("updated_at")?,
    })
}
