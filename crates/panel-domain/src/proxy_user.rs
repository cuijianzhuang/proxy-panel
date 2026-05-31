use chrono::{DateTime, Utc};
use panel_persistence::Database;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::plan::QuotaType;

#[derive(Debug, Clone, Serialize)]
pub struct ProxyUser {
    pub id:                 i64,
    pub name:               String,
    pub uuid:               String,
    /// Used by Trojan / Shadowsocks-2022 / Hysteria2 / TUIC. Ignored for VLESS/VMess.
    pub password:           String,
    pub plan_id:            Option<i64>,
    pub enabled:            bool,
    pub quota_type:         QuotaType,
    pub quota_gb:           f64,
    pub quota_reset_day:    i32,
    pub last_reset_at:      Option<DateTime<Utc>>,
    pub used_bytes:         i64,
    pub expires_at:         Option<DateTime<Utc>>,
    pub speed_limit_mbps:   Option<i32>,
    pub device_limit:       Option<i32>,
    /// Rotatable secret used by /sub/{token}.
    pub subscription_token: String,
    pub note:               Option<String>,
    pub tags:               Vec<String>,
    pub last_seen_at:       Option<DateTime<Utc>>,
    pub last_seen_ip:       Option<String>,
    pub created_at:         DateTime<Utc>,
    pub updated_at:         DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProxyUser {
    pub name:    String,
    /// Optional — generated as UUIDv4 if missing.
    #[serde(default)]
    pub uuid:    Option<String>,
    /// Optional — generated as 16 random hex bytes if missing.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub plan_id: Option<i64>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_quota_type")]
    pub quota_type: QuotaType,
    #[serde(default)]
    pub quota_gb: f64,
    #[serde(default = "first_of_month")]
    pub quota_reset_day: i32,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub speed_limit_mbps: Option<i32>,
    #[serde(default)]
    pub device_limit: Option<i32>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateProxyUser {
    pub name:             Option<String>,
    pub plan_id:          Option<Option<i64>>,
    pub enabled:          Option<bool>,
    pub quota_type:       Option<QuotaType>,
    pub quota_gb:         Option<f64>,
    pub quota_reset_day:  Option<i32>,
    pub expires_at:       Option<Option<DateTime<Utc>>>,
    pub speed_limit_mbps: Option<Option<i32>>,
    pub device_limit:     Option<Option<i32>>,
    pub note:             Option<Option<String>>,
    pub tags:             Option<Vec<String>>,
    /// Optional: rotate the subscription_token to invalidate the existing URL.
    pub rotate_subscription_token: Option<bool>,
}

fn yes() -> bool {
    true
}
fn default_quota_type() -> QuotaType {
    QuotaType::Permanent
}
fn first_of_month() -> i32 {
    1
}

/// Hex string suitable as a subscription URL secret. 32 random bytes → 64 chars.
pub fn random_subscription_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn random_password_hex() -> String {
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[derive(Clone)]
pub struct ProxyUserRepo {
    db: Database,
}

const COLS: &str = "id, name, uuid, password, plan_id, enabled, quota_type, quota_gb, \
                    quota_reset_day, last_reset_at, used_bytes, expires_at, \
                    speed_limit_mbps, device_limit, subscription_token, note, tags, \
                    last_seen_at, last_seen_ip, created_at, updated_at";

impl ProxyUserRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<ProxyUser>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM proxy_users ORDER BY id");
                sqlx::query(&sql)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_sqlite)
                    .collect()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM proxy_users ORDER BY id");
                sqlx::query(&sql)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_postgres)
                    .collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<ProxyUser>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM proxy_users WHERE id = ?");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_sqlite)
                    .transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM proxy_users WHERE id = $1");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_postgres)
                    .transpose()
            }
        }
    }

    pub async fn find_by_subscription_token(&self, token: &str) -> Result<Option<ProxyUser>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM proxy_users WHERE subscription_token = ?");
                sqlx::query(&sql)
                    .bind(token)
                    .fetch_optional(pool)
                    .await?
                    .map(map_sqlite)
                    .transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM proxy_users WHERE subscription_token = $1");
                sqlx::query(&sql)
                    .bind(token)
                    .fetch_optional(pool)
                    .await?
                    .map(map_postgres)
                    .transpose()
            }
        }
    }

    /// List proxy users attached to a listener (via `listener_clients`).
    pub async fn list_for_listener(&self, listener_id: i64) -> Result<Vec<ProxyUser>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!(
                    "SELECT {COLS} FROM proxy_users u \
                     INNER JOIN listener_clients lc ON lc.proxy_user_id = u.id \
                     WHERE lc.listener_id = ? AND u.enabled = 1 \
                     ORDER BY u.id"
                );
                sqlx::query(&sql)
                    .bind(listener_id)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_sqlite)
                    .collect()
            }
            Database::Postgres(pool) => {
                let sql = format!(
                    "SELECT {COLS} FROM proxy_users u \
                     INNER JOIN listener_clients lc ON lc.proxy_user_id = u.id \
                     WHERE lc.listener_id = $1 AND u.enabled = TRUE \
                     ORDER BY u.id"
                );
                sqlx::query(&sql)
                    .bind(listener_id)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_postgres)
                    .collect()
            }
        }
    }

    /// List listener ids attached to a proxy user.
    pub async fn listener_ids_for_user(&self, user_id: i64) -> Result<Vec<i64>> {
        let ids = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "SELECT listener_id FROM listener_clients WHERE proxy_user_id = ? ORDER BY listener_id",
            )
            .bind(user_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| r.try_get::<i64, _>("listener_id"))
            .collect::<std::result::Result<Vec<_>, _>>()?,
            Database::Postgres(pool) => sqlx::query(
                "SELECT listener_id FROM listener_clients WHERE proxy_user_id = $1 ORDER BY listener_id",
            )
            .bind(user_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| r.try_get::<i64, _>("listener_id"))
            .collect::<std::result::Result<Vec<_>, _>>()?,
        };
        Ok(ids)
    }

    pub async fn create(&self, input: CreateProxyUser) -> Result<ProxyUser> {
        if input.name.trim().is_empty() {
            return Err(Error::invalid("name is required"));
        }
        if input.quota_gb < 0.0 {
            return Err(Error::invalid("quota_gb must be >= 0"));
        }

        let uuid_value = input
            .uuid
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let password = input.password.clone().unwrap_or_else(random_password_hex);
        let sub_token = random_subscription_token();
        let tags_json = serde_json::to_value(&input.tags).unwrap_or(serde_json::json!([]));

        let id = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "INSERT INTO proxy_users \
                   (name, uuid, password, plan_id, enabled, quota_type, quota_gb, \
                    quota_reset_day, expires_at, speed_limit_mbps, device_limit, \
                    subscription_token, note, tags) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&input.name)
            .bind(&uuid_value)
            .bind(&password)
            .bind(input.plan_id)
            .bind(input.enabled)
            .bind(input.quota_type.as_str())
            .bind(input.quota_gb)
            .bind(input.quota_reset_day)
            .bind(input.expires_at)
            .bind(input.speed_limit_mbps)
            .bind(input.device_limit)
            .bind(&sub_token)
            .bind(input.note.as_deref())
            .bind(Json(&tags_json))
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
            Database::Postgres(pool) => sqlx::query(
                "INSERT INTO proxy_users \
                   (name, uuid, password, plan_id, enabled, quota_type, quota_gb, \
                    quota_reset_day, expires_at, speed_limit_mbps, device_limit, \
                    subscription_token, note, tags) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) RETURNING id",
            )
            .bind(&input.name)
            .bind(&uuid_value)
            .bind(&password)
            .bind(input.plan_id)
            .bind(input.enabled)
            .bind(input.quota_type.as_str())
            .bind(input.quota_gb)
            .bind(input.quota_reset_day)
            .bind(input.expires_at)
            .bind(input.speed_limit_mbps)
            .bind(input.device_limit)
            .bind(&sub_token)
            .bind(input.note.as_deref())
            .bind(Json(&tags_json))
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    /// Add bytes to a user's running `used_bytes` counter (called by the
    /// traffic collector each cycle).
    pub async fn add_used_bytes(&self, id: i64, bytes: i64) -> Result<()> {
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query("UPDATE proxy_users SET used_bytes = used_bytes + ?, last_seen_at = ?, updated_at = ? WHERE id = ?")
                    .bind(bytes).bind(now).bind(now).bind(id).execute(p).await?;
            }
            Database::Postgres(p) => {
                sqlx::query("UPDATE proxy_users SET used_bytes = used_bytes + $1, last_seen_at = $2, updated_at = $2 WHERE id = $3")
                    .bind(bytes).bind(now).bind(id).execute(p).await?;
            }
        }
        Ok(())
    }

    /// Force a user enabled/disabled (collector uses this to cut off over-quota
    /// users). Returns the updated row.
    pub async fn set_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(p) => {
                sqlx::query("UPDATE proxy_users SET enabled = ?, updated_at = ? WHERE id = ?")
                    .bind(enabled).bind(now).bind(id).execute(p).await?;
            }
            Database::Postgres(p) => {
                sqlx::query("UPDATE proxy_users SET enabled = $1, updated_at = $2 WHERE id = $3")
                    .bind(enabled).bind(now).bind(id).execute(p).await?;
            }
        }
        Ok(())
    }

    pub async fn update(&self, id: i64, patch: UpdateProxyUser) -> Result<ProxyUser> {
        let existing = self.find(id).await?.ok_or(Error::NotFound)?;
        let mut next = existing.clone();
        if let Some(v) = patch.name {
            next.name = v;
        }
        if let Some(v) = patch.plan_id {
            next.plan_id = v;
        }
        if let Some(v) = patch.enabled {
            next.enabled = v;
        }
        if let Some(v) = patch.quota_type {
            next.quota_type = v;
        }
        if let Some(v) = patch.quota_gb {
            next.quota_gb = v;
        }
        if let Some(v) = patch.quota_reset_day {
            next.quota_reset_day = v;
        }
        if let Some(v) = patch.expires_at {
            next.expires_at = v;
        }
        if let Some(v) = patch.speed_limit_mbps {
            next.speed_limit_mbps = v;
        }
        if let Some(v) = patch.device_limit {
            next.device_limit = v;
        }
        if let Some(v) = patch.note {
            next.note = v;
        }
        if let Some(v) = patch.tags {
            next.tags = v;
        }
        if patch.rotate_subscription_token.unwrap_or(false) {
            next.subscription_token = random_subscription_token();
        }
        if next.quota_gb < 0.0 {
            return Err(Error::invalid("quota_gb must be >= 0"));
        }

        let now = Utc::now();
        let tags_json = serde_json::to_value(&next.tags).unwrap_or(serde_json::json!([]));
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE proxy_users SET name=?, plan_id=?, enabled=?, quota_type=?, \
                       quota_gb=?, quota_reset_day=?, expires_at=?, speed_limit_mbps=?, \
                       device_limit=?, subscription_token=?, note=?, tags=?, updated_at=? \
                     WHERE id=?",
                )
                .bind(&next.name)
                .bind(next.plan_id)
                .bind(next.enabled)
                .bind(next.quota_type.as_str())
                .bind(next.quota_gb)
                .bind(next.quota_reset_day)
                .bind(next.expires_at)
                .bind(next.speed_limit_mbps)
                .bind(next.device_limit)
                .bind(&next.subscription_token)
                .bind(next.note.as_deref())
                .bind(Json(&tags_json))
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE proxy_users SET name=$1, plan_id=$2, enabled=$3, quota_type=$4, \
                       quota_gb=$5, quota_reset_day=$6, expires_at=$7, speed_limit_mbps=$8, \
                       device_limit=$9, subscription_token=$10, note=$11, tags=$12, updated_at=$13 \
                     WHERE id=$14",
                )
                .bind(&next.name)
                .bind(next.plan_id)
                .bind(next.enabled)
                .bind(next.quota_type.as_str())
                .bind(next.quota_gb)
                .bind(next.quota_reset_day)
                .bind(next.expires_at)
                .bind(next.speed_limit_mbps)
                .bind(next.device_limit)
                .bind(&next.subscription_token)
                .bind(next.note.as_deref())
                .bind(Json(&tags_json))
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
            Database::Sqlite(pool) => sqlx::query("DELETE FROM proxy_users WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
            Database::Postgres(pool) => sqlx::query("DELETE FROM proxy_users WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
        };
        Ok(n > 0)
    }

    /// Zero the user's traffic counter and record the reset time.
    pub async fn reset_traffic(&self, id: i64) -> Result<ProxyUser> {
        let now = chrono::Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE proxy_users SET used_bytes = 0, last_reset_at = ?, \
                     updated_at = ? WHERE id = ?",
                )
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE proxy_users SET used_bytes = 0, last_reset_at = $1, \
                     updated_at = $2 WHERE id = $3",
                )
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
        }
        self.find(id).await?.ok_or(Error::NotFound)
    }
}

