CREATE TABLE sessions (
    token_hash   TEXT        PRIMARY KEY,
    user_id      BIGINT      NOT NULL REFERENCES panel_users(id) ON DELETE CASCADE,
    expires_at   TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    ip           TEXT,
    user_agent   TEXT
);

CREATE INDEX idx_sessions_user    ON sessions(user_id);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);
