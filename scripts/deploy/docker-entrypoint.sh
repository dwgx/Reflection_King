#!/usr/bin/env bash
set -euo pipefail

APP_DIR="${APP_DIR:-/app}"
STORAGE_DIR="${RK_STORAGE_DIR:-/data}"
PUBLIC_PORT="${RK_PUBLIC_PORT:-8780}"
ADMIN_KEY_FILE="${RK_ADMIN_KEY_FILE:-${STORAGE_DIR}/admin-key.txt}"

mkdir -p "${STORAGE_DIR}" "${STORAGE_DIR}/browser-profiles"

if [[ -z "${RK_API_KEY:-}" ]]; then
  if [[ -f "${ADMIN_KEY_FILE}" ]]; then
    export RK_API_KEY
    RK_API_KEY="$(tr -d '\r\n' < "${ADMIN_KEY_FILE}")"
  else
    export RK_API_KEY
    RK_API_KEY="$(openssl rand -hex 32)"
    printf '%s\n' "${RK_API_KEY}" > "${ADMIN_KEY_FILE}"
    chmod 600 "${ADMIN_KEY_FILE}" || true
  fi
fi
if [[ -z "${RK_BROWSER_INTERNAL_TOKEN:-}" ]]; then
  export RK_BROWSER_INTERNAL_TOKEN
  RK_BROWSER_INTERNAL_TOKEN="$(openssl rand -hex 32)"
fi

export RK_BIND_ADDRESS="${RK_BIND_ADDRESS:-0.0.0.0:${PUBLIC_PORT}}"
export RK_PUBLIC_BASE_URL="${RK_PUBLIC_BASE_URL:-http://localhost:${PUBLIC_PORT}}"
export RK_STORAGE_DIR="${STORAGE_DIR}"
export RK_MAX_DOWNLOAD_MB="${RK_MAX_DOWNLOAD_MB:-300}"
export RK_JOB_TTL_HOURS="${RK_JOB_TTL_HOURS:-24}"
export RK_MAX_CONCURRENT_JOBS="${RK_MAX_CONCURRENT_JOBS:-2}"
export RK_FFMPEG_PATH="${RK_FFMPEG_PATH:-ffmpeg}"
export RK_BROWSER_HOST="${RK_BROWSER_HOST:-127.0.0.1}"
export RK_BROWSER_PORT="${RK_BROWSER_PORT:-8791}"
export RK_BROWSER_PROBE_URL="${RK_BROWSER_PROBE_URL:-http://127.0.0.1:8791}"
export RK_BROWSER_PROFILE_ROOT="${RK_BROWSER_PROFILE_ROOT:-${STORAGE_DIR}/browser-profiles}"
export RK_BROWSER_DEFAULT_PROFILE="${RK_BROWSER_DEFAULT_PROFILE:-admin_default}"
export RK_BROWSER_TIMEOUT_MS="${RK_BROWSER_TIMEOUT_MS:-45000}"
export RK_BROWSER_MAX_EVENTS="${RK_BROWSER_MAX_EVENTS:-500}"
export RK_BROWSER_MAX_CANDIDATES="${RK_BROWSER_MAX_CANDIDATES:-50}"
export RK_BROWSER_HEADED="${RK_BROWSER_HEADED:-0}"
export RK_YTDLP_PATH="${RK_YTDLP_PATH:-/opt/venv/bin/yt-dlp}"
export RK_YOU_GET_PATH="${RK_YOU_GET_PATH:-/opt/venv/bin/you-get}"
export RK_STREAMLINK_PATH="${RK_STREAMLINK_PATH:-/opt/venv/bin/streamlink}"
export RK_EXTERNAL_PROBE_TIMEOUT_SECONDS="${RK_EXTERNAL_PROBE_TIMEOUT_SECONDS:-45}"

echo "Reflection King starting."
echo "Dashboard: ${RK_PUBLIC_BASE_URL}"
echo "Admin key file: ${ADMIN_KEY_FILE}"
if [[ "${RK_PRINT_BOOTSTRAP_KEY:-0}" == "1" ]]; then
  echo "Admin key: ${RK_API_KEY}"
else
  echo "Admin key: [hidden; read ${ADMIN_KEY_FILE} inside the container or set RK_PRINT_BOOTSTRAP_KEY=1]"
fi

cd "${APP_DIR}/services/reflection-browser"
node dist/server.js &
BROWSER_PID="$!"

cleanup() {
  kill "${BROWSER_PID}" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

cd "${APP_DIR}"
exec /app/reflection-api
