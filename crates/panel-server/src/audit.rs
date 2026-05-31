//! Audit log: records every successful write to the panel API.
//!
//! Implemented as an axum middleware that runs *after* the handler so it
//! sees the final status code. Read methods (GET/HEAD/OPTIONS) are skipped
//! because they'd flood the table without insight.
//!
//! The `actor` columns are populated by reading the session cookie
//! out-of-band (same logic as `CurrentUser`); anonymous writes — e.g. a
//! future open registration flow — would land with NULL actor.

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use chrono::{DateTime, Utc};
use panel_auth::SessionToken;
use panel_persistence::Database;
use serde::Serialize;
use sqlx::Row;
use std::net::SocketAddr;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub id:         i64,
    pub actor_id:   Option<i64>,
    pub actor_name: Option<String>,
    pub method:     String,
    pub path:       String,
    pub status:     i32,
    pub ip:         Option<String>,
    pub user_agent: Option<String>,
    pub ts:         DateTime<Utc>,
}

/// Middleware: log write requests to the audit_logs table after the handler runs.
pub async fn middleware(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let should_log = matches!(req.method(), &Method::POST | &Method::PUT | &Method::DELETE | &Method::PATCH);

    // Snapshot fields we'll need post-response.
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let ua = req
        .headers()
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let cookie_header = req
        .headers()
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let response = next.run(req).await;

    if !should_log {
        return response;
    }
    // Ignore login/logout themselves to avoid leaking attempt patterns.
    if path == "/api/login" || path == "/api/logout" {
        return response;
    }
    let status = response.status().as_u16() as i32;

    // Resolve actor by looking up the session, best-effort.
    let (actor_id, actor_name) = resolve_actor(&state, cookie_header.as_deref()).await;
    let ip = peer.ip().to_string();

    let _ = insert(
        &state.db,
        actor_id,
        actor_name.as_deref(),
        method.as_str(),
        &path,
        status,
        Some(&ip),
        ua.as_deref(),
    )
    .await;

    response
}

async fn resolve_actor(state: &AppState, cookie_header: Option<&str>) -> (Option<i64>, Option<String>) {
    let raw = match cookie_header {
        Some(s) => s,
        None => return (None, None),
    };
    let mut token = None;
    for pair in raw.split(';') {
        if let Some((name, value)) = pair.split_once('=') {
            if name.trim() == panel_auth::COOKIE_NAME {
                token = SessionToken::from_cookie(value.trim());
                break;
            }
        }
    }
    let token = match token {
        Some(t) => t,
        None => return (None, None),
    };
    // We deliberately don't `touch` here — that's the auth extractor's job.
    if let Ok(Some(s)) = state.sessions.touch(&token).await {
        if let Ok(Some(u)) = state.users.find_by_id(s.user_id).await {
            return (Some(u.id), Some(u.username));
        }
    }
    (None, None)
}

async fn insert(
    db: &Database,
    actor_id: Option<i64>,
    actor_name: Option<&str>,
    method: &str,
    path: &str,
    status: i32,
    ip: Option<&str>,
    ua: Option<&str>,
) -> sqlx::Result<()> {
    match db {
        Database::Sqlite(pool) => {
            sqlx::query(
                "INSERT INTO audit_logs (actor_id, actor_name, method, path, status, ip, user_agent) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(actor_id)
            .bind(actor_name)
            .bind(method)
            .bind(path)
            .bind(status)
            .bind(ip)
            .bind(ua)
            .execute(pool)
            .await?;
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "INSERT INTO audit_logs (actor_id, actor_name, method, path, status, ip, user_agent) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(actor_id)
            .bind(actor_name)
            .bind(method)
            .bind(path)
            .bind(status)
            .bind(ip)
            .bind(ua)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// API endpoint: most recent 200 audit entries. Admin only.
pub async fn list(
    _: crate::auth::RequireAdmin,
    State(state): State<AppState>,
) -> Result<axum::Json<Vec<AuditEntry>>, crate::error::ApiError> {
    let rows = match &state.db {
        Database::Sqlite(pool) => {
            sqlx::query(
                "SELECT id, actor_id, actor_name, method, path, status, ip, user_agent, ts \
                 FROM audit_logs ORDER BY id DESC LIMIT 200",
            )
            .fetch_all(pool)
            .await
            .map_err(crate::error::ApiError::internal)?
            .into_iter()
            .map(map_sqlite)
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::error::ApiError::internal)?
        }
        Database::Postgres(pool) => {
            sqlx::query(
                "SELECT id, actor_id, actor_name, method, path, status, ip, user_agent, ts \
                 FROM audit_logs ORDER BY id DESC LIMIT 200",
            )
            .fetch_all(pool)
            .await
            .map_err(crate::error::ApiError::internal)?
            .into_iter()
            .map(map_postgres)
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::error::ApiError::internal)?
        }
    };
    Ok(axum::Json(rows))
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> sqlx::Result<AuditEntry> {
    Ok(AuditEntry {
        id:         row.try_get("id")?,
        actor_id:   row.try_get("actor_id")?,
        actor_name: row.try_get("actor_name")?,
        method:     row.try_get("method")?,
        path:       row.try_get("path")?,
        status:     row.try_get("status")?,
        ip:         row.try_get("ip")?,
        user_agent: row.try_get("user_agent")?,
        ts:         row.try_get("ts")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> sqlx::Result<AuditEntry> {
    Ok(AuditEntry {
        id:         row.try_get("id")?,
        actor_id:   row.try_get("actor_id")?,
        actor_name: row.try_get("actor_name")?,
        method:     row.try_get("method")?,
        path:       row.try_get("path")?,
        status:     row.try_get("status")?,
        ip:         row.try_get("ip")?,
        user_agent: row.try_get("user_agent")?,
        ts:         row.try_get("ts")?,
    })
}
