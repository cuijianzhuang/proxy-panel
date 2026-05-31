CREATE TABLE listeners (
    id                  BIGSERIAL   PRIMARY KEY,
    node_id             BIGINT,
    name                TEXT        NOT NULL,
    core                TEXT        NOT NULL CHECK (core IN ('xray', 'singbox')),
    protocol            TEXT        NOT NULL,
    transport           TEXT        NOT NULL DEFAULT 'tcp',
    tls_mode            TEXT        NOT NULL DEFAULT 'none'
                        CHECK (tls_mode IN ('none', 'tls', 'reality')),
    port                INTEGER     NOT NULL CHECK (port BETWEEN 1 AND 65535),
    params              JSONB       NOT NULL DEFAULT '{}'::jsonb,
    enabled             BOOLEAN     NOT NULL DEFAULT TRUE,
    source_listener_id  BIGINT      REFERENCES listeners(id) ON DELETE SET NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_listeners_node    ON listeners(node_id);
CREATE INDEX idx_listeners_enabled ON listeners(enabled);
