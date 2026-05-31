-- Per-node SSH credentials. Until now the panel used one global key/password
-- (PANEL_SSH_KEY) for every node; these columns let each VPS carry its own
-- password or inline private key, entered when the node is added.
--
-- ssh_auth_method:
--   'global'   — use the panel's configured global credential (default,
--                backward-compatible with existing rows)
--   'password' — authenticate with `ssh_password`
--   'key'      — authenticate with the inline PEM in `ssh_private_key`
--
-- `ssh_password` / `ssh_private_key` are secrets: the API never serialises them
-- back to clients (it only exposes `ssh_auth_method` + a "has credential" flag).
ALTER TABLE nodes ADD COLUMN ssh_auth_method  TEXT NOT NULL DEFAULT 'global';
ALTER TABLE nodes ADD COLUMN ssh_password     TEXT;
ALTER TABLE nodes ADD COLUMN ssh_private_key  TEXT;
