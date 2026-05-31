use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Uniform HTTP error type. `IntoResponse` writes a `{"error":"..."}` body.
#[derive(Debug)]
pub struct ApiError {
    pub status:  StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, message)
    }

    pub fn internal(err: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({ "error": self.message }));
        (self.status, body).into_response()
    }
}

impl From<panel_auth::Error> for ApiError {
    fn from(err: panel_auth::Error) -> Self {
        use panel_auth::Error::*;
        match err {
            InvalidCredentials => Self::unauthorized("invalid credentials"),
            AccountDisabled => Self::unauthorized("account disabled"),
            SessionInvalid => Self::unauthorized("session invalid"),
            other => Self::internal(other),
        }
    }
}

impl From<panel_persistence::Error> for ApiError {
    fn from(err: panel_persistence::Error) -> Self {
        Self::internal(err)
    }
}

impl From<panel_domain::Error> for ApiError {
    fn from(err: panel_domain::Error) -> Self {
        use panel_domain::Error::*;
        match err {
            Validation(msg) => Self::new(StatusCode::BAD_REQUEST, msg),
            NotFound => Self::new(StatusCode::NOT_FOUND, "not found"),
            other => Self::internal(other),
        }
    }
}

impl From<panel_core::RenderError> for ApiError {
    fn from(err: panel_core::RenderError) -> Self {
        // Render errors are caused by config the caller (or the listener row)
        // can fix — surface as 4xx instead of 5xx.
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::internal(err)
    }
}
