//! `/api/chain-proxies` CRUD. Read for any authed user; writes admin-only.

use axum::extract::{Path, State};
use axum::Json;
use panel_domain::{ChainProxy, CreateChainProxy, UpdateChainProxy};
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn list(_: CurrentUser, State(s): State<AppState>) -> Result<Json<Vec<ChainProxy>>, ApiError> {
    Ok(Json(s.chain_proxies.list().await?))
}

pub async fn get_one(
    _: CurrentUser, State(s): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<ChainProxy>, ApiError> {
    Ok(Json(s.chain_proxies.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "not found")
    })?))
}

pub async fn create(
    _: RequireAdmin, State(s): State<AppState>, Json(input): Json<CreateChainProxy>,
) -> Result<(axum::http::StatusCode, Json<ChainProxy>), ApiError> {
    Ok((axum::http::StatusCode::CREATED, Json(s.chain_proxies.create(input).await?)))
}

pub async fn update(
    _: RequireAdmin, State(s): State<AppState>, Path(id): Path<i64>,
    Json(patch): Json<UpdateChainProxy>,
) -> Result<Json<ChainProxy>, ApiError> {
    Ok(Json(s.chain_proxies.update(id, patch).await?))
}

pub async fn delete(
    _: RequireAdmin, State(s): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !s.chain_proxies.delete(id).await? {
        return Err(ApiError::new(axum::http::StatusCode::NOT_FOUND, "not found"));
    }
    Ok(Json(json!({ "ok": true })))
}
