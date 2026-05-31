-- Persistent async task queue. Each row is a single operation against a node
-- (apply_config / restart / check_health). The background worker picks rows
-- in `pending` status, flips them to `running`, executes, then writes the
-- final status + error + log. Worker crash recovery: on startup, any row
-- stuck in `running` is moved back to `pending`.
CREATE TABLE node_operation_tasks (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id      INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind         TEXT    NOT NULL
                 CHECK (kind IN ('apply_config', 'restart', 'check_health')),
    status       TEXT    NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'running', 'success', 'failed')),
    payload      TEXT    NOT NULL DEFAULT '{}',
    log          TEXT    NOT NULL DEFAULT '',
    error        TEXT,
    started_at   TEXT,
    finished_at  TEXT,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_tasks_node    ON node_operation_tasks(node_id);
CREATE INDEX idx_tasks_status  ON node_operation_tasks(status);
CREATE INDEX idx_tasks_created ON node_operation_tasks(created_at);
