-- Per-collection traffic increments. The collector polls each online node,
-- gets per-user up/down byte deltas since the last poll, and appends one row
-- per (node, user) per cycle. Aggregation (totals, daily) is done on read.
CREATE TABLE stats_samples (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id       INTEGER NOT NULL,
    proxy_user_id INTEGER NOT NULL,
    ts            TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    up_delta      INTEGER NOT NULL DEFAULT 0,
    down_delta    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_stats_user_ts ON stats_samples(proxy_user_id, ts);
CREATE INDEX idx_stats_ts      ON stats_samples(ts);
