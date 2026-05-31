use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainProxyType {
    Socks5,
    Http,
}

impl ChainProxyType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Socks5 => "socks5",
            Self::Http => "http",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "socks5" => Ok(Self::Socks5),
            "http" => Ok(Self::Http),
            other => Err(Error::invalid(format!("unknown chain proxy type: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ChainProxy {
    pub id:         i64,
    pub name:       String,
    pub proxy_type: ChainProxyType,
    pub address:    String,
    pub port:       i32,
    /// Credentials are stored as-is for now; rotation/encryption is a future tweak.
    pub username:   Option<String>,
    pub password:   Option<String>,
    pub enabled:    bool,
    pub note:       Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateChainProxy {
    pub name:       String,
    #[serde(default = "default_type")]
    pub proxy_type: ChainProxyType,
    pub address:    String,
    pub port:       i32,
    #[serde(default)]
    pub username:   Option<String>,
    #[serde(default)]
    pub password:   Option<String>,
    #[serde(default = "yes")]
    pub enabled:    bool,
    #[serde(default)]
    pub note:       Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateChainProxy {
    pub name:       Option<String>,
    pub proxy_type: Option<ChainProxyType>,
    pub address:    Option<String>,
    pub port:       Option<i32>,
    pub username:   Option<Option<String>>,
    pub password:   Option<Option<String>>,
    pub enabled:    Option<bool>,
    pub note:       Option<Option<String>>,
}

fn default_type() -> ChainProxyType { ChainProxyType::Socks5 }
fn yes() -> bool { true }

impl CreateChainProxy {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() { return Err(Error::invalid("name is required")); }
        if self.address.trim().is_empty() { return Err(Error::invalid("address is required")); }
        if !(1..=65535).contains(&self.port) { return Err(Error::invalid("port must be 1..=65535")); }
        Ok(())
    }
}

const COLS: &str = "id, name, proxy_type, address, port, username, password, enabled, note, created_at, updated_at";

#[derive(Clone)]
pub struct ChainProxyRepo { db: Database }

impl ChainProxyRepo {
    pub fn new(db: Database) -> Self { Self { db } }

    pub async fn list(&self) -> Result<Vec<ChainProxy>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM chain_proxies ORDER BY id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_sqlite).collect()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM chain_proxies ORDER BY id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_postgres).collect()
            }
        }
    }

    /// Only the chain proxies currently flagged enabled — used by the renderer
    /// so a disabled chain silently falls back to direct, instead of pushing a
    /// broken outbound to the VPS.
    pub async fn list_enabled(&self) -> Result<Vec<ChainProxy>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM chain_proxies WHERE enabled = 1 ORDER BY id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_sqlite).collect()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM chain_proxies WHERE enabled = TRUE ORDER BY id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_postgres).collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<ChainProxy>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM chain_proxies WHERE id = ?");
                sqlx::query(&sql).bind(id).fetch_optional(p).await?.map(map_sqlite).transpose()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM chain_proxies WHERE id = $1");
                sqlx::query(&sql).bind(id).fetch_optional(p).await?.map(map_postgres).transpose()
            }
        }
    }

    pub async fn create(&self, input: CreateChainProxy) -> Result<ChainProxy> {
        input.validate()?;
        let id = match &self.db {
            Database::Sqlite(p) => sqlx::query(
                "INSERT INTO chain_proxies (name, proxy_type, address, port, username, password, enabled, note) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&input.name).bind(input.proxy_type.as_str()).bind(&input.address)
            .bind(input.port).bind(input.username.as_deref()).bind(input.password.as_deref())
            .bind(input.enabled).bind(input.note.as_deref())
            .fetch_one(p).await?.try_get::<i64, _>("id")?,
            Database::Postgres(p) => sqlx::query(
                "INSERT INTO chain_proxies (name, proxy_type, address, port, username, password, enabled, note) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
            )
            .bind(&input.name).bind(input.proxy_type.as_str()).bind(&input.address)
            .bind(input.port).bind(input.username.as_deref()).bind(input.password.as_deref())
            .bind(input.enabled).bind(input.note.as_deref())
            .fetch_one(p).await?.try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(&self, id: i64, patch: UpdateChainProxy) -> Result<ChainProxy> {
        let mut row = self.find(id).await?.ok_or(Error::NotFound)?;
        if let Some(v) = patch.name       { row.name = v; }
        if let Some(v) = patch.proxy_type { row.proxy_type = v; }
        if let Some(v) = patch.address    { row.address = v; }
        if let Some(v) = patch.port       { row.port = v; }
        if let Some(v) = patch.username   { row.username = v; }
        if let Some(v) = patch.password   { row.password = v; }
        if let Some(v) = patch.enabled    { row.enabled = v; }
        if let Some(v) = patch.note       { row.note = v; }
        if !(1..=65535).contains(&row.port) { return Err(Error::invalid("port must be 1..=65535")); }
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query(
                    "UPDATE chain_proxies SET name=?, proxy_type=?, address=?, port=?, username=?, \
                       password=?, enabled=?, note=?, updated_at=? WHERE id=?",
                )
                .bind(&row.name).bind(row.proxy_type.as_str()).bind(&row.address)
                .bind(row.port).bind(row.username.as_deref()).bind(row.password.as_deref())
                .bind(row.enabled).bind(row.note.as_deref()).bind(now).bind(id)
                .execute(p).await?;
            }
            Database::Postgres(p) => {
                sqlx::query(
                    "UPDATE chain_proxies SET name=$1, proxy_type=$2, address=$3, port=$4, username=$5, \
                       password=$6, enabled=$7, note=$8, updated_at=$9 WHERE id=$10",
                )
                .bind(&row.name).bind(row.proxy_type.as_str()).bind(&row.address)
                .bind(row.port).bind(row.username.as_deref()).bind(row.password.as_deref())
                .bind(row.enabled).bind(row.note.as_deref()).bind(now).bind(id)
                .execute(p).await?;
            }
        }
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn delete(&self, id: i64) -> Result<bool> {
        let n = match &self.db {
            Database::Sqlite(p) => sqlx::query("DELETE FROM chain_proxies WHERE id = ?")
                .bind(id).execute(p).await?.rows_affected(),
            Database::Postgres(p) => sqlx::query("DELETE FROM chain_proxies WHERE id = $1")
                .bind(id).execute(p).await?.rows_affected(),
        };
        Ok(n > 0)
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<ChainProxy> {
    let kind: String = row.try_get("proxy_type")?;
    Ok(ChainProxy {
        id:         row.try_get("id")?,
        name:       row.try_get("name")?,
        proxy_type: ChainProxyType::parse(&kind)?,
        address:    row.try_get("address")?,
        port:       row.try_get("port")?,
        username:   row.try_get("username")?,
        password:   row.try_get("password")?,
        enabled:    row.try_get("enabled")?,
        note:       row.try_get("note")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn map_postgres(row: sqlx::postgres::PgRow) -> Result<ChainProxy> {
    let kind: String = row.try_get("proxy_type")?;
    Ok(ChainProxy {
        id:         row.try_get("id")?,
        name:       row.try_get("name")?,
        proxy_type: ChainProxyType::parse(&kind)?,
        address:    row.try_get("address")?,
        port:       row.try_get("port")?,
        username:   row.try_get("username")?,
        password:   row.try_get("password")?,
        enabled:    row.try_get("enabled")?,
        note:       row.try_get("note")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
