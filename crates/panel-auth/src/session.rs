use chrono::{DateTime, Utc};
use panel_persistence::Database;
use rand::RngCore;
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::error::Result;

/// Opaque token presented in the session cookie.
///
/// Internally 32 random bytes. The cookie carries the hex-encoded value;
/// the database stores only its sha256 hash, so a DB leak doesn't yield
/// usable session cookies.
#[derive(Debug, Clone)]
pub struct SessionToken {
    bytes: [u8; 32],
}

impl SessionToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        Self { bytes }
    }

    /// Parse a hex-encoded cookie value back into a token. Returns `None` on
    /// any decoding failure — we treat malformed input as "no session".
    pub fn from_cookie(value: &str) -> Option<Self> {
        let bytes = hex::decode(value).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Some(Self { bytes: out })
    }

    /// Hex string suitable for the cookie value.
    pub fn cookie_value(&self) -> String {
        hex::encode(self.bytes)
    }

    /// Hex-encoded sha256 — what we store as `sessions.token_hash`.
    pub fn db_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.bytes);
        hex::encode(h.finalize())
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id:    i64,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct SessionRepo {
    db: Database,
}

impl SessionRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        token: &SessionToken,
        user_id: i64,
        expires_at: DateTime<Utc>,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<()> {
        let hash = token.db_hash();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO sessions (token_hash, user_id, expires_at, ip, user_agent) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&hash)
                .bind(user_id)
                .bind(expires_at)
                .bind(ip)
                .bind(user_agent)
                .execute(pool)
                .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO sessions (token_hash, user_id, expires_at, ip, user_agent) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(&hash)
                .bind(user_id)
                .bind(expires_at)
                .bind(ip)
                .bind(user_agent)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    /// Look up a non-expired session and bump `last_used_at`. Returns `None`
    /// if the token is unknown or expired.
    pub async fn touch(&self, token: &SessionToken) -> Result<Option<Session>> {
        let hash = token.db_hash();
        let now = Utc::now();
        let session = match &self.db {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT user_id, expires_at FROM sessions \
                     WHERE token_hash = ? AND expires_at > ?",
                )
                .bind(&hash)
                .bind(now)
                .fetch_optional(pool)
                .await?;

                if let Some(row) = row {
                    sqlx::query("UPDATE sessions SET last_used_at = ? WHERE token_hash = ?")
                        .bind(now)
                        .bind(&hash)
                        .execute(pool)
                        .await?;
                    Some(Session {
                        user_id:    row.try_get("user_id")?,
                        expires_at: row.try_get("expires_at")?,
                    })
                } else {
                    None
                }
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "UPDATE sessions SET last_used_at = $1 \
                     WHERE token_hash = $2 AND expires_at > $1 \
                     RETURNING user_id, expires_at",
                )
                .bind(now)
                .bind(&hash)
                .fetch_optional(pool)
                .await?;

                row.map(|row| -> Result<Session> {
                    Ok(Session {
                        user_id:    row.try_get("user_id")?,
                        expires_at: row.try_get("expires_at")?,
                    })
                })
                .transpose()?
            }
        };
        Ok(session)
    }

    pub async fn delete(&self, token: &SessionToken) -> Result<()> {
        let hash = token.db_hash();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
                    .bind(&hash)
                    .execute(pool)
                    .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query("DELETE FROM sessions WHERE token_hash = $1")
                    .bind(&hash)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }

    /// Drop every session belonging to `user_id` *except* the one whose token
    /// is `keep` (typically the caller's own cookie). Use after password change.
    pub async fn purge_for_user_except(
        &self,
        user_id: i64,
        keep: Option<&SessionToken>,
    ) -> Result<u64> {
        let keep_hash = keep.map(|t| t.db_hash()).unwrap_or_default();
        let affected = match &self.db {
            Database::Sqlite(pool) => sqlx::query(
                "DELETE FROM sessions WHERE user_id = ? AND token_hash != ?",
            )
            .bind(user_id)
            .bind(&keep_hash)
            .execute(pool)
            .await?
            .rows_affected(),
            Database::Postgres(pool) => sqlx::query(
                "DELETE FROM sessions WHERE user_id = $1 AND token_hash != $2",
            )
            .bind(user_id)
            .bind(&keep_hash)
            .execute(pool)
            .await?
            .rows_affected(),
        };
        Ok(affected)
    }

    /// Purge expired sessions. Safe to call periodically.
    pub async fn purge_expired(&self) -> Result<u64> {
        let now = Utc::now();
        let affected = match &self.db {
            Database::Sqlite(pool) => sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
                .bind(now)
                .execute(pool)
                .await?
                .rows_affected(),
            Database::Postgres(pool) => sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
                .bind(now)
                .execute(pool)
                .await?
                .rows_affected(),
        };
        Ok(affected)
    }
}
