//! Developer helper: `cargo run -p panel-auth --bin add_user -- <user> <pw> [--admin]`
//!
//! Uses the same `PanelUserRepo` as the server, so the resulting row is
//! interchangeable. Intended for dev seeding only; production should go
//! through the UI / API once user management lands.

use panel_auth::PanelUserRepo;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: add_user <username> <password> [--admin]");
        std::process::exit(2);
    }
    let username = &args[0];
    let password = &args[1];
    let is_admin = args.iter().any(|a| a == "--admin");
    let role = if is_admin { "admin" } else { "viewer" };

    let url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://./data/panel.db".to_string());
    let db = panel_persistence::Database::connect(&url).await?;
    db.migrate().await?;

    let repo = PanelUserRepo::new(db);
    let id = repo.create(username, password, role, is_admin).await?;
    println!("created user id={id} username={username} role={role} is_admin={is_admin}");
    Ok(())
}
