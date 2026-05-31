//! `/api/plans` CRUD. Read for any authenticated user; write for admin only.

use axum::extract::{Path, State};
use axum::Json;
use panel_domain::{CreatePlan, Plan, UpdatePlan};
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

pub async fn list(_: CurrentUser, State(state): State<AppState>) -> Result<Json<Vec<Plan>>, ApiError> {
    Ok(Json(state.plans.list().await?))
}

pub async fn get_one(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Plan>, ApiError> {
    let row = state
        .plans
        .find(id)
        .await?
        .ok_or_else(|| ApiError::new(axum::http::StatusCode::NOT_FOUND, "plan not found"))?;
    Ok(Json(row))
}

pub async fn create(
    _: RequireAdmin,
    State(state): State<AppState>,
    Json(input): Json<CreatePlan>,
) -> Result<(axum::http::StatusCode, Json<Plan>), ApiError> {
    let row = state.plans.create(input).await?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn update(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(patch): Json<UpdatePlan>,
) -> Result<Json<Plan>, ApiError> {
    Ok(Json(state.plans.update(id, patch).await?))
}

pub async fn delete(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.plans.delete(id).await? {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "plan not found",
        ));
    }
    Ok(Json(json!({ "ok": true })))
}
