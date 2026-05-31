#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("auth failed: {0}")]
    Auth(String),
    #[error("io: {0}")]
    Io(String),
    #[error("command failed (exit {exit}): {stderr}")]
    Command { exit: i32, stderr: String },
}

pub type Result<T> = std::result::Result<T, Error>;
