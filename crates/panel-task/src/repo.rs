use chrono::Utc;
use panel_persistence::Database;
use sqlx::types::Json;
use sqlx::Row;

use crate::model::{NewTask, Task, TaskKind, TaskStatus};

const COLS: &str = "id, node_id, kind, status, payload, log, error, \
                    started_at, finished_at, created_at, updated_at";

#[derive(Clone)]
pub struct TaskRepo {
    db: Database,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskRepoError {
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("invalid stored value: {0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, TaskRepoError>;

impl TaskRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn enqueue(&self, t: NewTask) -> Result<Task> {
        let id = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "INSERT INTO node_operation_tasks (node_id, kind, payload) \
                 VALUES (?, ?, ?) RETURNING id",
            )
            .bind(t.node_id)
            .bind(t.kind.as_str())
            .bind(Json(&t.payload))
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
            Database::Postgres(pool) => sqlx::query(
                "INSERT INTO node_operation_tasks (node_id, kind, payload) \
                 VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(t.node_id)
            .bind(t.kind.as_str())
            .bind(Json(&t.payload))
            .fetch_one(pool)
            .await?
            .try_get::<i64, _>("id")?,
        };
        self.find(id).await?.ok_or(TaskRepoError::Decode("missing after insert".into()))
    }

    pub async fn list(&self, limit: i64) -> Result<Vec<Task>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!(
                    "SELECT {COLS} FROM node_operation_tasks ORDER BY id DESC LIMIT ?"
                );
                sqlx::query(&sql)
                    .bind(limit)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_sqlite)
                    .collect()
            }
            Database::Postgres(pool) => {
                let sql = format!(
                    "SELECT {COLS} FROM node_operation_tasks ORDER BY id DESC LIMIT $1"
                );
                sqlx::query(&sql)
                    .bind(limit)
                    .fetch_all(pool)
                    .await?
                    .into_iter()
                    .map(map_postgres)
                    .collect()
            }
        }
    }

    pub async fn find(&self, id: i64) -> Result<Option<Task>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {COLS} FROM node_operation_tasks WHERE id = ?");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_sqlite)
                    .transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {COLS} FROM node_operation_tasks WHERE id = $1");
                sqlx::query(&sql)
                    .bind(id)
                    .fetch_optional(pool)
                    .await?
                    .map(map_postgres)
                    .transpose()
            }
        }
    }

    /// Atomically claim the oldest pending task. Returns `None` if the queue
    /// is empty. Uses a status check + update so two workers can't both grab
    /// the same row (we still rely on the SQL engine for serialisation).
    pub async fn claim_next(&self) -> Result<Option<Task>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let mut tx = pool.begin().await?;
                let row = sqlx::query(
                    "SELECT id FROM node_operation_tasks WHERE status = 'pending' \
                     ORDER BY id LIMIT 1",
                )
                .fetch_optional(&mut *tx)
                .await?;
                let Some(row) = row else {
                    return Ok(None);
                };
                let id: i64 = row.try_get("id")?;
                let now = Utc::now();
                sqlx::query(
                    "UPDATE node_operation_tasks SET status = 'running', started_at = ?, \
                       updated_at = ? WHERE id = ? AND status = 'pending'",
                )
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
                self.find(id).await
            }
            Database::Postgres(pool) => {
                // FOR UPDATE SKIP LOCKED is the right hammer here in PG;
                // single-statement UPDATE ... RETURNING also works because we
                // gate by status = 'pending'.
                let row = sqlx::query(
                    "UPDATE node_operation_tasks \
                     SET status = 'running', started_at = NOW(), updated_at = NOW() \
                     WHERE id = ( \
                       SELECT id FROM node_operation_tasks \
                         WHERE status = 'pending' \
                         ORDER BY id \
                         FOR UPDATE SKIP LOCKED LIMIT 1 \
                     ) RETURNING id",
                )
                .fetch_optional(pool)
                .await?;
                if let Some(r) = row {
                    let id: i64 = r.try_get("id")?;
                    self.find(id).await
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub async fn append_log(&self, id: i64, line: &str) -> Result<()> {
        // SQLite's ||-concat plus parameter binding does the trick on both backends.
        let now = Utc::now();
        let suffix = format!("{line}\n");
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE node_operation_tasks SET log = log || ?, updated_at = ? WHERE id = ?",
                )
                .bind(&suffix)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE node_operation_tasks SET log = log || $1, updated_at = $2 WHERE id = $3",
                )
                .bind(&suffix)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    pub async fn finish(
        &self,
        id: i64,
        status: TaskStatus,
        error: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "UPDATE node_operation_tasks SET status = ?, error = ?, \
                       finished_at = ?, updated_at = ? WHERE id = ?",
                )
                .bind(status.as_str())
                .bind(error)
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE node_operation_tasks SET status = $1, error = $2, \
                       finished_at = $3, updated_at = $4 WHERE id = $5",
                )
                .bind(status.as_str())
                .bind(error)
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Move any tasks stuck in `running` back to `pending`. Called once at
    /// worker startup to recover from a panel crash.
    pub async fn recover_orphans(&self) -> Result<u64> {
        let n = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "UPDATE node_operation_tasks SET status = 'pending', started_at = NULL \
                 WHERE status = 'running'",
            )
            .execute(pool)
            .await?
            .rows_affected(),
            Database::Postgres(pool) => sqlx::query(
                "UPDATE node_operation_tasks SET status = 'pending', started_at = NULL \
                 WHERE status = 'running'",
            )
            .execute(pool)
            .await?
            .rows_affected(),
        };
        Ok(n)
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<Task> {
    let kind: String = row.try_get("kind")?;
    let status: String = row.try_get("status")?;
    let payload: Json<serde_json::Value> = row.try_get("payload")?;
    Ok(Task {
        id:          row.try_get("id")?,
        node_id:     row.try_get("node_id")?,
        kind:        TaskKind::parse(&kind)
            .ok_or_else(|| TaskRepoError::Decode(format!("kind={kind}")))?,
        status:      TaskStatus::parse(&status)
            .ok_or_else(|| TaskRepoError::Decode(format!("status={status}")))?,
        payload:     payload.0,
        log:         row.try_get("log")?,
        error:       row.try_get("error")?,
        started_at:  row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        created_at:  row.try_get("created_at")?,
        updated_at:  row.try_get("updated_at")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> Result<Task> {
    let kind: String = row.try_get("kind")?;
    let status: String = row.try_get("status")?;
    let payload: Json<serde_json::Value> = row.try_get("payload")?;
    Ok(Task {
        id:          row.try_get("id")?,
        node_id:     row.try_get("node_id")?,
        kind:        TaskKind::parse(&kind)
            .ok_or_else(|| TaskRepoError::Decode(format!("kind={kind}")))?,
        status:      TaskStatus::parse(&status)
            .ok_or_else(|| TaskRepoError::Decode(format!("status={status}")))?,
        payload:     payload.0,
        log:         row.try_get("log")?,
        error:       row.try_get("error")?,
        started_at:  row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        created_at:  row.try_get("created_at")?,
        updated_at:  row.try_get("updated_at")?,
    })
}