// ============================================================================
// Row mapping
// ============================================================================

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<ProxyUser> {
    let qt: String = row.try_get("quota_type")?;
    let tags: Json<serde_json::Value> = row.try_get("tags")?;
    Ok(ProxyUser {
        id:                 row.try_get("id")?,
        name:               row.try_get("name")?,
        uuid:               row.try_get("uuid")?,
        password:           row.try_get("password")?,
        plan_id:            row.try_get("plan_id")?,
        enabled:            row.try_get("enabled")?,
        quota_type:         QuotaType::parse(&qt)?,
        quota_gb:           row.try_get("quota_gb")?,
        quota_reset_day:    row.try_get("quota_reset_day")?,
        last_reset_at:      row.try_get("last_reset_at")?,
        used_bytes:         row.try_get("used_bytes")?,
        expires_at:         row.try_get("expires_at")?,
        speed_limit_mbps:   row.try_get("speed_limit_mbps")?,
        device_limit:       row.try_get("device_limit")?,
        subscription_token: row.try_get("subscription_token")?,
        note:               row.try_get("note")?,
        tags:               json_array_to_strings(tags.0),
        last_seen_at:       row.try_get("last_seen_at")?,
        last_seen_ip:       row.try_get("last_seen_ip")?,
        created_at:         row.try_get("created_at")?,
        updated_at:         row.try_get("updated_at")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> Result<ProxyUser> {
    let qt: String = row.try_get("quota_type")?;
    let tags: Json<serde_json::Value> = row.try_get("tags")?;
    Ok(ProxyUser {
        id:                 row.try_get("id")?,
        name:               row.try_get("name")?,
        uuid:               row.try_get("uuid")?,
        password:           row.try_get("password")?,
        plan_id:            row.try_get("plan_id")?,
        enabled:            row.try_get("enabled")?,
        quota_type:         QuotaType::parse(&qt)?,
        quota_gb:           row.try_get("quota_gb")?,
        quota_reset_day:    row.try_get("quota_reset_day")?,
        last_reset_at:      row.try_get("last_reset_at")?,
        used_bytes:         row.try_get("used_bytes")?,
        expires_at:         row.try_get("expires_at")?,
        speed_limit_mbps:   row.try_get("speed_limit_mbps")?,
        device_limit:       row.try_get("device_limit")?,
        subscription_token: row.try_get("subscription_token")?,
        note:               row.try_get("note")?,
        tags:               json_array_to_strings(tags.0),
        last_seen_at:       row.try_get("last_seen_at")?,
        last_seen_ip:       row.try_get("last_seen_ip")?,
        created_at:         row.try_get("created_at")?,
        updated_at:         row.try_get("updated_at")?,
    })
}

fn json_array_to_strings(v: serde_json::Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}
