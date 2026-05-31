-- Plans: pricing/quota templates that proxy_users can subscribe to.
CREATE TABLE plans (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    name              TEXT    NOT NULL,
    quota_type        TEXT    NOT NULL DEFAULT 'permanent'
                      CHECK (quota_type IN ('permanent', 'monthly')),
    quota_gb          REAL    NOT NULL DEFAULT 0,    -- 0 = unlimited
    quota_reset_day   INTEGER NOT NULL DEFAULT 1,    -- only meaningful when monthly
    duration_days     INTEGER,                       -- NULL = no expiry
    device_limit      INTEGER,                       -- NULL = no limit
    speed_limit_mbps  INTEGER,                       -- NULL = no throttle
    created_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Proxy users: the people whose subscription URLs feed into client apps.
-- A proxy user's identity is the `uuid` (vless/vmess) or `password` (trojan/ss);
-- both are stored so the same row works across protocols. `subscription_token`
-- is a separate, rotatable secret used by /sub/<token>.
CREATE TABLE proxy_users (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT    NOT NULL,
    uuid                TEXT    NOT NULL UNIQUE,
    password            TEXT    NOT NULL,
    plan_id             INTEGER REFERENCES plans(id) ON DELETE SET NULL,
    enabled             BOOLEAN NOT NULL DEFAULT 1,
    quota_type          TEXT    NOT NULL DEFAULT 'permanent'
                        CHECK (quota_type IN ('permanent', 'monthly')),
    quota_gb            REAL    NOT NULL DEFAULT 0,
    quota_reset_day     INTEGER NOT NULL DEFAULT 1,
    last_reset_at       TEXT,
    used_bytes          INTEGER NOT NULL DEFAULT 0,
    expires_at          TEXT,
    speed_limit_mbps    INTEGER,
    device_limit        INTEGER,
    subscription_token  TEXT    NOT NULL UNIQUE,
    note                TEXT,
    tags                TEXT    NOT NULL DEFAULT '[]',
    last_seen_at        TEXT,
    last_seen_ip        TEXT,
    created_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at          TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_proxy_users_plan      ON proxy_users(plan_id);
CREATE INDEX idx_proxy_users_enabled   ON proxy_users(enabled);
CREATE INDEX idx_proxy_users_sub_token ON proxy_users(subscription_token);

-- Many-to-many: which listeners each proxy user has access to. Both sides
-- cascade-delete since orphan rows would just create rendering ambiguity.
CREATE TABLE listener_clients (
    listener_id    INTEGER NOT NULL REFERENCES listeners(id)    ON DELETE CASCADE,
    proxy_user_id  INTEGER NOT NULL REFERENCES proxy_users(id)  ON DELETE CASCADE,
    PRIMARY KEY (listener_id, proxy_user_id)
);

CREATE INDEX idx_listener_clients_user ON listener_clients(proxy_user_id);
