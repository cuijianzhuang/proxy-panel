CREATE TABLE plans (
    id                BIGSERIAL    PRIMARY KEY,
    name              TEXT         NOT NULL,
    quota_type        TEXT         NOT NULL DEFAULT 'permanent'
                      CHECK (quota_type IN ('permanent', 'monthly')),
    quota_gb          DOUBLE PRECISION NOT NULL DEFAULT 0,
    quota_reset_day   INTEGER      NOT NULL DEFAULT 1,
    duration_days     INTEGER,
    device_limit      INTEGER,
    speed_limit_mbps  INTEGER,
    created_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE proxy_users (
    id                  BIGSERIAL    PRIMARY KEY,
    name                TEXT         NOT NULL,
    uuid                TEXT         NOT NULL UNIQUE,
    password            TEXT         NOT NULL,
    plan_id             BIGINT       REFERENCES plans(id) ON DELETE SET NULL,
    enabled             BOOLEAN      NOT NULL DEFAULT TRUE,
    quota_type          TEXT         NOT NULL DEFAULT 'permanent'
                        CHECK (quota_type IN ('permanent', 'monthly')),
    quota_gb            DOUBLE PRECISION NOT NULL DEFAULT 0,
    quota_reset_day     INTEGER      NOT NULL DEFAULT 1,
    last_reset_at       TIMESTAMPTZ,
    used_bytes          BIGINT       NOT NULL DEFAULT 0,
    expires_at          TIMESTAMPTZ,
    speed_limit_mbps    INTEGER,
    device_limit        INTEGER,
    subscription_token  TEXT         NOT NULL UNIQUE,
    note                TEXT,
    tags                JSONB        NOT NULL DEFAULT '[]'::jsonb,
    last_seen_at        TIMESTAMPTZ,
    last_seen_ip        TEXT,
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_proxy_users_plan      ON proxy_users(plan_id);
CREATE INDEX idx_proxy_users_enabled   ON proxy_users(enabled);
CREATE INDEX idx_proxy_users_sub_token ON proxy_users(subscription_token);

CREATE TABLE listener_clients (
    listener_id    BIGINT NOT NULL REFERENCES listeners(id)    ON DELETE CASCADE,
    proxy_user_id  BIGINT NOT NULL REFERENCES proxy_users(id)  ON DELETE CASCADE,
    PRIMARY KEY (listener_id, proxy_user_id)
);

CREATE INDEX idx_listener_clients_user ON listener_clients(proxy_user_id);
