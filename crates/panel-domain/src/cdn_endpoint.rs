use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CdnKind {
    Domain,
    Ip,
}

impl CdnKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Ip => "ip",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "domain" => Ok(Self::Domain),
            "ip" => Ok(Self::Ip),
            other => Err(Error::invalid(format!("unknown cdn kind: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CdnEndpoint {
    pub id:         i64,
    pub name:       String,
    pub address:    String,
    pub kind:       CdnKind,
    pub enabled:    bool,
    pub sort_order: i32,
    pub note:       Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCdnEndpoint {
    pub name:    String,
    pub address: String,
    #[serde(default = "default_kind")]
    pub kind:    CdnKind,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_sort")]
    pub sort_order: i32,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateCdnEndpoint {
    pub name:       Option<String>,
    pub address:    Option<String>,
    pub kind:       Option<CdnKind>,
    pub enabled:    Option<bool>,
    pub sort_order: Option<i32>,
    pub note:       Option<Option<String>>,
}

fn default_kind() -> CdnKind { CdnKind::Domain }
fn default_sort() -> i32 { 100 }
fn yes() -> bool { true }

impl CreateCdnEndpoint {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() { return Err(Error::invalid("name is required")); }
        if self.address.trim().is_empty() { return Err(Error::invalid("address is required")); }
        Ok(())
    }
}

const COLS: &str = "id, name, address, kind, enabled, sort_order, note, created_at, updated_at";

#[derive(Clone)]
pub struct CdnEndpointRepo { db: Database }

impl CdnEndpointRepo {
    pub fn new(db: Database) -> Self { Self { db } }

    pub async fn list(&self) -> Result<Vec<CdnEndpoint>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM cdn_endpoints ORDER BY sort_order, id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_sqlite).collect()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM cdn_endpoints ORDER BY sort_order, id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_postgres).collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<CdnEndpoint>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM cdn_endpoints WHERE id = ?");
                sqlx::query(&sql).bind(id).fetch_optional(p).await?.map(map_sqlite).transpose()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM cdn_endpoints WHERE id = $1");
                sqlx::query(&sql).bind(id).fetch_optional(p).await?.map(map_postgres).transpose()
            }
        }
    }

    pub async fn create(&self, input: CreateCdnEndpoint) -> Result<CdnEndpoint> {
        input.validate()?;
        let id = match &self.db {
            Database::Sqlite(p) => sqlx::query(
                "INSERT INTO cdn_endpoints (name, address, kind, enabled, sort_order, note) \
                 VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&input.name).bind(&input.address).bind(input.kind.as_str())
            .bind(input.enabled).bind(input.sort_order).bind(input.note.as_deref())
            .fetch_one(p).await?.try_get::<i64, _>("id")?,
            Database::Postgres(p) => sqlx::query(
                "INSERT INTO cdn_endpoints (name, address, kind, enabled, sort_order, note) \
                 VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            )
            .bind(&input.name).bind(&input.address).bind(input.kind.as_str())
            .bind(input.enabled).bind(input.sort_order).bind(input.note.as_deref())
            .fetch_one(p).await?.try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(&self, id: i64, patch: UpdateCdnEndpoint) -> Result<CdnEndpoint> {
        let mut row = self.find(id).await?.ok_or(Error::NotFound)?;
        if let Some(v) = patch.name       { row.name = v; }
        if let Some(v) = patch.address    { row.address = v; }
        if let Some(v) = patch.kind       { row.kind = v; }
        if let Some(v) = patch.enabled    { row.enabled = v; }
        if let Some(v) = patch.sort_order { row.sort_order = v; }
        if let Some(v) = patch.note       { row.note = v; }
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query(
                    "UPDATE cdn_endpoints SET name=?, address=?, kind=?, enabled=?, sort_order=?, \
                       note=?, updated_at=? WHERE id=?",
                )
                .bind(&row.name).bind(&row.address).bind(row.kind.as_str())
                .bind(row.enabled).bind(row.sort_order).bind(row.note.as_deref())
                .bind(now).bind(id)
                .execute(p).await?;
            }
            Database::Postgres(p) => {
                sqlx::query(
                    "UPDATE cdn_endpoints SET name=$1, address=$2, kind=$3, enabled=$4, sort_order=$5, \
                       note=$6, updated_at=$7 WHERE id=$8",
                )
                .bind(&row.name).bind(&row.address).bind(row.kind.as_str())
                .bind(row.enabled).bind(row.sort_order).bind(row.note.as_deref())
                .bind(now).bind(id)
                .execute(p).await?;
            }
        }
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn delete(&self, id: i64) -> Result<bool> {
        let n = match &self.db {
            Database::Sqlite(p) => sqlx::query("DELETE FROM cdn_endpoints WHERE id = ?")
                .bind(id).execute(p).await?.rows_affected(),
            Database::Postgres(p) => sqlx::query("DELETE FROM cdn_endpoints WHERE id = $1")
                .bind(id).execute(p).await?.rows_affected(),
        };
        Ok(n > 0)
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<CdnEndpoint> {
    let kind: String = row.try_get("kind")?;
    Ok(CdnEndpoint {
        id:         row.try_get("id")?,
        name:       row.try_get("name")?,
        address:    row.try_get("address")?,
        kind:       CdnKind::parse(&kind)?,
        enabled:    row.try_get("enabled")?,
        sort_order: row.try_get("sort_order")?,
        note:       row.try_get("note")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn map_postgres(row: sqlx::postgres::PgRow) -> Result<CdnEndpoint> {
    let kind: String = row.try_get("kind")?;
    Ok(CdnEndpoint {
        id:         row.try_get("id")?,
        name:       row.try_get("name")?,
        address:    row.try_get("address")?,
        kind:       CdnKind::parse(&kind)?,
        enabled:    row.try_get("enabled")?,
        sort_order: row.try_get("sort_order")?,
        note:       row.try_get("note")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
