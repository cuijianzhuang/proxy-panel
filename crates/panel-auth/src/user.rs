use chrono::{DateTime, Utc};
use panel_persistence::Database;
use serde::Serialize;
use sqlx::Row;

use crate::error::{Error, Result};
use crate::password::{hash_password, verify_password};

#[derive(Debug, Clone, Serialize)]
pub struct PanelUser {
    pub id:            i64,
    pub username:      String,
    pub role:          String,
    pub is_admin:      bool,
    pub active:        bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at:    DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

const SELECT_COLUMNS: &str =
    "id, username, role, is_admin, active, last_login_at, created_at, updated_at";

#[derive(Clone)]
pub struct PanelUserRepo {
    db: Database,
}

impl PanelUserRepo {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn count(&self) -> Result<i64> {
        let n = match &self.db {
            Database::Sqlite(pool) => sqlx::query("SELECT COUNT(*) AS n FROM panel_users")
                .fetch_one(pool)
                .await?
                .try_get::<i64, _>("n")?,
            Database::Postgres(pool) => sqlx::query("SELECT COUNT(*) AS n FROM panel_users")
                .fetch_one(pool)
                .await?
                .try_get::<i64, _>("n")?,
        };
        Ok(n)
    }

    pub async fn find_by_username(&self, username: &str) -> Result<Option<PanelUser>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!(
                    "SELECT {SELECT_COLUMNS} FROM panel_users WHERE username = ?"
                );
                let opt = sqlx::query(&sql)
                    .bind(username)
                    .fetch_optional(pool)
                    .await?;
                opt.map(map_sqlite).transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!(
                    "SELECT {SELECT_COLUMNS} FROM panel_users WHERE username = $1"
                );
                let opt = sqlx::query(&sql)
                    .bind(username)
                    .fetch_optional(pool)
                    .await?;
                opt.map(map_postgres).transpose()
            }
        }
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<PanelUser>> {
        match &self.db {
            Database::Sqlite(pool) => {
                let sql = format!("SELECT {SELECT_COLUMNS} FROM panel_users WHERE id = ?");
                let opt = sqlx::query(&sql).bind(id).fetch_optional(pool).await?;
                opt.map(map_sqlite).transpose()
            }
            Database::Postgres(pool) => {
                let sql = format!("SELECT {SELECT_COLUMNS} FROM panel_users WHERE id = $1");
                let opt = sqlx::query(&sql).bind(id).fetch_optional(pool).await?;
                opt.map(map_postgres).transpose()
            }
        }
    }

    /// Insert a new user with a freshly hashed password. Returns the new id.
    pub async fn create(
        &self,
        username: &str,
        plaintext_password: &str,
        role: &str,
        is_admin: bool,
    ) -> Result<i64> {
        let pw_hash = hash_password(plaintext_password)?;
        let id = match &self.db {
            Database::Sqlite(pool) => {
                let row = sqlx::query(
                    "INSERT INTO panel_users (username, pw_hash, role, is_admin, active) \
                     VALUES (?, ?, ?, ?, 1) RETURNING id",
                )
                .bind(username)
                .bind(&pw_hash)
                .bind(role)
                .bind(is_admin)
                .fetch_one(pool)
                .await?;
                row.try_get::<i64, _>("id")?
            }
            Database::Postgres(pool) => {
                let row = sqlx::query(
                    "INSERT INTO panel_users (username, pw_hash, role, is_admin, active) \
                     VALUES ($1, $2, $3, $4, TRUE) RETURNING id",
                )
                .bind(username)
                .bind(&pw_hash)
                .bind(role)
                .bind(is_admin)
                .fetch_one(pool)
                .await?;
                row.try_get::<i64, _>("id")?
            }
        };
        Ok(id)
    }

    /// Verify (username, password). Returns the user on success and bumps
    /// `last_login_at`. Inactive accounts return `AccountDisabled`.
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<PanelUser> {
        let user = self
            .find_by_username(username)
            .await?
            .ok_or(Error::InvalidCredentials)?;

        if !user.active {
            return Err(Error::AccountDisabled);
        }

        let stored = self.fetch_pw_hash(user.id).await?;
        if !verify_password(password, &stored)? {
            return Err(Error::InvalidCredentials);
        }

        self.touch_last_login(user.id).await?;
        self.find_by_id(user.id)
            .await?
            .ok_or(Error::InvalidCredentials)
    }

    /// Change a user's password. Verifies the old one first; on success
    /// hashes + writes the new one and bumps `updated_at`.
    pub async fn change_password(
        &self,
        id: i64,
        old_password: &str,
        new_password: &str,
    ) -> Result<()> {
        if new_password.len() < 8 {
            return Err(Error::PasswordHash("new password must be ≥ 8 chars".into()));
        }
        let stored = self.fetch_pw_hash(id).await?;
        if !verify_password(old_password, &stored)? {
            return Err(Error::InvalidCredentials);
        }
        let new_hash = hash_password(new_password)?;
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query("UPDATE panel_users SET pw_hash = ?, updated_at = ? WHERE id = ?")
                    .bind(&new_hash)
                    .bind(now)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query("UPDATE panel_users SET pw_hash = $1, updated_at = $2 WHERE id = $3")
                    .bind(&new_hash)
                    .bind(now)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
        }
        Ok(())
    }

    async fn fetch_pw_hash(&self, id: i64) -> Result<String> {
        let hash = match &self.db {
            Database::Sqlite(pool) => sqlx::query("SELECT pw_hash FROM panel_users WHERE id = ?")
                .bind(id)
                .fetch_one(pool)
                .await?
                .try_get::<String, _>("pw_hash")?,
            Database::Postgres(pool) => sqlx::query("SELECT pw_hash FROM panel_users WHERE id = $1")
                .bind(id)
                .fetch_one(pool)
                .await?
                .try_get::<String, _>("pw_hash")?,
        };
        Ok(hash)
    }

    async fn touch_last_login(&self, id: i64) -> Result<()> {
        let now = Utc::now();
        match &self.db {
            Database::Sqlite(pool) => {
                sqlx::query("UPDATE panel_users SET last_login_at = ?, updated_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(now)
                    .bind(id)
                    .execute(pool)
                    .await?;
            }
            Database::Postgres(pool) => {
                sqlx::query(
                    "UPDATE panel_users SET last_login_at = $1, updated_at = $1 WHERE id = $2",
                )
                .bind(now)
                .bind(id)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }
}

fn map_sqlite(row: sqlx::sqlite::SqliteRow) -> Result<PanelUser> {
    Ok(PanelUser {
        id:            row.try_get("id")?,
        username:      row.try_get("username")?,
        role:          row.try_get("role")?,
        is_admin:      row.try_get("is_admin")?,
        active:        row.try_get("active")?,
        last_login_at: row.try_get("last_login_at")?,
        created_at:    row.try_get("created_at")?,
        updated_at:    row.try_get("updated_at")?,
    })
}

fn map_postgres(row: sqlx::postgres::PgRow) -> Result<PanelUser> {
    Ok(PanelUser {
        id:            row.try_get("id")?,
        username:      row.try_get("username")?,
        role:          row.try_get("role")?,
        is_admin:      row.try_get("is_admin")?,
        active:        row.try_get("active")?,
        last_login_at: row.try_get("last_login_at")?,
        created_at:    row.try_get("created_at")?,
        updated_at:    row.try_get("updated_at")?,
    })
}
