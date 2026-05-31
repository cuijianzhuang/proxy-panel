CREATE TABLE audit_logs (
    id          BIGSERIAL    PRIMARY KEY,
    actor_id    BIGINT,
    actor_name  TEXT,
    method      TEXT         NOT NULL,
    path        TEXT         NOT NULL,
    status      INTEGER      NOT NULL,
    ip          TEXT,
    user_agent  TEXT,
    ts          TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_audit_ts    ON audit_logs(ts DESC);
CREATE INDEX idx_audit_actor ON audit_logs(actor_id, ts DESC);

CREATE TABLE backups (
    id          BIGSERIAL    PRIMARY KEY,
    filename    TEXT         NOT NULL UNIQUE,
    size_bytes  BIGINT       NOT NULL,
    kind        TEXT         NOT NULL DEFAULT 'manual'
                CHECK (kind IN ('manual', 'auto')),
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_backups_created ON backups(created_at DESC);
