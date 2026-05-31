//! `/api/nodes/:id/apply` (enqueue) + `/api/tasks` (read).

use axum::extract::{Path, Query, State};
use axum::Json;
use panel_task::{NewTask, Task, TaskKind};
use serde::Deserialize;
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}
fn default_limit() -> i64 {
    50
}

pub async fn list(
    _: CurrentUser,
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Task>>, ApiError> {
    let limit = q.limit.clamp(1, 500);
    let rows = state
        .tasks
        .list(limit)
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok(Json(rows))
}

pub async fn get_one(
    _: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Task>, ApiError> {
    let row = state
        .tasks
        .find(id)
        .await
        .map_err(|e| ApiError::internal(e))?
        .ok_or_else(|| {
            ApiError::new(axum::http::StatusCode::NOT_FOUND, "task not found")
        })?;
    Ok(Json(row))
}

/// Enqueue an `apply_config` task for the node.
pub async fn apply_node(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(node_id): Path<i64>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    if state.nodes.find(node_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "node not found",
        ));
    }
    let task = state
        .tasks
        .enqueue(NewTask {
            node_id,
            kind:    TaskKind::ApplyConfig,
            payload: json!({}),
        })
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(json!({ "task_id": task.id, "status": task.status })),
    ))
}

/// Enqueue a `restart` task.
pub async fn restart_node(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(node_id): Path<i64>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    if state.nodes.find(node_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "node not found",
        ));
    }
    let task = state
        .tasks
        .enqueue(NewTask {
            node_id,
            kind:    TaskKind::Restart,
            payload: json!({}),
        })
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(json!({ "task_id": task.id, "status": task.status })),
    ))
}

/// Enqueue a `provision` task — install the core binary + systemd service on
/// a fresh VPS, then write the current config and start the service.
/// Safe to re-run: skips installation if the binary is already present.
pub async fn provision_node(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(node_id): Path<i64>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    if state.nodes.find(node_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "node not found",
        ));
    }
    let task = state
        .tasks
        .enqueue(NewTask {
            node_id,
            kind:    TaskKind::Provision,
            payload: json!({}),
        })
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(json!({ "task_id": task.id, "status": task.status })),
    ))
}

/// Enqueue a `check_health` task.
pub async fn health_check_node(
    _: RequireAdmin,
    State(state): State<AppState>,
    Path(node_id): Path<i64>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), ApiError> {
    if state.nodes.find(node_id).await?.is_none() {
        return Err(ApiError::new(
            axum::http::StatusCode::NOT_FOUND,
            "node not found",
        ));
    }
    let task = state
        .tasks
        .enqueue(NewTask {
            node_id,
            kind:    TaskKind::CheckHealth,
            payload: json!({}),
        })
        .await
        .map_err(|e| ApiError::internal(e))?;
    Ok((
        axum::http::StatusCode::ACCEPTED,
        Json(json!({ "task_id": task.id, "status": task.status })),
    ))
}
