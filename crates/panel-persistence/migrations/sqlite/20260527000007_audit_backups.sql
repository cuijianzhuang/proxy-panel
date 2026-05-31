-- Audit log: who did what, when. Populated by the panel-server middleware
-- on every successful write (POST/PUT/DELETE). Reads are not logged.
CREATE TABLE audit_logs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    actor_id    INTEGER,                  -- nullable: anonymous /sub/* writes are NULL
    actor_name  TEXT,
    method      TEXT    NOT NULL,
    path        TEXT    NOT NULL,
    status      INTEGER NOT NULL,
    ip          TEXT,
    user_agent  TEXT,
    ts          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_audit_ts     ON audit_logs(ts DESC);
CREATE INDEX idx_audit_actor  ON audit_logs(actor_id, ts DESC);

-- Backup ledger. The actual `.db` files live on disk under data/backups/;
-- this table is just the catalog the UI shows.
CREATE TABLE backups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    filename    TEXT    NOT NULL UNIQUE,
    size_bytes  INTEGER NOT NULL,
    kind        TEXT    NOT NULL DEFAULT 'manual'
                CHECK (kind IN ('manual', 'auto')),
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_backups_created ON backups(created_at DESC);
