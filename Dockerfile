# =============================================================================
# proxy-panel — multi-stage build → single static-ish binary on debian-slim.
#
#   docker build -t proxy-panel .
#   docker run -p 8080:8080 -v proxy-panel-data:/data \
#       -e PANEL_ADMIN_PASSWORD=changeme proxy-panel
#
# Build order matters: the frontend is compiled FIRST so the Rust stage can
# embed web/dist via rust-embed. Change the frontend → the web stage cache
# busts → the embedded assets refresh.
# =============================================================================

# ---- 1. frontend -----------------------------------------------------------
FROM node:22-slim AS web
WORKDIR /web
# Install deps first for layer caching (package.json changes rarely).
COPY web/package.json web/package-lock.json* ./
RUN npm install
COPY web/ ./
RUN npm run build          # → /web/dist

# ---- 2. backend ------------------------------------------------------------
FROM rust:1-bookworm AS build
WORKDIR /app
COPY . .
# Drop in the freshly built SPA so `rust-embed` (in panel-server/src/spa.rs)
# embeds it at compile time. Overwrites any dist that slipped through .dockerignore.
COPY --from=web /web/dist ./web/dist
# sqlx-sqlite bundles its own SQLite; russh + lettre use rustls — no openssl,
# no libsqlite3 needed at build or runtime.
RUN cargo build --release -p panel-server

# ---- 3. runtime ------------------------------------------------------------
FROM debian:bookworm-slim AS runtime
# ca-certificates: needed for outbound TLS (Telegram / SMTP / webhook / SSH not).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/panel-server /usr/local/bin/panel-server

# Defaults suited to a container: bind all interfaces, DB on the data volume.
ENV PANEL_BIND=0.0.0.0:8080 \
    DATABASE_URL=sqlite:///data/panel.db \
    PANEL_REMOTE_MODE=dry-run \
    RUST_LOG=info,sqlx=warn

# SQLite file, backups, and any local state live here — mount a volume to persist.
VOLUME /data
EXPOSE 8080

# Drop privileges. The data volume is chowned by the entrypoint-less CMD via
# Docker's volume init; if you bind-mount a host dir, `chown 65534` it first.
USER 65534:65534

CMD ["panel-server"]
