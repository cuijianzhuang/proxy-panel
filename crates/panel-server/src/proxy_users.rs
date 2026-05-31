//! `/api/proxy-users` CRUD + `/api/listeners/:id/clients` attach/detach.
//!
//! All writes are admin-only. Reads are gated by `CurrentUser`.

use axum::extract::{Path, State};
use axum::Json;
use panel_domain::{CreateProxyUser, ProxyUser, UpdateProxyUser};
use chrono::Utc;
use serde::Deserialize;
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// Proxy users
// ---------------------------------------------------------------------------

pub async fn list(
    _: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<ProxyUser>>, ApiError> {
    Ok(Json(state.proxy_users.list().await?))
}

pub async fn get_one(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProxyUser>, ApiError> {
    let row = state.proxy_users.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "proxy user not found")
    })?;
    Ok(Json(row))
}

pub async fn create(
    _: RequireAdmin,
    State(state): State<AppState>,
    Json(input): Json<CreateProxyUser>,
) -> Result<(axum::http::StatusCode, Json<ProxyUser>), ApiError> {
    let row = state.proxy_users.create(input).await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateProxyUser>,
) -> Result<Json<ProxyUser>, ApiError> {
    Ok(Json(state.proxy_users.update(id, patch).await?))
}

pub async fn delete(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.proxy_users.delete(id).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "proxy user not found",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Listener attachments
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AttachRequest {
    pub proxy_user_id: i64,
}

/// List users attached to a listener.
pub async fn list_clients(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(listener_id): Path<i64>,
) -> Result<Json<Vec<ProxyUser>>, ApiError> {
    // Make sure the listener exists so we return 404 vs an empty list silently.
    if state.listeners.find(listener_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "listener not found",
        ));
    }
    Ok(Json(state.proxy_users.list_for_listener(listener_id).await?))
}

/// Attach a proxy user to a listener. Idempotent.
pub async fn attach(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(listener_id): Path<i64>,
    Json(req): Json<AttachRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate both sides exist for a clean 404 rather than an FK error.
    if state.listeners.find(listener_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "listener not found",
        ));
    }
    if state.proxy_users.find(req.proxy_user_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "proxy user not found",
        ));
    }
    state
        .listeners
        .attach_client(listener_id, req.proxy_user_id)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn detach(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path((listener_id, user_id)): Path<(i64, i64)>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = state.listeners.detach_client(listener_id, user_id).await?;
    if !removed {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "attachment not found",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Extra user actions
// ---------------------------------------------------------------------------

/// `POST /api/proxy-users/:id/reset-traffic`
/// Zero the user's `used_bytes` counter.
pub async fn reset_traffic(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProxyUser>, ApiError> {
    let user = state.proxy_users.reset_traffic(id).await
        .map_err(|e| ApiError::internal(e))?;
    Ok(Json(user))
}

/// `POST /api/proxy-users/:id/kick`
/// Disable + rotate subscription token → existing clients reconnect on next
/// poll and find they can no longer authenticate. Re-enable manually.
pub async fn kick(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProxyUser>, ApiError> {
    if state.proxy_users.find(id).await?.is_none() {
        return Err(ApiError::new(axum::http::StatusCode::NOT_FOUND, "user not found"));
    }
    let user = state.proxy_users.update(id, UpdateProxyUser {
        enabled:                   Some(false),
        rotate_subscription_token: Some(true),
        ..Default::default()
    }).await?;
    Ok(Json(user))
}

/// `POST /api/proxy-users/:id/enable`
/// Re-enable a previously disabled / kicked user.
pub async fn enable(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<ProxyUser>, ApiError> {
    if state.proxy_users.find(id).await?.is_none() {
        return Err(ApiError::new(axum::http::StatusCode::NOT_FOUND, "user not found"));
    }
    let user = state.proxy_users.update(id, UpdateProxyUser {
        enabled: Some(true),
        ..Default::default()
    }).await?;
    Ok(Json(user))
}

// suppress the unused chrono import warning if used indirectly
const _: fn() = || { let _ = Utc::now(); };
