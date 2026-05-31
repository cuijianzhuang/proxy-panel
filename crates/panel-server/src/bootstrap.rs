//! First-run admin bootstrap: if `panel_users` is empty, create one.

use panel_auth::PanelUserRepo;
use rand::distributions::{Alphanumeric, DistString};

pub async fn ensure_admin(users: &PanelUserRepo, explicit_password: Option<&str>) -> anyhow::Result<()> {
    if users.count().await? > 0 {
        return Ok(());
    }

    let (password, generated) = match explicit_password {
        Some(pw) => (pw.to_string(), false),
        None => (Alphanumeric.sample_string(&mut rand::thread_rng(), 24), true),
    };

    users.create("admin", &password, "admin", true).await?;

    if generated {
        // One-time visible print so a fresh install is actually usable.
        // Subsequent boots will skip this branch entirely.
        println!();
        println!("==========================================================");
        println!("  proxy-panel: first-run admin account created");
        println!("    username: admin");
        println!("    password: {password}");
        println!("  (set PANEL_ADMIN_PASSWORD to choose your own next time)");
        println!("==========================================================");
        println!();
    } else {
        tracing::info!("first-run admin account created (password from PANEL_ADMIN_PASSWORD)");
    }

    Ok(())
}
