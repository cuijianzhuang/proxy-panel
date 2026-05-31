CREATE TABLE nodes (
    id            BIGSERIAL    PRIMARY KEY,
    name          TEXT         NOT NULL,
    addr          TEXT         NOT NULL,
    public_host   TEXT,
    core          TEXT         NOT NULL CHECK (core IN ('xray', 'singbox')),
    core_version  TEXT,
    mgmt_port     INTEGER      NOT NULL DEFAULT 0,
    mgmt_secret   TEXT,
    ssh_port      INTEGER      NOT NULL DEFAULT 22,
    ssh_user      TEXT         NOT NULL DEFAULT 'root',
    status        TEXT         NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'provisioning', 'online', 'offline', 'failed')),
    last_seen_at  TIMESTAMPTZ,
    note          TEXT,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_nodes_status ON nodes(status);
