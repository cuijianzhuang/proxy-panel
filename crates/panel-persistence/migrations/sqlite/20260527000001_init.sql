-- Initial schema (SQLite)
--
-- Note on booleans: SQLite has no native BOOLEAN, but sqlx-sqlite maps any
-- column whose declared type contains "BOOL" to Rust `bool`. We rely on that
-- so the Rust side can use a single `bool` type against both backends.
CREATE TABLE panel_users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT    NOT NULL UNIQUE,
    pw_hash       TEXT    NOT NULL,
    role          TEXT    NOT NULL DEFAULT 'viewer',
    is_admin      BOOLEAN NOT NULL DEFAULT 0,
    active        BOOLEAN NOT NULL DEFAULT 1,
    totp_secret   TEXT,
    last_login_at TEXT,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_panel_users_username ON panel_users(username);
