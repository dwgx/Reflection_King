#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-/opt/reflection-king}"
ENV_DIR="/etc/reflection-king"
ENV_FILE="${ENV_DIR}/reflection.env"
YTDLP_VENV="${APP_DIR}/storage/yt-dlp-venv"
PHANTOMJS_DIR="${APP_DIR}/storage/phantomjs"
PUBLIC_PORT="${RK_PUBLIC_PORT:-8780}"
ADMIN_KEY_FILE="${RK_ADMIN_KEY_FILE:-/root/reflection-king-admin-key.txt}"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root on the target Linux server." >&2
  exit 1
fi

if [[ "${APP_DIR}" != /* ]] || [[ "${APP_DIR}" =~ [[:space:]] ]]; then
  echo "APP_DIR must be an absolute path without whitespace." >&2
  exit 1
fi

cd "${APP_DIR}"

if ! python3 -m venv --help >/dev/null 2>&1; then
  apt-get update
  apt-get install -y python3-venv python3-pip
fi

mkdir -p "${ENV_DIR}"
set_env_var() {
  local key="$1"
  local value="$2"
  local tmp_file
  tmp_file="$(mktemp)"
  awk -v key="${key}" -v value="${value}" '
    BEGIN { updated = 0 }
    $0 ~ "^" key "=" {
      print key "=" value
      updated = 1
      next
    }
    { print }
    END {
      if (updated == 0) {
        print key "=" value
      }
    }
  ' "${ENV_FILE}" > "${tmp_file}"
  cat "${tmp_file}" > "${ENV_FILE}"
  rm -f "${tmp_file}"
}

if [[ ! -f "${ENV_FILE}" ]]; then
  PUBLIC_BASE_URL="${RK_PUBLIC_BASE_URL:-http://localhost:${PUBLIC_PORT}}"
  API_KEY="${RK_API_KEY:-$(openssl rand -hex 32)}"
  BROWSER_INTERNAL_TOKEN="${RK_BROWSER_INTERNAL_TOKEN:-$(openssl rand -hex 32)}"
  cat > "${ENV_FILE}" <<EOF
RK_BIND_ADDRESS=127.0.0.1:8787
RK_PUBLIC_BASE_URL=${PUBLIC_BASE_URL}
RK_STORAGE_DIR=${APP_DIR}/storage
RK_MAX_DOWNLOAD_MB=300
RK_JOB_TTL_HOURS=24
RK_MAX_CONCURRENT_JOBS=2
RK_FFMPEG_PATH=ffmpeg
RK_BROWSER_PROBE_URL=http://127.0.0.1:8791
RK_BROWSER_INTERNAL_TOKEN=${BROWSER_INTERNAL_TOKEN}
RK_BROWSER_PROBE_TIMEOUT_SECONDS=90
RK_BROWSER_HOST=127.0.0.1
RK_BROWSER_PORT=8791
RK_BROWSER_PROFILE_ROOT=${APP_DIR}/storage/browser-profiles
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
  chmod 600 "${ENV_FILE}"
else
  API_KEY="$(grep -E '^RK_API_KEY=' "${ENV_FILE}" | tail -n 1 | cut -d= -f2- || true)"
  BROWSER_INTERNAL_TOKEN="$(grep -E '^RK_BROWSER_INTERNAL_TOKEN=' "${ENV_FILE}" | tail -n 1 | cut -d= -f2- || true)"
  PUBLIC_BASE_URL="$(grep -E '^RK_PUBLIC_BASE_URL=' "${ENV_FILE}" | tail -n 1 | cut -d= -f2- || true)"
  PUBLIC_BASE_URL="${PUBLIC_BASE_URL:-${RK_PUBLIC_BASE_URL:-http://localhost:${PUBLIC_PORT}}}"
fi

if [[ -n "${RK_PUBLIC_BASE_URL:-}" ]]; then
  PUBLIC_BASE_URL="${RK_PUBLIC_BASE_URL}"
fi
if [[ -z "${API_KEY:-}" ]]; then
  API_KEY="${RK_API_KEY:-$(openssl rand -hex 32)}"
fi
if [[ -z "${BROWSER_INTERNAL_TOKEN:-}" ]]; then
  BROWSER_INTERNAL_TOKEN="${RK_BROWSER_INTERNAL_TOKEN:-$(openssl rand -hex 32)}"
fi
set_env_var "RK_PUBLIC_BASE_URL" "${PUBLIC_BASE_URL}"
set_env_var "RK_STORAGE_DIR" "${APP_DIR}/storage"
set_env_var "RK_BROWSER_PROFILE_ROOT" "${APP_DIR}/storage/browser-profiles"
set_env_var "RK_BROWSER_INTERNAL_TOKEN" "${BROWSER_INTERNAL_TOKEN}"
set_env_var "RK_API_KEY" "${API_KEY}"
chmod 600 "${ENV_FILE}"

if [[ -n "${API_KEY}" ]]; then
  install -m 600 /dev/null "${ADMIN_KEY_FILE}"
  printf '%s\n' "${API_KEY}" > "${ADMIN_KEY_FILE}"
fi

source /root/.cargo/env || true

install_node_deps() {
  if [[ -f package-lock.json ]]; then
    npm ci
  else
    npm install
  fi
}

ensure_admin_key_usable() {
  local auth_check_url="http://127.0.0.1:8787/api/jobs?limit=1"
  if [[ -z "${API_KEY:-}" ]]; then
    return 0
  fi

  if curl -fsS -H "x-api-key: ${API_KEY}" "${auth_check_url}" >/dev/null 2>&1; then
    return 0
  fi

  RK_ADMIN_SECRET="${API_KEY}" RK_DB_PATH="${APP_DIR}/storage/reflection.db" python3 - <<'PY'
import datetime
import hashlib
import os
import sqlite3
import uuid

secret = os.environ["RK_ADMIN_SECRET"].strip()
db_path = os.environ["RK_DB_PATH"]
if not secret:
    raise SystemExit(0)

now = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
key_hash = hashlib.sha256(secret.encode("utf-8")).hexdigest()
key_prefix = secret[:16]

with sqlite3.connect(db_path, timeout=30) as conn:
    conn.execute(
        """
        INSERT INTO api_keys (
            id,
            label,
            key_hash,
            key_prefix,
            role,
            allow_browser_probe,
            allow_ytdlp,
            allow_external_adapters,
            allow_login_profile,
            created_at,
            revoked_at
        )
        VALUES (?, ?, ?, ?, 'admin', 1, 1, 1, 1, ?, NULL)
        ON CONFLICT(key_hash) DO UPDATE SET
            role = 'admin',
            allow_browser_probe = 1,
            allow_ytdlp = 1,
            allow_external_adapters = 1,
            allow_login_profile = 1,
            revoked_at = NULL
        """,
        (str(uuid.uuid4()), "Admin key", key_hash, key_prefix, now),
    )
PY

  if ! curl -fsS -H "x-api-key: ${API_KEY}" "${auth_check_url}" >/dev/null 2>&1; then
    echo "Admin key file was written, but the API still rejects that key." >&2
    exit 1
  fi
}

cd "${APP_DIR}/services/reflection-browser"
install_node_deps
npm run build
npx playwright install chromium
npx playwright install-deps chromium

cd "${APP_DIR}/apps/reflection-dashboard"
install_node_deps
npm run build

if [[ ! -x "${YTDLP_VENV}/bin/yt-dlp" ]]; then
  python3 -m venv "${YTDLP_VENV}"
  "${YTDLP_VENV}/bin/python" -m pip install --upgrade pip
fi
"${YTDLP_VENV}/bin/python" -m pip install --upgrade yt-dlp
"${YTDLP_VENV}/bin/python" -m pip install --upgrade you-get streamlink || true
if ! grep -q '^RK_YTDLP_PATH=' "${ENV_FILE}"; then
  printf '\nRK_YTDLP_PATH=%s\n' "${YTDLP_VENV}/bin/yt-dlp" >> "${ENV_FILE}"
fi
if [[ -x "${YTDLP_VENV}/bin/you-get" ]] && ! grep -q '^RK_YOU_GET_PATH=' "${ENV_FILE}"; then
  printf 'RK_YOU_GET_PATH=%s\n' "${YTDLP_VENV}/bin/you-get" >> "${ENV_FILE}"
fi
if [[ -x "${YTDLP_VENV}/bin/streamlink" ]] && ! grep -q '^RK_STREAMLINK_PATH=' "${ENV_FILE}"; then
  printf 'RK_STREAMLINK_PATH=%s\n' "${YTDLP_VENV}/bin/streamlink" >> "${ENV_FILE}"
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

cat > /etc/systemd/system/reflection-api.service <<EOF
[Unit]
Description=Reflection King API
After=network.target reflection-browser.service
Wants=reflection-browser.service

[Service]
Type=simple
WorkingDirectory=${APP_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${APP_DIR}/target/release/reflection-api
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

cat > /etc/systemd/system/reflection-browser.service <<EOF
[Unit]
Description=Reflection King Browser Sidecar
After=network.target

[Service]
Type=simple
WorkingDirectory=${APP_DIR}/services/reflection-browser
EnvironmentFile=${ENV_FILE}
ExecStart=/usr/bin/npm run start
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

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

HEALTH_URL="http://127.0.0.1:8787/api/health"
for _attempt in $(seq 1 30); do
  if curl -fsS "${HEALTH_URL}" >/dev/null 2>&1; then
    HEALTH_OK=1
    break
  fi
  sleep 1
done

if [[ "${HEALTH_OK:-0}" != "1" ]]; then
  echo "Reflection King services started, but API health check did not become ready in time." >&2
  echo "Check logs with: journalctl -u reflection-api -n 100 --no-pager" >&2
  exit 1
fi

ensure_admin_key_usable

echo
echo "Reflection King installed."
echo "Dashboard: ${PUBLIC_BASE_URL}"
echo "Admin key file: ${ADMIN_KEY_FILE}"
if [[ "${RK_PRINT_BOOTSTRAP_KEY:-0}" == "1" ]]; then
  echo "Admin key: ${API_KEY}"
else
  echo "Admin key: [hidden; read ${ADMIN_KEY_FILE} on the server or rerun with RK_PRINT_BOOTSTRAP_KEY=1]"
fi
echo
echo "Check services:"
echo "  systemctl status nginx reflection-browser reflection-api"
echo "Health check: ${HEALTH_URL}"
