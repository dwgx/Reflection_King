# syntax=docker/dockerfile:1

FROM node:22-bookworm AS dashboard-build
WORKDIR /src
COPY apps/reflection-dashboard/package*.json apps/reflection-dashboard/
RUN cd apps/reflection-dashboard && npm ci
COPY apps/reflection-dashboard apps/reflection-dashboard
RUN cd apps/reflection-dashboard && npm run build

FROM node:22-bookworm AS browser-build
WORKDIR /src/services/reflection-browser
COPY services/reflection-browser/package*.json ./
RUN npm ci
COPY services/reflection-browser ./
RUN npm run build && npm prune --omit=dev

FROM rust:1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY crates crates
COPY docs/static docs/static
COPY --from=dashboard-build /src/crates/reflection-api/dashboard-dist crates/reflection-api/dashboard-dist
RUN cargo build --release -p reflection-api

FROM node:22-bookworm AS runtime
WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    ffmpeg \
    openssl \
    python3 \
    python3-pip \
    python3-venv \
    sqlite3 \
    tini \
  && rm -rf /var/lib/apt/lists/*

RUN python3 -m venv /opt/venv \
  && /opt/venv/bin/python -m pip install --upgrade pip \
  && /opt/venv/bin/python -m pip install --upgrade yt-dlp you-get streamlink

COPY --from=rust-build /src/target/release/reflection-api /app/reflection-api
COPY --from=rust-build /src/crates/reflection-api/dashboard-dist /app/crates/reflection-api/dashboard-dist
COPY --from=browser-build /src/services/reflection-browser/package*.json /app/services/reflection-browser/
COPY --from=browser-build /src/services/reflection-browser/node_modules /app/services/reflection-browser/node_modules
COPY --from=browser-build /src/services/reflection-browser/dist /app/services/reflection-browser/dist
COPY scripts/deploy/docker-entrypoint.sh /usr/local/bin/reflection-king-entrypoint

RUN cd /app/services/reflection-browser \
  && npx playwright install --with-deps chromium

RUN chmod +x /usr/local/bin/reflection-king-entrypoint /app/reflection-api \
  && mkdir -p /data

ENV APP_DIR=/app \
    RK_PUBLIC_PORT=8780 \
    RK_BIND_ADDRESS=0.0.0.0:8780 \
    RK_PUBLIC_BASE_URL=http://localhost:8780 \
    RK_STORAGE_DIR=/data \
    RK_FFMPEG_PATH=ffmpeg \
    RK_BROWSER_PROBE_URL=http://127.0.0.1:8791 \
    RK_BROWSER_HOST=127.0.0.1 \
    RK_BROWSER_PORT=8791 \
    RK_BROWSER_PROFILE_ROOT=/data/browser-profiles \
    RK_BROWSER_DEFAULT_PROFILE=admin_default \
    RK_BROWSER_HEADED=0 \
    RK_YTDLP_PATH=/opt/venv/bin/yt-dlp \
    RK_YOU_GET_PATH=/opt/venv/bin/you-get \
    RK_STREAMLINK_PATH=/opt/venv/bin/streamlink

VOLUME ["/data"]
EXPOSE 8780
ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/reflection-king-entrypoint"]
