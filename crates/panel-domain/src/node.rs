use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{Error, Result};
use crate::listener::CoreKind;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    Pending,
    Provisioning,
    Online,
    Offline,
    Failed,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Provisioning => "provisioning",
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(Self::Pending),
            "provisioning" => Ok(Self::Provisioning),
            "online" => Ok(Self::Online),
            "offline" => Ok(Self::Offline),
            "failed" => Ok(Self::Failed),
            other => Err(Error::invalid(format!("unknown node status: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id:           i64,
    pub name:         String,
    pub addr:         String,
    pub public_host:  Option<String>,
    pub core:         CoreKind,
    pub core_version: Option<String>,
    pub mgmt_port:    i32,
    pub mgmt_secret:  Option<String>,
    pub ssh_port:     i32,
    pub ssh_user:     String,
    /// 'global' | 'password' | 'key'. Determines which credential the SSH
    /// remote uses for this node (see migration 012).
    pub ssh_auth_method: String,
    /// Per-node SSH password. SECRET — skipped from API responses; only the
    /// SSH layer reads it in-process.
    #[serde(skip_serializing)]
    pub ssh_password: Option<String>,
    /// Per-node inline SSH private key (PEM). SECRET — skipped from API
    /// responses; only the SSH layer reads it in-process.
    #[serde(skip_serializing)]
    pub ssh_private_key: Option<String>,
    /// Computed flag so the UI can show "已配置凭据" without ever receiving
    /// the secret itself. Set during row mapping, not stored as a column.
    pub has_ssh_credential: bool,
    pub status:       NodeStatus,
    pub last_seen_at: Option<DateTime<Utc>>,
    pub note:         Option<String>,
    /// SHA256 fingerprint of the SSH server's host key, pinned on first
    /// successful connect (TOFU). `None` until the first connect records it.
    pub ssh_host_fingerprint: Option<String>,
    pub created_at:   DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateNode {
    pub name:         String,
    pub addr:         String,
    pub core:         CoreKind,
    #[serde(default)]
    pub public_host:  Option<String>,
    #[serde(default)]
    pub mgmt_port:    Option<i32>,
    #[serde(default)]
    pub mgmt_secret:  Option<String>,
    #[serde(default = "default_ssh_port")]
    pub ssh_port:     i32,
    #[serde(default = "default_ssh_user")]
    pub ssh_user:     String,
    #[serde(default = "default_ssh_auth_method")]
    pub ssh_auth_method: String,
    #[serde(default)]
    pub ssh_password:    Option<String>,
    #[serde(default)]
    pub ssh_private_key: Option<String>,
    #[serde(default)]
    pub note:         Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateNode {
    pub name:         Option<String>,
    pub addr:         Option<String>,
    pub public_host:  Option<Option<String>>,
    pub core:         Option<CoreKind>,
    pub core_version: Option<Option<String>>,
    pub mgmt_port:    Option<i32>,
    pub mgmt_secret:  Option<Option<String>>,
    pub ssh_port:     Option<i32>,
    pub ssh_user:     Option<String>,
    pub ssh_auth_method: Option<String>,
    /// Secret update semantics: `None` (field absent) = leave as-is; `Some("")`
    /// = clear; `Some(value)` = replace. Lets the edit form omit the secret to
    /// keep the existing one without re-typing it.
    pub ssh_password:    Option<String>,
    pub ssh_private_key: Option<String>,
    pub status:       Option<NodeStatus>,
    pub note:         Option<Option<String>>,
}

fn default_ssh_port() -> i32 {
    22
}
fn default_ssh_user() -> String {
    "root".to_string()
}
fn default_ssh_auth_method() -> String {
    "global".to_string()
}

impl CreateNode {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::invalid("name is required"));
        }
        if self.addr.trim().is_empty() {
            return Err(Error::invalid("addr is required"));
        }
        if !(1..=65535).contains(&self.ssh_port) {
            return Err(Error::invalid("ssh_port must be 1..=65535"));
        }
        if let Some(p) = self.mgmt_port {
            if !(0..=65535).contains(&p) {
                return Err(Error::invalid("mgmt_port must be 0..=65535"));
            }
        }
        Ok(())
    }
}

const COLS: &str = "id, name, addr, public_host, core, core_version, mgmt_port, mgmt_secret, \
                    ssh_port, ssh_user, ssh_auth_method, ssh_password, ssh_private_key, \
                    status, last_seen_at, note, ssh_host_fingerprint, created_at, updated_at";

#[derive(Clone)]
pub struct NodeRepo {
    db: Database,
}

impl NodeRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<Node>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM nodes ORDER BY id");
                sqlx::query(&sql)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_sqlite)
                    .collect()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM nodes ORDER BY id");
                sqlx::query(&sql)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_postgres)
                    .collect()
            }
        }
    }

    /// Pin the SSH host-key fingerprint observed on first connect (TOFU).
    /// Idempotent — callers only invoke this when the column was previously
    /// NULL, so it never silently overwrites an established pin.
    pub async fn set_host_fingerprint(&self, id: i64, fingerprint: &str) -> Result<()> {
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query("UPDATE nodes SET ssh_host_fingerprint = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
                    .bind(fingerprint).bind(id).execute(pool).await?;
            }
            Database::Postgres(pool) => {
                sqlx::query("UPDATE nodes SET ssh_host_fingerprint = $1, updated_at = NOW() WHERE id = $2")
                    .bind(fingerprint).bind(id).execute(pool).await?;
            }
        }
        Ok(())
    }

    pub async fn find(&self, id: i64) -> Result<Option<Node>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM nodes WHERE id = ?");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_sqlite)
                    .transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM nodes WHERE id = $1");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_postgres)
                    .transpose()
            }
        }
    }

    pub async fn create(&self, input: CreateNode) -> Result<Node> {
        input.validate()?;
        let id = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "INSERT INTO nodes (name, addr, public_host, core, mgmt_port, mgmt_secret, \
                                    ssh_port, ssh_user, ssh_auth_method, ssh_password, \
                                    ssh_private_key, note) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&input.name)
            .bind(&input.addr)
            .bind(input.public_host.as_deref())
            .bind(input.core.as_str())
            .bind(input.mgmt_port.unwrap_or(0))
            .bind(input.mgmt_secret.as_deref())
            .bind(input.ssh_port)
            .bind(&input.ssh_user)
            .bind(&input.ssh_auth_method)
            .bind(input.ssh_password.as_deref().filter(|s| !s.is_empty()))
            .bind(input.ssh_private_key.as_deref().filter(|s| !s.is_empty()))
            .bind(input.note.as_deref())
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
            Database::Postgres(pool) => sqlx::query(
                "INSERT INTO nodes (name, addr, public_host, core, mgmt_port, mgmt_secret, \
                                    ssh_port, ssh_user, ssh_auth_method, ssh_password, \
                                    ssh_private_key, note) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING id",
            )
            .bind(&input.name)
            .bind(&input.addr)
            .bind(input.public_host.as_deref())
            .bind(input.core.as_str())
            .bind(input.mgmt_port.unwrap_or(0))
            .bind(input.mgmt_secret.as_deref())
            .bind(input.ssh_port)
            .bind(&input.ssh_user)
            .bind(&input.ssh_auth_method)
            .bind(input.ssh_password.as_deref().filter(|s| !s.is_empty()))
            .bind(input.ssh_private_key.as_deref().filter(|s| !s.is_empty()))
            .bind(input.note.as_deref())
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(&self, id: i64, patch: UpdateNode) -> Result<Node> {
        let existing = self.find(id).await?.ok_or(Error::NotFound)?;
        let mut next = existing;
        if let Some(v) = patch.name {
            next.name = v;
        }
        if let Some(v) = patch.addr {
            next.addr = v;
        }
        if let Some(v) = patch.public_host {
            next.public_host = v;
        }
        if let Some(v) = patch.core {
            next.core = v;
        }
        if let Some(v) = patch.core_version {
            next.core_version = v;
        }
        if let Some(v) = patch.mgmt_port {
            next.mgmt_port = v;
        }
        if let Some(v) = patch.mgmt_secret {
            next.mgmt_secret = v;
        }
        if let Some(v) = patch.ssh_port {
            next.ssh_port = v;
        }
        if let Some(v) = patch.ssh_user {
            next.ssh_user = v;
        }
        if let Some(v) = patch.ssh_auth_method {
            next.ssh_auth_method = v;
        }
        // Secret semantics: absent → leave; "" → clear; value → replace.
        if let Some(v) = patch.ssh_password {
            next.ssh_password = if v.is_empty() { None } else { Some(v) };
        }
        if let Some(v) = patch.ssh_private_key {
            next.ssh_private_key = if v.is_empty() { None } else { Some(v) };
        }
        if let Some(v) = patch.status {
            next.status = v;
        }
        if let Some(v) = patch.note {
            next.note = v;
        }
        if !(1..=65535).contains(&next.ssh_port) {
            return Err(Error::invalid("ssh_port must be 1..=65535"));
        }

        let now = Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE nodes SET name=?, addr=?, public_host=?, core=?, core_version=?, \
                       mgmt_port=?, mgmt_secret=?, ssh_port=?, ssh_user=?, ssh_auth_method=?, \
                       ssh_password=?, ssh_private_key=?, status=?, note=?, \
                       updated_at=? WHERE id=?",
                )
                .bind(&next.name)
                .bind(&next.addr)
                .bind(next.public_host.as_deref())
                .bind(next.core.as_str())
                .bind(next.core_version.as_deref())
                .bind(next.mgmt_port)
                .bind(next.mgmt_secret.as_deref())
                .bind(next.ssh_port)
                .bind(&next.ssh_user)
                .bind(&next.ssh_auth_method)
                .bind(next.ssh_password.as_deref())
                .bind(next.ssh_private_key.as_deref())
                .bind(next.status.as_str())
                .bind(next.note.as_deref())
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE nodes SET name=$1, addr=$2, public_host=$3, core=$4, core_version=$5, \
                       mgmt_port=$6, mgmt_secret=$7, ssh_port=$8, ssh_user=$9, ssh_auth_method=$10, \
                       ssh_password=$11, ssh_private_key=$12, status=$13, note=$14, \
                       updated_at=$15 WHERE id=$16",
                )
                .bind(&next.name)
                .bind(&next.addr)
                .bind(next.public_host.as_deref())
                .bind(next.core.as_str())
                .bind(next.core_version.as_deref())
                .bind(next.mgmt_port)
                .bind(next.mgmt_secret.as_deref())
                .bind(next.ssh_port)
                .bind(&next.ssh_user)
                .bind(&next.ssh_auth_method)
                .bind(next.ssh_password.as_deref())
                .bind(next.ssh_private_key.as_deref())
                .bind(next.status.as_str())
                .bind(next.note.as_deref())
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
        }
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn delete(&self, id: i64) -> Result<bool> {
        let n = match &self.db {
            Database::Sqlite(pool) => sqlx::query("DELETE FROM nodes WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
            Database::Postgres(pool) => sqlx::query("DELETE FROM nodes WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
        };
        Ok(n > 0)
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<Node> {
    let core: String = row.try_get("core")?;
    let status: String = row.try_get("status")?;
    let ssh_password: Option<String> = row.try_get("ssh_password")?;
    let ssh_private_key: Option<String> = row.try_get("ssh_private_key")?;
    let has_ssh_credential =
        ssh_password.as_deref().is_some_and(|s| !s.is_empty())
            || ssh_private_key.as_deref().is_some_and(|s| !s.is_empty());
    Ok(Node {
        id:           row.try_get("id")?,
        name:         row.try_get("name")?,
        addr:         row.try_get("addr")?,
        public_host:  row.try_get("public_host")?,
        core:         CoreKind::parse(&core)?,
        core_version: row.try_get("core_version")?,
        mgmt_port:    row.try_get("mgmt_port")?,
        mgmt_secret:  row.try_get("mgmt_secret")?,
        ssh_port:     row.try_get("ssh_port")?,
        ssh_user:     row.try_get("ssh_user")?,
        ssh_auth_method: row.try_get("ssh_auth_method")?,
        ssh_password,
        ssh_private_key,
        has_ssh_credential,
        status:       NodeStatus::parse(&status)?,
        last_seen_at: row.try_get("last_seen_at")?,
        note:         row.try_get("note")?,
        ssh_host_fingerprint: row.try_get("ssh_host_fingerprint")?,
        created_at:   row.try_get("created_at")?,
        updated_at:   row.try_get("updated_at")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> Result<Node> {
    let core: String = row.try_get("core")?;
    let status: String = row.try_get("status")?;
    let ssh_password: Option<String> = row.try_get("ssh_password")?;
    let ssh_private_key: Option<String> = row.try_get("ssh_private_key")?;
    let has_ssh_credential =
        ssh_password.as_deref().is_some_and(|s| !s.is_empty())
            || ssh_private_key.as_deref().is_some_and(|s| !s.is_empty());
    Ok(Node {
        id:           row.try_get("id")?,
        name:         row.try_get("name")?,
        addr:         row.try_get("addr")?,
        public_host:  row.try_get("public_host")?,
        core:         CoreKind::parse(&core)?,
        core_version: row.try_get("core_version")?,
        mgmt_port:    row.try_get("mgmt_port")?,
        mgmt_secret:  row.try_get("mgmt_secret")?,
        ssh_port:     row.try_get("ssh_port")?,
        ssh_user:     row.try_get("ssh_user")?,
        ssh_auth_method: row.try_get("ssh_auth_method")?,
        ssh_password,
        ssh_private_key,
        has_ssh_credential,
        status:       NodeStatus::parse(&status)?,
        last_seen_at: row.try_get("last_seen_at")?,
        note:         row.try_get("note")?,
        ssh_host_fingerprint: row.try_get("ssh_host_fingerprint")?,
        created_at:   row.try_get("created_at")?,
        updated_at:   row.try_get("updated_at")?,
    })
}
