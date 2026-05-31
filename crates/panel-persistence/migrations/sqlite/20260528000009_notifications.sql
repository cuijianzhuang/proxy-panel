-- 告警通知通道。config_json holds type-specific fields:
--   telegram: { "bot_token": "...", "chat_id": "..." }
--   webhook:  { "url": "...", "header_name": "?", "header_value": "?" }
--   smtp:     { "host","port","username","password","from","to","tls" }
CREATE TABLE notification_channels (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,
    type        TEXT    NOT NULL CHECK (type IN ('telegram', 'webhook', 'smtp')),
    config_json TEXT    NOT NULL DEFAULT '{}',
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Event → channels routing. One row per event type; `channels` is a JSON
-- array of channel ids. Rows are seeded lazily by the API the first time an
-- event type is touched, so this table starts empty.
CREATE TABLE notification_rules (
    event_type  TEXT    PRIMARY KEY,
    channel_ids TEXT    NOT NULL DEFAULT '[]',   -- JSON array of channel ids
    enabled     BOOLEAN NOT NULL DEFAULT 1,
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
