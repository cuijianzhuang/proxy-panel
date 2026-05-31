-- Initial schema (PostgreSQL)
CREATE TABLE panel_users (
    id            BIGSERIAL    PRIMARY KEY,
    username      TEXT         NOT NULL UNIQUE,
    pw_hash       TEXT         NOT NULL,
    role          TEXT         NOT NULL DEFAULT 'viewer',
    is_admin      BOOLEAN      NOT NULL DEFAULT FALSE,
    active        BOOLEAN      NOT NULL DEFAULT TRUE,
    totp_secret   TEXT,
    last_login_at TIMESTAMPTZ,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_panel_users_username ON panel_users(username);
