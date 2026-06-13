#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-/opt/reflection-king}"
ENV_DIR="/etc/reflection-king"
YTDLP_VENV="${APP_DIR}/storage/yt-dlp-venv"
PHANTOMJS_DIR="${APP_DIR}/storage/phantomjs"
PUBLIC_PORT="${RK_PUBLIC_PORT:-8780}"
ADMIN_KEY_FILE="${RK_ADMIN_KEY_FILE:-/root/reflection-king-admin-key.txt}"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root on the target Linux server." >&2
  exit 1
fi

cd "${APP_DIR}"

if ! python3 -m venv --help >/dev/null 2>&1; then
  apt-get update
  apt-get install -y python3-venv python3-pip
fi

mkdir -p "${ENV_DIR}"
if [[ ! -f "${ENV_DIR}/reflection.env" ]]; then
  PUBLIC_BASE_URL="${RK_PUBLIC_BASE_URL:-http://localhost:${PUBLIC_PORT}}"
  API_KEY="${RK_API_KEY:-$(openssl rand -hex 32)}"
  cat > "${ENV_DIR}/reflection.env" <<EOF
RK_BIND_ADDRESS=127.0.0.1:8787
RK_PUBLIC_BASE_URL=${PUBLIC_BASE_URL}
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
RK_YTDLP_PATH=${YTDLP_VENV}/bin/yt-dlp
RK_YTDLP_TIMEOUT_SECONDS=45
RK_YTDLP_MAX_JSON_MB=8
RK_EXTERNAL_PROBE_TIMEOUT_SECONDS=45
RK_API_KEY=${API_KEY}
EOF
  chmod 600 "${ENV_DIR}/reflection.env"
else
  API_KEY="$(grep -E '^RK_API_KEY=' "${ENV_DIR}/reflection.env" | tail -n 1 | cut -d= -f2- || true)"
  PUBLIC_BASE_URL="$(grep -E '^RK_PUBLIC_BASE_URL=' "${ENV_DIR}/reflection.env" | tail -n 1 | cut -d= -f2- || true)"
  PUBLIC_BASE_URL="${PUBLIC_BASE_URL:-${RK_PUBLIC_BASE_URL:-http://localhost:${PUBLIC_PORT}}}"
fi

if [[ -n "${API_KEY}" ]]; then
  install -m 600 /dev/null "${ADMIN_KEY_FILE}"
  printf '%s\n' "${API_KEY}" > "${ADMIN_KEY_FILE}"
fi

source /root/.cargo/env || true

cd "${APP_DIR}/services/reflection-browser"
npm install
npm run build
npx playwright install chromium
npx playwright install-deps chromium

cd "${APP_DIR}/apps/reflection-dashboard"
npm install
npm run build

if [[ ! -x "${YTDLP_VENV}/bin/yt-dlp" ]]; then
  python3 -m venv "${YTDLP_VENV}"
  "${YTDLP_VENV}/bin/python" -m pip install --upgrade pip
fi
"${YTDLP_VENV}/bin/python" -m pip install --upgrade yt-dlp
"${YTDLP_VENV}/bin/python" -m pip install --upgrade you-get streamlink || true
if ! grep -q '^RK_YTDLP_PATH=' "${ENV_DIR}/reflection.env"; then
  printf '\nRK_YTDLP_PATH=%s\n' "${YTDLP_VENV}/bin/yt-dlp" >> "${ENV_DIR}/reflection.env"
fi
if [[ -x "${YTDLP_VENV}/bin/you-get" ]] && ! grep -q '^RK_YOU_GET_PATH=' "${ENV_DIR}/reflection.env"; then
  printf 'RK_YOU_GET_PATH=%s\n' "${YTDLP_VENV}/bin/you-get" >> "${ENV_DIR}/reflection.env"
fi
if [[ -x "${YTDLP_VENV}/bin/streamlink" ]] && ! grep -q '^RK_STREAMLINK_PATH=' "${ENV_DIR}/reflection.env"; then
  printf 'RK_STREAMLINK_PATH=%s\n' "${YTDLP_VENV}/bin/streamlink" >> "${ENV_DIR}/reflection.env"
fi
if ! command -v phantomjs >/dev/null 2>&1; then
  mkdir -p "${PHANTOMJS_DIR}"
  cd "${PHANTOMJS_DIR}"
  npm init -y >/dev/null 2>&1 || true
  if npm install phantomjs-prebuilt@2.1.16 --omit=dev; then
    ln -sfn "${PHANTOMJS_DIR}/node_modules/.bin/phantomjs" /usr/local/bin/phantomjs
  else
    echo "Warning: PhantomJS install failed; iQIYI extraction may remain unavailable." >&2
  fi
fi

cd "${APP_DIR}"
cargo build --release --workspace

cp scripts/deploy/reflection-api.service /etc/systemd/system/reflection-api.service
cp scripts/deploy/reflection-browser.service /etc/systemd/system/reflection-browser.service

cat > /etc/nginx/sites-available/reflection-king <<EOF
server {
    listen ${PUBLIC_PORT};
    server_name _;

    client_max_body_size 10m;

    location / {
        proxy_pass http://127.0.0.1:8787;
        proxy_http_version 1.1;
        proxy_set_header Host \$host;
        proxy_set_header X-Real-IP \$remote_addr;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto \$scheme;
    }
}
EOF
ln -sfn /etc/nginx/sites-available/reflection-king /etc/nginx/sites-enabled/reflection-king
rm -f /etc/nginx/sites-enabled/default
nginx -t

systemctl daemon-reload
systemctl enable nginx reflection-browser reflection-api
systemctl restart nginx reflection-browser reflection-api

echo
echo "Reflection King installed."
echo "Dashboard: ${PUBLIC_BASE_URL}"
echo "Admin key: ${API_KEY}"
echo "Admin key file: ${ADMIN_KEY_FILE}"
echo
echo "Check services:"
echo "  systemctl status nginx reflection-browser reflection-api"
