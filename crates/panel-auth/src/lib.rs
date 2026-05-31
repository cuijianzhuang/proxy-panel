//! Authentication: password hashing (argon2id), opaque session tokens,
//! and dialect-aware repositories for `panel_users` and `sessions`.

mod error;
mod password;
mod session;
mod user;

pub use error::{Error, Result};
pub use password::{hash_password, verify_password};
pub use session::{Session, SessionRepo, SessionToken};
pub use user::{PanelUser, PanelUserRepo};

/// How long a freshly-issued session stays valid.
pub const SESSION_TTL: chrono::Duration = chrono::Duration::days(7);

/// Cookie name for the browser session token.
pub const COOKIE_NAME: &str = "vpspanel_session";
