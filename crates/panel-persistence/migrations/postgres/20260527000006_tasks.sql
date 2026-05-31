CREATE TABLE node_operation_tasks (
    id           BIGSERIAL    PRIMARY KEY,
    node_id      BIGINT       NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind         TEXT         NOT NULL
                 CHECK (kind IN ('apply_config', 'restart', 'check_health')),
    status       TEXT         NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'running', 'success', 'failed')),
    payload      JSONB        NOT NULL DEFAULT '{}'::jsonb,
    log          TEXT         NOT NULL DEFAULT '',
    error        TEXT,
    started_at   TIMESTAMPTZ,
    finished_at  TIMESTAMPTZ,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_tasks_node    ON node_operation_tasks(node_id);
CREATE INDEX idx_tasks_status  ON node_operation_tasks(status);
CREATE INDEX idx_tasks_created ON node_operation_tasks(created_at);
