CREATE TABLE stats_samples (
    id            BIGSERIAL    PRIMARY KEY,
    node_id       BIGINT       NOT NULL,
    proxy_user_id BIGINT       NOT NULL,
    ts            TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    up_delta      BIGINT       NOT NULL DEFAULT 0,
    down_delta    BIGINT       NOT NULL DEFAULT 0
);
CREATE INDEX idx_stats_user_ts ON stats_samples(proxy_user_id, ts);
CREATE INDEX idx_stats_ts      ON stats_samples(ts);
