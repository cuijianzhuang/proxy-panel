-- Extend the node_operation_tasks.kind CHECK constraint to allow 'provision'.
-- SQLite doesn't support ALTER COLUMN, so we recreate the table.
CREATE TABLE node_operation_tasks_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id     INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL CHECK(kind IN ('apply_config','restart','check_health','provision')),
    status      TEXT    NOT NULL DEFAULT 'pending'
                        CHECK(status IN ('pending','running','success','failed')),
    payload     TEXT    NOT NULL DEFAULT '{}',
    log         TEXT    NOT NULL DEFAULT '',
    error       TEXT,
    started_at  DATETIME,
    finished_at DATETIME,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
INSERT INTO node_operation_tasks_new SELECT * FROM node_operation_tasks;
DROP TABLE node_operation_tasks;
ALTER TABLE node_operation_tasks_new RENAME TO node_operation_tasks;
CREATE INDEX idx_not_node   ON node_operation_tasks(node_id);
CREATE INDEX idx_not_status ON node_operation_tasks(status);
