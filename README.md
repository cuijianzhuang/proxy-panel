# proxy-panel

Self-hosted Xray / sing-box management panel.

Status: **M1 — scaffold**. See [design doc](../../Users/cuijianzhuang/xray-singbox-panel-design.md) for the target architecture.

## 一键启动 (one-click)

Builds the frontend (if needed) + backend, then boots with sensible defaults
and prints the access info. Single binary serves API + embedded UI.

```bash
# Linux / macOS
./start.sh                 # dev build
./start.sh --release       # release build
./start.sh --skip-build    # run existing binary

# Windows (PowerShell)
.\start.ps1
.\start.ps1 -Release
.\start.ps1 -SkipBuild
```

Override defaults via env: `PANEL_BIND`, `DATABASE_URL`, `PANEL_PUBLIC_HOST`,
`PANEL_ADMIN_PASSWORD` (auto-generated + printed on first run if unset),
`PANEL_REMOTE_MODE` (`dry-run` | `ssh`).

## Docker

Multi-stage build → single ~小 image on `debian-slim` (frontend embedded in the
binary; rustls + bundled SQLite, no openssl/libsqlite needed at runtime).

```bash
# SQLite (default) — data persists in a named volume
PANEL_ADMIN_PASSWORD=changeme docker compose up -d --build
docker compose logs -f panel          # watch boot / auto-generated password

# or plain docker
docker build -t proxy-panel .
docker run -d -p 8080:8080 -v proxy-panel-data:/data \
    -e PANEL_ADMIN_PASSWORD=changeme proxy-panel

# PostgreSQL instead of SQLite
export DATABASE_URL="postgres://panel:panelpw@postgres:5432/panel"
docker compose --profile postgres up -d --build
```

Container defaults: binds `0.0.0.0:8080`, DB at `/data/panel.db` (mount `/data`
to persist), runs as non-root uid 65534. `HEALTHCHECK` uses the binary's own
`--healthcheck` (a dependency-free TCP probe of `/api/healthz`).

Behind a TLS reverse proxy, also set `PANEL_COOKIE_SECURE=1`.

## Run (manual, dev)

```pwsh
# defaults: bind 127.0.0.1:8080, sqlite://./data/panel.db (auto-created)
cargo run -p panel-server
curl http://127.0.0.1:8080/api/healthz
# -> {"status":"ok","db":{"kind":"sqlite","ping":"ok"}, ...}
```

### Choosing the database

`DATABASE_URL` switches backend at runtime — no rebuild needed.

```pwsh
# SQLite (default)
$env:DATABASE_URL = "sqlite://./data/panel.db"

# PostgreSQL
$env:DATABASE_URL = "postgres://panel:secret@localhost:5432/panel"
```

Inspect the connected DB's schema and applied migrations:

```pwsh
cargo run -p panel-persistence --bin dump_schema
```

Migrations live under `crates/panel-persistence/migrations/{sqlite,postgres}/` —
keep both dialects in lockstep when adding a new migration file.

### First run

On first launch, if `panel_users` is empty, an `admin` account is created:

- Set `PANEL_ADMIN_PASSWORD` to choose the password, or leave unset to have one
  generated and printed to stdout once.
- Subsequent launches skip the bootstrap.

### Auth endpoints

```pwsh
# login -> sets HttpOnly Set-Cookie: vpspanel_session=<64-hex>
curl -X POST http://127.0.0.1:8080/api/login `
  -H "Content-Type: application/json" `
  -d '{"username":"admin","password":"..."}' `
  -c cookies.txt

# me -> 200 with user JSON, 401 if no/expired session
curl http://127.0.0.1:8080/api/me -b cookies.txt

# logout -> clears cookie, drops the session row
curl -X POST http://127.0.0.1:8080/api/logout -b cookies.txt
```

Set `PANEL_COOKIE_SECURE=1` in production (with TLS in front) to mark the cookie
`Secure`.

### Listener CRUD

```pwsh
# any authed user can read
curl http://127.0.0.1:8080/api/listeners       -b cookies.txt
curl http://127.0.0.1:8080/api/listeners/1     -b cookies.txt

# admin only: create
curl -X POST http://127.0.0.1:8080/api/listeners -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"name":"vl-reality","core":"xray","protocol":"vless","tls_mode":"reality","port":24299,"params":{"flow":"xtls-rprx-vision"}}'

# partial update (PUT, only non-null fields applied)
curl -X PUT http://127.0.0.1:8080/api/listeners/1 -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"enabled":false}'

curl -X DELETE http://127.0.0.1:8080/api/listeners/1 -b cookies.txt
```

Permission gates:

| Action            | viewer | admin |
|-------------------|:------:|:-----:|
| GET list / one    | ✓      | ✓     |
| POST create       | 403    | ✓     |
| PUT update        | 403    | ✓     |
| DELETE            | 403    | ✓     |

### Dev helpers

```pwsh
# Add a user from the CLI
cargo run -p panel-auth --bin add_user -- viewer viewer-pw
cargo run -p panel-auth --bin add_user -- alice secret --admin
```

### Rendering a listener to core config

Each listener carries a `core` (`xray` | `singbox`). The matching adapter
renders it into the JSON shape that core expects in its `inbounds[]` array:

```pwsh
curl http://127.0.0.1:8080/api/listeners/1/preview -b cookies.txt
```

Returns `{ core, inbound }`. The `inbound` is plug-and-play into the target
core's config; `clients` / `users` arrays are empty until proxy users land.

Unsupported combos (e.g. xray + hysteria2) return `422` with an explicit
message rather than producing a broken config the core would reject.

### End-to-end flow

```pwsh
# 1. as admin: create plan
curl -X POST http://127.0.0.1:8080/api/plans -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"name":"NO.01 100G","quota_type":"permanent","quota_gb":100}'

