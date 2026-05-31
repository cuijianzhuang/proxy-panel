CREATE TABLE cdn_endpoints (
    id          BIGSERIAL    PRIMARY KEY,
    name        TEXT         NOT NULL,
    address     TEXT         NOT NULL,
    kind        TEXT         NOT NULL DEFAULT 'domain'
                CHECK (kind IN ('domain', 'ip')),
    enabled     BOOLEAN      NOT NULL DEFAULT TRUE,
    sort_order  INTEGER      NOT NULL DEFAULT 100,
    note        TEXT,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_cdn_enabled_sort ON cdn_endpoints(enabled, sort_order);

CREATE TABLE chain_proxies (
    id          BIGSERIAL    PRIMARY KEY,
    name        TEXT         NOT NULL,
    proxy_type  TEXT         NOT NULL DEFAULT 'socks5'
                CHECK (proxy_type IN ('socks5', 'http')),
    address     TEXT         NOT NULL,
    port        INTEGER      NOT NULL CHECK (port BETWEEN 1 AND 65535),
    username    TEXT,
    password    TEXT,
    enabled     BOOLEAN      NOT NULL DEFAULT TRUE,
    note        TEXT,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_chain_enabled ON chain_proxies(enabled);
