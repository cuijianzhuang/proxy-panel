-- Nodes: the VPSes running xray / sing-box. A listener can be attached to
-- exactly one node (listeners.node_id), and a node carries the metadata
-- needed to (eventually) SSH into it and pull stats from its mgmt port.
CREATE TABLE nodes (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL,
    addr          TEXT    NOT NULL,                           -- IP or hostname for SSH/stats
    public_host   TEXT,                                       -- override host in subscription URIs (CDN)
    core          TEXT    NOT NULL CHECK (core IN ('xray', 'singbox')),
    core_version  TEXT,
    mgmt_port     INTEGER NOT NULL DEFAULT 0,                 -- 0 = stats API not provisioned
    mgmt_secret   TEXT,                                       -- bearer for stats endpoint
    ssh_port      INTEGER NOT NULL DEFAULT 22,
    ssh_user      TEXT    NOT NULL DEFAULT 'root',
    status        TEXT    NOT NULL DEFAULT 'pending'
                  CHECK (status IN ('pending', 'provisioning', 'online', 'offline', 'failed')),
    last_seen_at  TEXT,
    note          TEXT,
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_nodes_status ON nodes(status);
