-- CDN 优选: pool of domains/IPs that listeners with cdn_enabled rotate through.
CREATE TABLE cdn_endpoints (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    address     TEXT    NOT NULL,                          -- domain (cdn.example.com) or IP
    kind        TEXT    NOT NULL DEFAULT 'domain'
                CHECK (kind IN ('domain', 'ip')),
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 100,              -- lower = higher priority
    note        TEXT,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_cdn_enabled_sort ON cdn_endpoints(enabled, sort_order);

-- 链式代理: SOCKS5/HTTP proxy that listeners can route their outbound through.
-- v1 just models the row; outbound wiring into the renderer comes next round.
CREATE TABLE chain_proxies (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    proxy_type  TEXT    NOT NULL DEFAULT 'socks5'
                CHECK (proxy_type IN ('socks5', 'http')),
    address     TEXT    NOT NULL,                          -- IP or hostname of the upstream proxy
    port        INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    username    TEXT,
    password    TEXT,
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    note        TEXT,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_chain_enabled ON chain_proxies(enabled);
