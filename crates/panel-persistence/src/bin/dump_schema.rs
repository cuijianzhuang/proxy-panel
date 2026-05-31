//! Quick CLI utility to dump the connected DB's `panel_users` schema and the
//! applied migration list. Used as a smoke test from the dev shell.

use sqlx::Row;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://./data/panel.db".to_string());
    let db = panel_persistence::Database::connect(&url).await?;
    db.migrate().await?;

    match &db {
        panel_persistence::Database::Sqlite(pool) => {
            println!("[sqlite] panel_users columns:");
            let rows = sqlx::query("PRAGMA table_info(panel_users)")
                .fetch_all(pool)
                .await?;
            for r in &rows {
                let name: String = r.try_get("name")?;
                let ty: String = r.try_get("type")?;
                println!("  - {name} : {ty}");
            }
            println!("[sqlite] applied migrations:");
            let rows = sqlx::query("SELECT version, description FROM _sqlx_migrations ORDER BY version")
                .fetch_all(pool)
                .await?;
            for r in &rows {
                let v: i64 = r.try_get("version")?;
                let d: String = r.try_get("description")?;
                println!("  - {v}  {d}");
            }
        }
        panel_persistence::Database::Postgres(pool) => {
            println!("[postgres] panel_users columns:");
            let rows = sqlx::query(
                "SELECT column_name, data_type FROM information_schema.columns \
                 WHERE table_name = 'panel_users' ORDER BY ordinal_position",
            )
            .fetch_all(pool)
            .await?;
            for r in &rows {
                let name: String = r.try_get("column_name")?;
                let ty: String = r.try_get("data_type")?;
                println!("  - {name} : {ty}");
            }
        }
    }

    Ok(())
}
