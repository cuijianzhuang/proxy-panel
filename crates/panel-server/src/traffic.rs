//! `/api/traffic` — per-user totals joined with quota, daily series, and a
//! manual collect trigger.

use axum::extract::{Query, State};
use axum::Json;
use panel_domain::{DailyPoint, UserTotal};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::{CurrentUser, RequireAdmin};
use crate::error::ApiError;
use crate::state::AppState;

/// A user's traffic joined with their quota + enabled state, for the table.
#[derive(Serialize)]
pub struct UserTraffic {
    pub proxy_user_id: i64,
    pub name:          String,
    pub up:            i64,
    pub down:          i64,
    pub total:         i64,
    pub used_bytes:    i64,
    pub quota_gb:      f64,
    pub enabled:       bool,
}

pub async fn summary(
    _: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let totals: Vec<UserTotal> = state.stats.user_totals().await?;
    let users = state.proxy_users.list().await?;

    let rows: Vec<UserTraffic> = users
        .iter()
        .map(|u| {
            let t = totals.iter().find(|x| x.proxy_user_id == u.id);
            let up = t.map(|x| x.up).unwrap_or(0);
            let down = t.map(|x| x.down).unwrap_or(0);
            UserTraffic {
                proxy_user_id: u.id,
                name:          u.name.clone(),
                up,
                down,
                total:         up + down,
                used_bytes:    u.used_bytes,
                quota_gb:      u.quota_gb,
                enabled:       u.enabled,
            }
        })
        .collect();

    let last = state.stats.last_collected().await?;
    let grand_total: i64 = rows.iter().map(|r| r.total).sum();

    Ok(Json(json!({
        "users":          rows,
        "grand_total":    grand_total,
        "last_collected": last,
    })))
}

#[derive(Deserialize)]
pub struct SeriesQuery {
    #[serde(default = "default_days")]
    pub days: i64,
}
fn default_days() -> i64 { 14 }

pub async fn series(
    _: CurrentUser,
    State(state): State<AppState>,
    Query(q): Query<SeriesQuery>,
) -> Result<Json<Vec<DailyPoint>>, ApiError> {
    Ok(Json(state.stats.daily_series(q.days).await?))
}

/// Run a collection pass right now (admin). Handy for testing and for "refresh"
/// buttons in the UI.
pub async fn collect_now(
    _: RequireAdmin,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let s = state.collect_traffic().await;
    Ok(Json(json!({
        "nodes_polled":    s.nodes_polled,
        "samples_written": s.samples_written,
        "users_cut_off":   s.users_cut_off,
    })))
}
