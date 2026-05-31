#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("account is disabled")]
    AccountDisabled,
    #[error("session not found or expired")]
    SessionInvalid,
    #[error("password hashing failed: {0}")]
    PasswordHash(String),
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
