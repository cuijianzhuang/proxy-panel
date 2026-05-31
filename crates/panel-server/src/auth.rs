//! Login / logout / me endpoints and a `CurrentUser` extractor that resolves
//! the session cookie attached to incoming requests.

use std::net::SocketAddr;

use async_trait::async_trait;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::header::{HeaderMap, HeaderValue, COOKIE, SET_COOKIE, USER_AGENT};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use panel_auth::{PanelUser, SessionToken, COOKIE_NAME, SESSION_TTL};
use serde::Deserialize;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let user = state.users.authenticate(&req.username, &req.password).await?;

    let token = SessionToken::generate();
    let expires_at = Utc::now() + SESSION_TTL;
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let ip = peer.ip().to_string();

    state
        .sessions
        .create(&token, user.id, expires_at, Some(&ip), ua.as_deref())
        .await?;

    let mut response = Json(serde_json::to_value(&user).unwrap_or_default()).into_response();
    let cookie = build_cookie(&token.cookie_value(), state.cookie_secure, SESSION_TTL.num_seconds());
    response
        .headers_mut()
        .append(SET_COOKIE, HeaderValue::from_str(&cookie).expect("valid cookie"));
    Ok(response)
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = read_session_token(&headers) {
        state.sessions.delete(&token).await?;
    }

    let body = Json(serde_json::json!({ "ok": true })).into_response();
    let mut response = body;
    let cookie = build_cookie("", state.cookie_secure, 0);
    response
        .headers_mut()
        .append(SET_COOKIE, HeaderValue::from_str(&cookie).expect("valid cookie"));
    Ok(response)
}

pub async fn me(CurrentUser(user): CurrentUser) -> Json<PanelUser> {
    Json(user)
}

#[derive(Deserialize)]
pub struct ChangePasswordRequest {
    pub old_password: String,
    pub new_password: String,
}

/// Self-service password change. Verifies the old password before saving;
/// other sessions for this user are dropped so a stolen cookie elsewhere
/// stops working. The current cookie keeps living.
pub async fn change_password(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    state
        .users
        .change_password(user.id, &req.old_password, &req.new_password)
        .await?;

    let keep = read_session_token(&headers);
    let _ = state
        .sessions
        .purge_for_user_except(user.id, keep.as_ref())
        .await;

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// Axum extractor: pulls the session cookie, validates it, loads the user.
/// 401 if no cookie / bad cookie / expired session / user vanished.
pub struct CurrentUser(pub PanelUser);

/// Stronger extractor: same as `CurrentUser` plus `is_admin == true`, else 403.
///
/// Inner user is kept for future audit logging even if handlers ignore it today.
pub struct RequireAdmin(#[allow(dead_code)] pub PanelUser);

#[async_trait]
impl FromRequestParts<AppState> for RequireAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        if !user.is_admin {
            return Err(ApiError::new(StatusCode::FORBIDDEN, "admin required"));
        }
        Ok(RequireAdmin(user))
    }
}

#[async_trait]
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = read_session_token(&parts.headers)
            .ok_or_else(|| ApiError::unauthorized("not authenticated"))?;

        let session = state
            .sessions
            .touch(&token)
            .await?
            .ok_or_else(|| ApiError::unauthorized("session expired"))?;

        let user = state
            .users
            .find_by_id(session.user_id)
            .await?
            .ok_or_else(|| ApiError::unauthorized("user gone"))?;

        if !user.active {
            return Err(ApiError::unauthorized("account disabled"));
        }

        Ok(CurrentUser(user))
    }
}

/// Read our cookie out of the `Cookie:` header. Returns the parsed token, or
/// `None` if no cookie is present or it's malformed (treated as "no session").
fn read_session_token(headers: &HeaderMap) -> Option<SessionToken> {
    let raw = headers.get(COOKIE)?.to_str().ok()?;
    for pair in raw.split(';') {
        let (name, value) = pair.split_once('=')?;
        if name.trim() == COOKIE_NAME {
            return SessionToken::from_cookie(value.trim());
        }
    }
    None
}

/// Build a `Set-Cookie` value with safe defaults. `max_age == 0` clears it.
fn build_cookie(value: &str, secure: bool, max_age_secs: i64) -> String {
    let mut parts = vec![format!("{}={}", COOKIE_NAME, value)];
    parts.push("Path=/".to_string());
    parts.push("HttpOnly".to_string());
    parts.push("SameSite=Strict".to_string());
    parts.push(format!("Max-Age={}", max_age_secs.max(0)));
    if secure {
        parts.push("Secure".to_string());
    }
    parts.join("; ")
}

#[allow(dead_code)]
const _STATUS_REFERENCE: StatusCode = StatusCode::OK; // keep StatusCode in scope for future handlers
