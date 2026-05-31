use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuotaType {
    Permanent,
    Monthly,
}

impl QuotaType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Permanent => "permanent",
            Self::Monthly => "monthly",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "permanent" => Ok(Self::Permanent),
            "monthly" => Ok(Self::Monthly),
            other => Err(Error::invalid(format!("unknown quota_type: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub id:                i64,
    pub name:              String,
    pub quota_type:        QuotaType,
    /// 0 = unlimited.
    pub quota_gb:          f64,
    pub quota_reset_day:   i32,
    pub duration_days:     Option<i32>,
    pub device_limit:      Option<i32>,
    pub speed_limit_mbps:  Option<i32>,
    pub created_at:        DateTime<Utc>,
    pub updated_at:        DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreatePlan {
    pub name:              String,
    #[serde(default = "default_quota_type")]
    pub quota_type:        QuotaType,
    #[serde(default)]
    pub quota_gb:          f64,
    #[serde(default = "first_of_month")]
    pub quota_reset_day:   i32,
    pub duration_days:     Option<i32>,
    pub device_limit:      Option<i32>,
    pub speed_limit_mbps:  Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdatePlan {
    pub name:              Option<String>,
    pub quota_type:        Option<QuotaType>,
    pub quota_gb:          Option<f64>,
    pub quota_reset_day:   Option<i32>,
    pub duration_days:     Option<Option<i32>>,
    pub device_limit:      Option<Option<i32>>,
    pub speed_limit_mbps:  Option<Option<i32>>,
}

fn default_quota_type() -> QuotaType {
    QuotaType::Permanent
}
fn first_of_month() -> i32 {
    1
}

impl CreatePlan {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(Error::invalid("name is required"));
        }
        if self.quota_gb < 0.0 {
            return Err(Error::invalid("quota_gb must be >= 0"));
        }
        if !(1..=28).contains(&self.quota_reset_day) {
            return Err(Error::invalid("quota_reset_day must be 1..=28"));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct PlanRepo {
    db: Database,
}

const COLS: &str = "id, name, quota_type, quota_gb, quota_reset_day, duration_days, \
                    device_limit, speed_limit_mbps, created_at, updated_at";

impl PlanRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<Plan>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM plans ORDER BY id");
                sqlx::query(&sql)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_sqlite)
                    .collect()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM plans ORDER BY id");
                sqlx::query(&sql)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_postgres)
                    .collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<Plan>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM plans WHERE id = ?");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_sqlite)
                    .transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM plans WHERE id = $1");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_postgres)
                    .transpose()
            }
        }
    }

    pub async fn create(&self, input: CreatePlan) -> Result<Plan> {
        input.validate()?;
        let id = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "INSERT INTO plans \
                   (name, quota_type, quota_gb, quota_reset_day, duration_days, device_limit, speed_limit_mbps) \
                 VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&input.name)
            .bind(input.quota_type.as_str())
            .bind(input.quota_gb)
            .bind(input.quota_reset_day)
            .bind(input.duration_days)
            .bind(input.device_limit)
            .bind(input.speed_limit_mbps)
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
            Database::Postgres(pool) => sqlx::query(
                "INSERT INTO plans \
                   (name, quota_type, quota_gb, quota_reset_day, duration_days, device_limit, speed_limit_mbps) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
            )
            .bind(&input.name)
            .bind(input.quota_type.as_str())
            .bind(input.quota_gb)
            .bind(input.quota_reset_day)
            .bind(input.duration_days)
            .bind(input.device_limit)
            .bind(input.speed_limit_mbps)
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(Error::NotFound)
    }

    pub async fn update(&self, id: i64, patch: UpdatePlan) -> Result<Plan> {
        let existing = self.find(id).await?.ok_or(Error::NotFound)?;
        let next = Plan {
            id:                existing.id,
            name:              patch.name.unwrap_or(existing.name),
            quota_type:        patch.quota_type.unwrap_or(existing.quota_type),
            quota_gb:          patch.quota_gb.unwrap_or(existing.quota_gb),
            quota_reset_day:   patch.quota_reset_day.unwrap_or(existing.quota_reset_day),
            duration_days:     patch.duration_days.unwrap_or(existing.duration_days),
            device_limit:      patch.device_limit.unwrap_or(existing.device_limit),
            speed_limit_mbps:  patch.speed_limit_mbps.unwrap_or(existing.speed_limit_mbps),
            created_at:        existing.created_at,
            updated_at:        existing.updated_at,
        };
        if next.quota_gb < 0.0 {
            return Err(Error::invalid("quota_gb must be >= 0"));
        }
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE plans SET name=?, quota_type=?, quota_gb=?, quota_reset_day=?, \
                       duration_days=?, device_limit=?, speed_limit_mbps=?, updated_at=? WHERE id=?",
                )
                .bind(&next.name)
                .bind(next.quota_type.as_str())
                .bind(next.quota_gb)
                .bind(next.quota_reset_day)
                .bind(next.duration_days)
                .bind(next.device_limit)
                .bind(next.speed_limit_mbps)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE plans SET name=$1, quota_type=$2, quota_gb=$3, quota_reset_day=$4, \
                       duration_days=$5, device_limit=$6, speed_limit_mbps=$7, updated_at=$8 WHERE id=$9",
                )
                .bind(&next.name)
                .bind(next.quota_type.as_str())
                .bind(next.quota_gb)
                .bind(next.quota_reset_day)
                .bind(next.duration_days)
                .bind(next.device_limit)
                .bind(next.speed_limit_mbps)
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
            Database::Sqlite(pool) => sqlx::query("DELETE FROM plans WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
            Database::Postgres(pool) => sqlx::query("DELETE FROM plans WHERE id = $1")
                .bind(id)
                .execute(pool)
                .await?
                .rows_affected(),
        };
        Ok(n > 0)
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<Plan> {
    let qt: String = row.try_get("quota_type")?;
    Ok(Plan {
        id:                row.try_get("id")?,
        name:              row.try_get("name")?,
        quota_type:        QuotaType::parse(&qt)?,
        quota_gb:          row.try_get("quota_gb")?,
        quota_reset_day:   row.try_get("quota_reset_day")?,
        duration_days:     row.try_get("duration_days")?,
        device_limit:      row.try_get("device_limit")?,
        speed_limit_mbps:  row.try_get("speed_limit_mbps")?,
        created_at:        row.try_get("created_at")?,
        updated_at:        row.try_get("updated_at")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> Result<Plan> {
    let qt: String = row.try_get("quota_type")?;
    Ok(Plan {
        id:                row.try_get("id")?,
        name:              row.try_get("name")?,
        quota_type:        QuotaType::parse(&qt)?,
        quota_gb:          row.try_get("quota_gb")?,
        quota_reset_day:   row.try_get("quota_reset_day")?,
        duration_days:     row.try_get("duration_days")?,
        device_limit:      row.try_get("device_limit")?,
        speed_limit_mbps:  row.try_get("speed_limit_mbps")?,
        created_at:        row.try_get("created_at")?,
        updated_at:        row.try_get("updated_at")?,
    })
}