# 2. create proxy user (server generates uuid + password + subscription_token)
curl -X POST http://127.0.0.1:8080/api/proxy-users -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"name":"alice","plan_id":1,"quota_gb":100}'

# 3. attach the user to a listener
curl -X POST http://127.0.0.1:8080/api/listeners/1/clients -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"proxy_user_id":1}'

# 4. preview rendered inbound (now includes the attached user)
curl http://127.0.0.1:8080/api/listeners/1/preview -b cookies.txt

# 5. share /sub/<subscription_token> with the user (no auth, token IS auth)
curl http://127.0.0.1:8080/sub/<token>
# -> base64-encoded list of vless:// / trojan:// / ss:// URIs
```

Set `PANEL_PUBLIC_HOST=panel.example.com` to control the host portion of the
generated URIs (otherwise falls back to the bind IP, useful only for local dev).

### Nodes & full config rendering

```pwsh
# create a node (the VPS that will actually run xray/sing-box)
curl -X POST http://127.0.0.1:8080/api/nodes -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"name":"vps-hk-01","addr":"1.2.3.4","core":"xray","mgmt_port":10086,"mgmt_secret":"..."}'

# create listeners pinned to that node (node_id set)
curl -X POST http://127.0.0.1:8080/api/listeners -b cookies.txt `
  -H "Content-Type: application/json" `
  -d '{"name":"vl-reality","core":"xray","protocol":"vless","tls_mode":"reality","port":24299,"node_id":1,"params":{...}}'

# fetch the *whole* config.json for that node — ready to write to /etc/xray/config.json
curl http://127.0.0.1:8080/api/nodes/1/config -b cookies.txt
# -> { node_id, core, inbound_count, config: { log, api, stats, policy,
#       inbounds:[...all enabled listeners + api inbound...], outbounds, routing } }
```

The renderer enforces `listener.core == node.core` and returns `422` with a
precise message if they mismatch. This is the same `CoreAdapter::render_node_config`
path the SSH-deployer uses below to push configs to nodes.

### Pushing configs to nodes (task queue)

Apply operations are async — the API enqueues a row in `node_operation_tasks`
and a background worker picks it up. Calls return `202 Accepted` with the
`task_id`; poll `/api/tasks/<id>` for status + log.

```pwsh
# render + push the current config to the node
curl -X POST http://127.0.0.1:8080/api/nodes/1/apply -b cookies.txt
# -> { "task_id": 1, "status": "pending" }

# poll
curl http://127.0.0.1:8080/api/tasks/1 -b cookies.txt
# -> { id, kind:"apply_config", status:"success", log:"...", started_at, finished_at }

# other operations
curl -X POST http://127.0.0.1:8080/api/nodes/1/restart      -b cookies.txt
curl -X POST http://127.0.0.1:8080/api/nodes/1/health-check -b cookies.txt

# recent tasks (newest first)
curl http://127.0.0.1:8080/api/tasks?limit=20 -b cookies.txt
```

The worker's SSH backend is chosen by `PANEL_REMOTE_MODE`:

| Mode | Behaviour |
|---|---|
| `dry-run` (default) | In-process mock. Every task succeeds, log shows what would have happened. Use for dev / CI. |
| `ssh` | Real SSH via `russh`. Auth from `PANEL_SSH_KEY_PATH` (preferred) or `PANEL_SSH_PASSWORD`. |

```pwsh
# real SSH (pubkey)
$env:PANEL_REMOTE_MODE   = "ssh"
$env:PANEL_SSH_KEY_PATH  = "$env:USERPROFILE\.ssh\id_ed25519"
cargo run -p panel-server

# real SSH (password)
$env:PANEL_REMOTE_MODE   = "ssh"
$env:PANEL_SSH_PASSWORD  = "..."
cargo run -p panel-server
```

The SSH client:
- Connects to `node.addr:node.ssh_port` as `node.ssh_user`.
- `write_file` uses `base64 -d` + atomic rename, so it works for binary content
  on any POSIX target with coreutils.
- `exec` captures stdout / stderr / exit code, all written into the task log.
- Connection / auth / command failures land in `task.error` with a precise
  message; the worker stays up and keeps draining the queue.
- **Host key verification**: v1 accepts every key. Pinning / TOFU is a follow-up;
  run only over networks you trust until then.

Worker behaviour:
- Polls every 500ms (configurable later).
- Crash recovery: on startup, rows stuck in `running` are moved back to `pending`.
- One task at a time across the queue (per-node serialisation is a future tweak).

## Layout

```
crates/
  panel-server/        # axum HTTP server (entry point); embeds web/dist via rust-embed
  panel-persistence/   # sqlx + SQLite/PostgreSQL + migrations
  panel-auth/          # argon2 + session cookies + repos
  panel-domain/        # Listener / Plan / ProxyUser / Node + repos
  panel-core/          # CoreAdapter trait + render IR
  panel-core-xray/     # Xray-core adapter
  panel-core-singbox/  # sing-box adapter
  panel-node/          # NodeRemote trait + DryRun / russh-backed SSH impls
  panel-task/          # persistent task queue + background worker
web/                   # React 18 + Vite + Tailwind frontend (sakura 🌸 theme)
```

## Frontend

```pwsh
# dev: hot-reload UI on :5173, proxies /api + /sub to the Rust server on :8080
cd web
npm install      # one-time
npm run dev

# production: build static assets, then the Rust binary embeds them
npm run build    # outputs web/dist/
cd ..
cargo build -p panel-server --release
# the single binary now serves both the API and the SPA
```
