use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::Serialize;
use sqlx::Row;

use crate::error::Result;

/// One traffic increment to persist.
#[derive(Debug, Clone)]
pub struct NewSample {
    pub node_id:       i64,
    pub proxy_user_id: i64,
    pub up_delta:      i64,
    pub down_delta:    i64,
}

/// Per-user lifetime totals, summed from `stats_samples`.
#[derive(Debug, Clone, Serialize)]
pub struct UserTotal {
    pub proxy_user_id: i64,
    pub up:            i64,
    pub down:          i64,
}

/// One day's total across all users (for the dashboard chart).
#[derive(Debug, Clone, Serialize)]
pub struct DailyPoint {
    pub day:   String,
    pub up:    i64,
    pub down:  i64,
}

#[derive(Clone)]
pub struct StatsRepo {
    db: Database,
}

impl StatsRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Batch-insert samples. Skips zero-delta rows to keep the table lean.
    pub async fn insert_samples(&self, samples: &[NewSample]) -> Result<()> {
        for s in samples {
            if s.up_delta == 0 && s.down_delta == 0 {
                continue;
            }
            match &self.db {
                Database::Sqlite(p) => {
                    sqlx::query(
                        "INSERT INTO stats_samples (node_id, proxy_user_id, up_delta, down_delta) \
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(s.node_id).bind(s.proxy_user_id).bind(s.up_delta).bind(s.down_delta)
                    .execute(p).await?;
                }
                Database::Postgres(p) => {
                    sqlx::query(
                        "INSERT INTO stats_samples (node_id, proxy_user_id, up_delta, down_delta) \
                         VALUES ($1, $2, $3, $4)",
                    )
                    .bind(s.node_id).bind(s.proxy_user_id).bind(s.up_delta).bind(s.down_delta)
                    .execute(p).await?;
                }
            }
        }
        Ok(())
    }

    /// Lifetime up/down per user across all samples.
    pub async fn user_totals(&self) -> Result<Vec<UserTotal>> {
        let sql = "SELECT proxy_user_id, \
                          COALESCE(SUM(up_delta), 0)   AS up, \
                          COALESCE(SUM(down_delta), 0) AS down \
                   FROM stats_samples GROUP BY proxy_user_id";
        let rows = match &self.db {
            Database::Sqlite(p) => sqlx::query(sql).fetch_all(p).await?
                .into_iter().map(|r| -> Result<UserTotal> {
                    Ok(UserTotal {
                        proxy_user_id: r.try_get("proxy_user_id")?,
                        up: r.try_get("up")?, down: r.try_get("down")?,
                    })
                }).collect::<Result<Vec<_>>>()?,
            Database::Postgres(p) => sqlx::query(sql).fetch_all(p).await?
                .into_iter().map(|r| -> Result<UserTotal> {
                    Ok(UserTotal {
                        proxy_user_id: r.try_get("proxy_user_id")?,
                        up: r.try_get("up")?, down: r.try_get("down")?,
                    })
                }).collect::<Result<Vec<_>>>()?,
        };
        Ok(rows)
    }

    /// Total up/down grouped by day (UTC) over the last `days` days.
    pub async fn daily_series(&self, days: i64) -> Result<Vec<DailyPoint>> {
        let days = days.clamp(1, 365);
        let rows = match &self.db {
            Database::Sqlite(p) => {
                // substr(ts,1,10) = YYYY-MM-DD; ts is stored as ISO text.
                let sql = format!(
                    "SELECT substr(ts,1,10) AS day, \
                            COALESCE(SUM(up_delta),0) AS up, \
                            COALESCE(SUM(down_delta),0) AS down \
                     FROM stats_samples \
                     WHERE ts >= strftime('%Y-%m-%dT%H:%M:%fZ','now','-{days} days') \
                     GROUP BY day ORDER BY day"
                );
                sqlx::query(&sql).fetch_all(p).await?
                    .into_iter().map(|r| -> Result<DailyPoint> {
                        Ok(DailyPoint { day: r.try_get("day")?, up: r.try_get("up")?, down: r.try_get("down")? })
                    }).collect::<Result<Vec<_>>>()?
            }
            Database::Postgres(p) => {
                let sql = "SELECT to_char(date_trunc('day', ts), 'YYYY-MM-DD') AS day, \
                                  COALESCE(SUM(up_delta),0)::bigint AS up, \
                                  COALESCE(SUM(down_delta),0)::bigint AS down \
                           FROM stats_samples \
                           WHERE ts >= NOW() - ($1 || ' days')::interval \
                           GROUP BY day ORDER BY day";
                sqlx::query(sql).bind(days.to_string()).fetch_all(p).await?
                    .into_iter().map(|r| -> Result<DailyPoint> {
                        Ok(DailyPoint { day: r.try_get("day")?, up: r.try_get("up")?, down: r.try_get("down")? })
                    }).collect::<Result<Vec<_>>>()?
            }
        };
        Ok(rows)
    }

    /// Most recent sample timestamp, for the dashboard "last collected" line.
    pub async fn last_collected(&self) -> Result<Option<DateTime<Utc>>> {
        let v = match &self.db {
            Database::Sqlite(p) => sqlx::query("SELECT MAX(ts) AS m FROM stats_samples")
                .fetch_one(p).await?.try_get::<Option<DateTime<Utc>>, _>("m")?,
            Database::Postgres(p) => sqlx::query("SELECT MAX(ts) AS m FROM stats_samples")
                .fetch_one(p).await?.try_get::<Option<DateTime<Utc>>, _>("m")?,
        };
        Ok(v)
    }
}
