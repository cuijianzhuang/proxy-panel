use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::Row;

use crate::error::{Error, Result};

/// Event types the panel can fire. Kept as a fixed list so the UI can show a
/// stable rules table; unknown strings from the DB still round-trip as-is.
pub const EVENT_TYPES: &[&str] = &[
    "node_offline",     // a node stopped responding
    "node_deployed",    // auto-deploy / apply finished
    "quota_exceed",     // a proxy user blew past their quota
    "backup",           // a backup completed (or failed)
    "task_failed",      // a node_operation_task ended in failed
    "cert_expiring",    // TLS cert within N days of expiry
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    Telegram,
    Webhook,
    Smtp,
}

impl ChannelType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Webhook => "webhook",
            Self::Smtp => "smtp",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "telegram" => Ok(Self::Telegram),
            "webhook" => Ok(Self::Webhook),
            "smtp" => Ok(Self::Smtp),
            other => Err(Error::invalid(format!("unknown channel type: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NotificationChannel {
    pub id:         i64,
    pub name:       String,
    #[serde(rename = "type")]
    pub kind:       ChannelType,
    /// Type-specific config. Secrets (bot_token, smtp password) are included;
    /// the API layer is responsible for any redaction it wants on read.
    pub config:     serde_json::Value,
    pub enabled:    bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateChannel {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ChannelType,
    #[serde(default = "empty_object")]
    pub config: serde_json::Value,
    #[serde(default = "yes")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateChannel {
    pub name:    Option<String>,
    pub config:  Option<serde_json::Value>,
    pub enabled: Option<bool>,
}

fn empty_object() -> serde_json::Value { serde_json::json!({}) }
fn yes() -> bool { true }

const COLS: &str = "id, name, type, config_json, enabled, created_at, updated_at";

#[derive(Clone)]
pub struct NotificationChannelRepo { db: Database }

impl NotificationChannelRepo {
    pub fn new(db: Database) -> Self { Self { db } }

    pub async fn list(&self) -> Result<Vec<NotificationChannel>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM notification_channels ORDER BY id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_sqlite).collect()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM notification_channels ORDER BY id");
                sqlx::query(&sql).fetch_all(p).await?.into_iter().map(map_postgres).collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<NotificationChannel>> {
        match &self.db {
            Database::Sqlite(p) => {
                let sql = format!("SELECT {COLS} FROM notification_channels WHERE id = ?");
                sqlx::query(&sql).bind(id).fetch_optional(p).await?.map(map_sqlite).transpose()
            }
            Database::Postgres(p) => {
                let sql = format!("SELECT {COLS} FROM notification_channels WHERE id = $1");
                sqlx::query(&sql).bind(id).fetch_optional(p).await?.map(map_postgres).transpose()
            }
        }
    }

    /// Fetch many channels by id, preserving only the enabled ones. Used by
    /// the dispatcher to materialise a rule's target list.
    pub async fn find_enabled_by_ids(&self, ids: &[i64]) -> Result<Vec<NotificationChannel>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        // Small N; just fetch all and filter in memory rather than build an IN clause.
        let all = self.list().await?;
        Ok(all
            .into_iter()
            .filter(|c| c.enabled && ids.contains(&c.id))
            .collect())
    }

    pub async fn create(&self, input: CreateChannel) -> Result<NotificationChannel> {
        if input.name.trim().is_empty() {
            return Err(Error::invalid("name is required"));
        }
        let id = match &self.db {
            Database::Sqlite(p) => sqlx::query(
                "INSERT INTO notification_channels (name, type, config_json, enabled) \
                 VALUES (?, ?, ?, ?) RETURNING id",
            )
            .bind(&input.name).bind(input.kind.as_str())
            .bind(Json(&input.config)).bind(input.enabled)
            .fetch_one(p).await?.try_get::<i64, _>("id")?,
            Database::Postgres(p) => sqlx::query(
                "INSERT INTO notification_channels (name, type, config_json, enabled) \
                 VALUES ($1, $2, $3, $4) RETURNING id",
            )
            .bind(&input.name).bind(input.kind.as_str())
            .bind(Json(&input.config)).bind(input.enabled)
            .fetch_one(p).await?.try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(&self, id: i64, patch: UpdateChannel) -> Result<NotificationChannel> {
        let mut row = self.find(id).await?.ok_or(Error::NotFound)?;
        if let Some(v) = patch.name    { row.name = v; }
        if let Some(v) = patch.config  { row.config = v; }
        if let Some(v) = patch.enabled { row.enabled = v; }
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query("UPDATE notification_channels SET name=?, config_json=?, enabled=?, updated_at=? WHERE id=?")
                    .bind(&row.name).bind(Json(&row.config)).bind(row.enabled).bind(now).bind(id)
                    .execute(p).await?;
            }
            Database::Postgres(p) => {
                sqlx::query("UPDATE notification_channels SET name=$1, config_json=$2, enabled=$3, updated_at=$4 WHERE id=$5")
                    .bind(&row.name).bind(Json(&row.config)).bind(row.enabled).bind(now).bind(id)
                    .execute(p).await?;
            }
        }
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn delete(&self, id: i64) -> Result<bool> {
        let n = match &self.db {
            Database::Sqlite(p) => sqlx::query("DELETE FROM notification_channels WHERE id = ?")
                .bind(id).execute(p).await?.rows_affected(),
            Database::Postgres(p) => sqlx::query("DELETE FROM notification_channels WHERE id = $1")
                .bind(id).execute(p).await?.rows_affected(),
        };
        Ok(n > 0)
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<NotificationChannel> {
    let kind: String = row.try_get("type")?;
    let config: Json<serde_json::Value> = row.try_get("config_json")?;
    Ok(NotificationChannel {
        id:         row.try_get("id")?,
        name:       row.try_get("name")?,
        kind:       ChannelType::parse(&kind)?,
        config:     config.0,
        enabled:    row.try_get("enabled")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
fn map_postgres(row: sqlx::postgres::PgRow) -> Result<NotificationChannel> {
    let kind: String = row.try_get("type")?;
    let config: Json<serde_json::Value> = row.try_get("config_json")?;
    Ok(NotificationChannel {
        id:         row.try_get("id")?,
        name:       row.try_get("name")?,
        kind:       ChannelType::parse(&kind)?,
        config:     config.0,
        enabled:    row.try_get("enabled")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

// ===========================================================================
// Rules
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct NotificationRule {
    pub event_type:  String,
    pub channel_ids: Vec<i64>,
    pub enabled:     bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpsertRule {
    pub channel_ids: Vec<i64>,
    #[serde(default = "yes")]
    pub enabled:     bool,
}

#[derive(Clone)]
pub struct NotificationRuleRepo { db: Database }

impl NotificationRuleRepo {
    pub fn new(db: Database) -> Self { Self { db } }

    /// List all stored rules, then synthesise empty defaults for any known
    /// `EVENT_TYPES` not yet persisted — so the UI always shows the full set.
    pub async fn list(&self) -> Result<Vec<NotificationRule>> {
        let stored = self.list_stored().await?;
        let mut out: Vec<NotificationRule> = Vec::new();
        for ev in EVENT_TYPES {
            if let Some(r) = stored.iter().find(|r| r.event_type == *ev) {
                out.push(r.clone());
            } else {
                out.push(NotificationRule {
                    event_type: (*ev).to_string(),
                    channel_ids: vec![],
                    enabled: true,
                });
            }
        }
        // Include any stored rows for event types not in the known list.
        for r in stored {
            if !EVENT_TYPES.contains(&r.event_type.as_str()) {
                out.push(r);
            }
        }
        Ok(out)
    }

    async fn list_stored(&self) -> Result<Vec<NotificationRule>> {
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query("SELECT event_type, channel_ids, enabled FROM notification_rules")
                    .fetch_all(p).await?.into_iter().map(map_rule_sqlite).collect()
            }
            Database::Postgres(p) => {
                sqlx::query("SELECT event_type, channel_ids, enabled FROM notification_rules")
                    .fetch_all(p).await?.into_iter().map(map_rule_postgres).collect()
            }
        }
    }

    pub async fn get(&self, event_type: &str) -> Result<NotificationRule> {
        let stored = self.list_stored().await?;
        Ok(stored
            .into_iter()
            .find(|r| r.event_type == event_type)
            .unwrap_or(NotificationRule {
                event_type: event_type.to_string(),
                channel_ids: vec![],
                enabled: true,
            }))
    }

    pub async fn upsert(&self, event_type: &str, input: UpsertRule) -> Result<NotificationRule> {
        let ids_json = serde_json::to_value(&input.channel_ids).unwrap_or(serde_json::json!([]));
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query(
                    "INSERT INTO notification_rules (event_type, channel_ids, enabled, updated_at) \
                     VALUES (?, ?, ?, ?) \
                     ON CONFLICT(event_type) DO UPDATE SET channel_ids=excluded.channel_ids, \
                       enabled=excluded.enabled, updated_at=excluded.updated_at",
                )
                .bind(event_type).bind(Json(&ids_json)).bind(input.enabled).bind(now)
                .execute(p).await?;
            }
            Database::Postgres(p) => {
                sqlx::query(
                    "INSERT INTO notification_rules (event_type, channel_ids, enabled, updated_at) \
                     VALUES ($1, $2, $3, $4) \
                     ON CONFLICT(event_type) DO UPDATE SET channel_ids=excluded.channel_ids, \
                       enabled=excluded.enabled, updated_at=excluded.updated_at",
                )
                .bind(event_type).bind(Json(&ids_json)).bind(input.enabled).bind(now)
                .execute(p).await?;
            }
        }
        self.get(event_type).await
    }
}

fn map_rule_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<NotificationRule> {
    let ids: Json<serde_json::Value> = row.try_get("channel_ids")?;
    Ok(NotificationRule {
        event_type:  row.try_get("event_type")?,
        channel_ids: json_to_ids(ids.0),
        enabled:     row.try_get("enabled")?,
    })
}
fn map_rule_postgres(row: sqlx::postgres::PgRow) -> Result<NotificationRule> {
    let ids: Json<serde_json::Value> = row.try_get("channel_ids")?;
    Ok(NotificationRule {
        event_type:  row.try_get("event_type")?,
        channel_ids: json_to_ids(ids.0),
        enabled:     row.try_get("enabled")?,
    })
}

fn json_to_ids(v: serde_json::Value) -> Vec<i64> {
    v.as_array()
        .map(|a| a.iter().filter_map(|x| x.as_i64()).collect())
        .unwrap_or_default()
}
