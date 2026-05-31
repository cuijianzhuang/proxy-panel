//! `/api/listeners` CRUD.
//!
//! Reads are gated by `CurrentUser` (any authenticated user).
//! Writes require `RequireAdmin` (is_admin == true).

use axum::extract::{Path, State};
use axum::Json;
use panel_core::InboundContext;
use panel_domain::{CreateListener, Listener, UpdateListener};
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn list(
    _: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<Listener>>, ApiError> {
    let rows = state.listeners.list().await?;
    Ok(Json(rows))
}

pub async fn get_one(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Listener>, ApiError> {
    let row = state.listeners.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "listener not found")
    })?;
    Ok(Json(row))
}

pub async fn create(
    _: RequireAdmin,
    State(state): State<AppState>,
    Json(input): Json<CreateListener>,
) -> Result<(axum::http::StatusCode, Json<Listener>), ApiError> {
    let row = state.listeners.create(input).await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdateListener>,
) -> Result<Json<Listener>, ApiError> {
    let row = state.listeners.update(id, patch).await?;
    Ok(Json(row))
}

pub async fn delete(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let removed = state.listeners.delete(id).await?;
    if !removed {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "listener not found",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Render this listener into the JSON shape its declared core (`xray` or
/// `singbox`) expects in its `inbounds[]` array. Useful for preview /
/// "what would this look like" debugging from the UI. Read-only.
pub async fn preview(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let listener = state.listeners.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "listener not found")
    })?;
    let clients = state.proxy_users.list_for_listener(listener.id).await?;
    let ctx = InboundContext {
        listener: &listener,
        clients:  &clients,
        // Per-listener preview only renders the inbound shape; chain proxy
        // only matters in a whole-node render where outbound + routing live.
        chain:    None,
    };
    let adapter = state.adapters.for_core(listener.core);
    let rendered = adapter.render_inbound(&ctx)?;
    Ok(Json(json!({
        "core":       listener.core,
        "client_cnt": clients.len(),
        "inbound":    rendered,
    })))
}
