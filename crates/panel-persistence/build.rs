// Tell cargo to re-run (and thus re-expand the `sqlx::migrate!` macro, which
// embeds the .sql files at compile time) whenever a migration changes or is
// added. Without this, dropping a new migration file in does NOT trigger a
// rebuild of this crate, so the embedded migrator goes stale and the new
// migration silently never runs.
fn main() {
    println!("cargo:rerun-if-changed=migrations/sqlite");
    println!("cargo:rerun-if-changed=migrations/postgres");
}
