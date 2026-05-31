//! Persistence layer with runtime-dispatched SQLite / PostgreSQL backends.
//!
//! Choose backend via the `DATABASE_URL`:
//! - `sqlite://./data/panel.db` (file is created if missing)
//! - `sqlite::memory:` (ephemeral, for tests)
//! - `postgres://user:pw@host:5432/dbname`
//!
//! Migrations live in `migrations/{sqlite,postgres}/` and are dispatched per dialect.

use std::path::Path;
use std::str::FromStr;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{PgPool, SqlitePool};

static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("./migrations/sqlite");
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbKind {
    Sqlite,
    Postgres,
}

impl DbKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
        }
    }
}

/// Runtime-dispatched DB handle. Cheap to `Clone` (wraps `Arc`-like pools).
#[derive(Debug, Clone)]
pub enum Database {
    Sqlite(SqlitePool),
    Postgres(PgPool),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unsupported DATABASE_URL scheme: {0} (expected sqlite:// or postgres://)")]
    UnsupportedScheme(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Database {
    /// Connect using a URL whose scheme decides the backend.
    pub async fn connect(url: &str) -> Result<Self> {
        if is_sqlite(url) {
            let opts = SqliteConnectOptions::from_str(url)?
                .create_if_missing(true)
                .foreign_keys(true)
                .busy_timeout(std::time::Duration::from_secs(5));

            ensure_sqlite_parent_dir(&opts)?;

            let pool = SqlitePoolOptions::new()
                .max_connections(8)
                .connect_with(opts)
                .await?;
            tracing::info!("connected to sqlite database");
            Ok(Self::Sqlite(pool))
        } else if is_postgres(url) {
            let opts = PgConnectOptions::from_str(url)?;
            let pool = PgPoolOptions::new()
                .max_connections(16)
                .connect_with(opts)
                .await?;
            tracing::info!("connected to postgres database");
            Ok(Self::Postgres(pool))
        } else {
            let scheme = url.split(':').next().unwrap_or("").to_string();
            Err(Error::UnsupportedScheme(scheme))
        }
    }

    pub fn kind(&self) -> DbKind {
        match self {
            Self::Sqlite(_) => DbKind::Sqlite,
            Self::Postgres(_) => DbKind::Postgres,
        }
    }

    /// Run all pending migrations for the active backend.
    pub async fn migrate(&self) -> Result<()> {
        match self {
            Self::Sqlite(pool) => SQLITE_MIGRATOR.run(pool).await?,
            Self::Postgres(pool) => POSTGRES_MIGRATOR.run(pool).await?,
        }
        tracing::info!(kind = self.kind().as_str(), "migrations applied");
        Ok(())
    }

    /// Cheap round-trip to verify the connection is alive.
    pub async fn ping(&self) -> Result<()> {
        match self {
            Self::Sqlite(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            }
            Self::Postgres(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            }
        }
        Ok(())
    }
}

fn is_sqlite(url: &str) -> bool {
    url.starts_with("sqlite:")
}

fn is_postgres(url: &str) -> bool {
    url.starts_with("postgres:") || url.starts_with("postgresql:")
}

/// SqliteConnectOptions::create_if_missing creates the file, but not its parent
/// directory. Make sure the dir exists so first boot doesn't fail.
fn ensure_sqlite_parent_dir(opts: &SqliteConnectOptions) -> Result<()> {
    let filename: &Path = opts.get_filename().as_ref();
    if let Some(parent) = filename.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
