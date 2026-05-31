//! Backup management.
//!
//! v1 covers SQLite via `VACUUM INTO` — that produces a clean, point-in-time
//! .db file that can be restored simply by replacing the live database.
//! PostgreSQL backups would shell out to `pg_dump`; that path is left as a
//! TODO and the endpoint surfaces a clear error.
//!
//! On-disk layout: `data/backups/vpspanel-<iso>.db`. The `backups` table is
//! the ledger the UI reads; files orphaned from the table get re-indexed on
//! the next `list()` call.

use std::path::PathBuf;

use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::Serialize;
use serde_json::json;
use sqlx::Row;

use crate::auth::RequireAdmin;
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct Backup {
    pub id:         i64,
    pub filename:   String,
    pub size_bytes: i64,
    pub kind:       String,
    pub created_at: DateTime<Utc>,
}

fn backups_dir() -> PathBuf {
    PathBuf::from("data/backups")
}

pub async fn list(
    _: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Vec<Backup>>, ApiError> {
    let rows = match &state.db {
        Database::Sqlite(pool) => sqlx::query(
            "SELECT id, filename, size_bytes, kind, created_at \
             FROM backups ORDER BY id DESC LIMIT 200",
        )
        .fetch_all(pool)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|r| -> sqlx::Result<Backup> {
            Ok(Backup {
                id:         r.try_get("id")?,
                filename:   r.try_get("filename")?,
                size_bytes: r.try_get("size_bytes")?,
                kind:       r.try_get("kind")?,
                created_at: r.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::internal)?,
        Database::Postgres(pool) => sqlx::query(
            "SELECT id, filename, size_bytes, kind, created_at \
             FROM backups ORDER BY id DESC LIMIT 200",
        )
        .fetch_all(pool)
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|r| -> sqlx::Result<Backup> {
            Ok(Backup {
                id:         r.try_get("id")?,
                filename:   r.try_get("filename")?,
                size_bytes: r.try_get("size_bytes")?,
                kind:       r.try_get("kind")?,
                created_at: r.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(ApiError::internal)?,
    };
    Ok(Json(rows))
}

pub async fn create(
    _: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<Backup>, ApiError> {
    let pool = match &state.db {
        Database::Sqlite(p) => p,
        Database::Postgres(_) => {
            return Err(ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                "PostgreSQL backup needs pg_dump — not wired yet",
            ));
        }
    };

    let dir = backups_dir();
    std::fs::create_dir_all(&dir).map_err(|e| ApiError::internal(e))?;

    let ts = Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let filename = format!("vpspanel-{ts}.db");
    let full = dir.join(&filename);

    // `VACUUM INTO` writes a clean copy of the DB at the target path. It's
    // single-statement and atomic — partial output never appears.
    let sql = format!("VACUUM INTO '{}'", full.to_string_lossy().replace('\'', "''"));
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(ApiError::internal)?;

    let size = std::fs::metadata(&full).map(|m| m.len() as i64).unwrap_or(0);

    let id = sqlx::query(
        "INSERT INTO backups (filename, size_bytes, kind) VALUES (?, ?, 'manual') RETURNING id",
    )
    .bind(&filename)
    .bind(size)
    .fetch_one(pool)
    .await
    .map_err(ApiError::internal)?
    .try_get::<i64, _>("id")
    .map_err(ApiError::internal)?;

    state
        .notify(
            "backup",
            "备份完成",
            format!("已创建备份 {filename}({size} 字节)。"),
        )
        .await;

    Ok(Json(Backup {
        id,
        filename,
        size_bytes: size,
        kind: "manual".into(),
        created_at: Utc::now(),
    }))
}

pub async fn delete(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pool = match &state.db {
        Database::Sqlite(p) => p,
        Database::Postgres(_) => {
            return Err(ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                "PostgreSQL backup not wired",
            ));
        }
    };

    let row = sqlx::query("SELECT filename FROM backups WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::internal)?;
    let row = row.ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "backup not found"))?;
    let filename: String = row.try_get("filename").map_err(ApiError::internal)?;

    // Best-effort delete the file, but always remove the row so a dangling
    // catalog entry doesn't linger.
    let _ = std::fs::remove_file(backups_dir().join(&filename));
    sqlx::query("DELETE FROM backups WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(json!({ "ok": true })))
}

/// Stream the `.db` file. Admin only — backups contain pw_hash, sessions, etc.
pub async fn download(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Response, ApiError> {
    let pool = match &state.db {
        Database::Sqlite(p) => p,
        Database::Postgres(_) => {
            return Err(ApiError::new(
                StatusCode::NOT_IMPLEMENTED,
                "PostgreSQL backup not wired",
            ))
        }
    };

    let row = sqlx::query("SELECT filename FROM backups WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "backup not found"))?;
    let filename: String = row.try_get("filename").map_err(ApiError::internal)?;

    let bytes = std::fs::read(backups_dir().join(&filename)).map_err(|e| {
        ApiError::new(StatusCode::NOT_FOUND, format!("backup file missing: {e}"))
    })?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                .unwrap_or(HeaderValue::from_static("attachment")),
        )
        .body(axum::body::Body::from(bytes))
        .unwrap_or_else(|_| (StatusCode::INTERNAL_SERVER_ERROR, "").into_response()))
}
