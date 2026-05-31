-- Listeners (proxy inbounds). Each listener belongs to a node (added later);
-- for now `node_id` is nullable so this table can land before `nodes` exists.
--
-- `params` is dialect-specific: JSON-as-TEXT on SQLite, JSONB on PostgreSQL.
-- Rust binds it through `sqlx::types::Json<serde_json::Value>` on both sides.
CREATE TABLE listeners (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id             INTEGER,
    name                TEXT    NOT NULL,
    core                TEXT    NOT NULL CHECK (core IN ('xray', 'singbox')),
    protocol            TEXT    NOT NULL,
    transport           TEXT    NOT NULL DEFAULT 'tcp',
    tls_mode            TEXT    NOT NULL DEFAULT 'none'
                        CHECK (tls_mode IN ('none', 'tls', 'reality')),
    port                INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    params              TEXT    NOT NULL DEFAULT '{}',
    enabled             BOOLEAN NOT NULL DEFAULT 1,
    source_listener_id  INTEGER REFERENCES listeners(id) ON DELETE SET NULL,
    created_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_listeners_node     ON listeners(node_id);
CREATE INDEX idx_listeners_enabled  ON listeners(enabled);
