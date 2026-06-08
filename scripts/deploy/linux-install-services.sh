#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-/opt/reflection-king}"
ENV_DIR="/etc/reflection-king"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root on the target Linux server." >&2
  exit 1
fi

cd "${APP_DIR}"

mkdir -p "${ENV_DIR}"
if [[ ! -f "${ENV_DIR}/reflection.env" ]]; then
  cat > "${ENV_DIR}/reflection.env" <<'EOF'
RK_BIND_ADDRESS=127.0.0.1:8787
RK_PUBLIC_BASE_URL=http://154.40.36.22
RK_STORAGE_DIR=/opt/reflection-king/storage
RK_MAX_DOWNLOAD_MB=300
RK_JOB_TTL_HOURS=24
RK_MAX_CONCURRENT_JOBS=2
RK_FFMPEG_PATH=ffmpeg
RK_BROWSER_PROBE_URL=http://127.0.0.1:8791
RK_BROWSER_PROBE_TIMEOUT_SECONDS=90
RK_BROWSER_HOST=127.0.0.1
RK_BROWSER_PORT=8791
RK_BROWSER_PROFILE_ROOT=/opt/reflection-king/storage/browser-profiles
RK_BROWSER_DEFAULT_PROFILE=admin_default
RK_BROWSER_TIMEOUT_MS=45000
RK_BROWSER_MAX_EVENTS=500
RK_BROWSER_MAX_CANDIDATES=50
RK_BROWSER_HEADED=0
EOF
fi

source /root/.cargo/env || true

cd "${APP_DIR}/services/reflection-browser"
npm install
npm run build
npx playwright install chromium
npx playwright install-deps chromium

cd "${APP_DIR}"
cargo build --release --workspace

cp scripts/deploy/reflection-api.service /etc/systemd/system/reflection-api.service
cp scripts/deploy/reflection-browser.service /etc/systemd/system/reflection-browser.service
systemctl daemon-reload
systemctl enable reflection-browser reflection-api
systemctl restart reflection-browser reflection-api

echo "Services installed. Check with:"
echo "  systemctl status reflection-browser reflection-api"
