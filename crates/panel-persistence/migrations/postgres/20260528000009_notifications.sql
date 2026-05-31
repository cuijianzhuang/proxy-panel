CREATE TABLE notification_channels (
    id          BIGSERIAL    PRIMARY KEY,
    name        TEXT         NOT NULL,
    type        TEXT         NOT NULL CHECK (type IN ('telegram', 'webhook', 'smtp')),
    config_json JSONB        NOT NULL DEFAULT '{}'::jsonb,
    enabled     BOOLEAN      NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE notification_rules (
    event_type  TEXT         PRIMARY KEY,
    channel_ids JSONB        NOT NULL DEFAULT '[]'::jsonb,
    enabled     BOOLEAN      NOT NULL DEFAULT TRUE,
    updated_at  TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
