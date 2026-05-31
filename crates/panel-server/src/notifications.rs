//! `/api/notifications` (channels) + `/api/notification-rules` + a test-send.
//!
//! Reads are admin-only here too — channel configs carry secrets (bot tokens,
//! SMTP passwords), so we don't expose them to viewers.

use axum::extract::{Path, State};
use axum::Json;
use panel_domain::{
    CreateChannel, NotificationChannel, NotificationRule, UpdateChannel, UpsertRule,
};
use panel_notify::Notification;
use serde_json::json;

use crate::auth::RequireAdmin;
use crate::error::ApiError;
use crate::state::AppState;

// ---- channels --------------------------------------------------------------

pub async fn list_channels(
    _: RequireAdmin, State(s): State<AppState>,
) -> Result<Json<Vec<NotificationChannel>>, ApiError> {
    Ok(Json(s.channels.list().await?))
}

pub async fn create_channel(
    _: RequireAdmin, State(s): State<AppState>, Json(input): Json<CreateChannel>,
) -> Result<(axum::http::StatusCode, Json<NotificationChannel>), ApiError> {
    Ok((axum::http::StatusCode::CREATED, Json(s.channels.create(input).await?)))
}

pub async fn update_channel(
    _: RequireAdmin, State(s): State<AppState>, Path(id): Path<i64>,
    Json(patch): Json<UpdateChannel>,
) -> Result<Json<NotificationChannel>, ApiError> {
    Ok(Json(s.channels.update(id, patch).await?))
}

pub async fn delete_channel(
    _: RequireAdmin, State(s): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !s.channels.delete(id).await? {
        return Err(ApiError::new(axum::http::StatusCode::NOT_FOUND, "not found"));
    }
    Ok(Json(json!({ "ok": true })))
}

/// Fire a sample message at one channel and report the outcome.
pub async fn test_channel(
    _: RequireAdmin, State(s): State<AppState>, Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let ch = s.channels.find(id).await?.ok_or_else(|| {
        ApiError::new(axum::http::StatusCode::NOT_FOUND, "channel not found")
    })?;
    let msg = Notification {
        event_type: "test".into(),
        title:      "proxy-panel 测试通知".into(),
        body:       format!("这是来自 proxy-panel 的测试消息,通道:{}。", ch.name),
    };
    let outcome = panel_notify::send_one(&ch, &msg).await;
    match outcome {
        Ok(()) => Ok(Json(json!({ "ok": true, "detail": "sent" }))),
        Err(e) => Ok(Json(json!({ "ok": false, "detail": e.to_string() }))),
    }
}

// ---- rules -----------------------------------------------------------------

pub async fn list_rules(
    _: RequireAdmin, State(s): State<AppState>,
) -> Result<Json<Vec<NotificationRule>>, ApiError> {
    Ok(Json(s.notification_rules.list().await?))
}

pub async fn upsert_rule(
    _: RequireAdmin, State(s): State<AppState>, Path(event_type): Path<String>,
    Json(input): Json<UpsertRule>,
) -> Result<Json<NotificationRule>, ApiError> {
    Ok(Json(s.notification_rules.upsert(&event_type, input).await?))
}
