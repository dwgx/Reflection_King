#!/usr/bin/env bash
set -euo pipefail

REPO_DIR="${REPO_DIR:-$HOME/reflection-king-smoke}"
SMOKE_SCRIPT="${SMOKE_SCRIPT:-/tmp/rk-live-smoke-2026-06-17.py}"
CATALOG="${CATALOG:-/tmp/rk-public-media-2026-06-17.json}"
SUMMARY_FILE="${SUMMARY_FILE:-/tmp/rk-public-catalog-summary-2026-06-17.json}"
BASE_URL="${BASE_URL:-http://127.0.0.1:8780}"

if [[ $# -gt 0 && "${1}" != --* ]]; then
  REPO_DIR="${1}"
  shift
fi

cd "${REPO_DIR}"

key_file="$(mktemp /tmp/rk-api-key.XXXXXX)"
chmod 600 "${key_file}"
cleanup() {
  rm -f "${key_file}"
}
trap cleanup EXIT

sudo docker compose --env-file .env.docker exec -T reflection-king sh -lc 'cat /data/admin-key.txt' > "${key_file}"
key_bytes="$(wc -c < "${key_file}" | tr -d ' ')"
echo "api_key_bytes=${key_bytes}"
if [[ "${key_bytes}" -lt 16 ]]; then
  echo "container admin key file is missing or empty" >&2
  exit 2
fi

python3 "${SMOKE_SCRIPT}" \
  --base-url "${BASE_URL}" \
  --api-key-file "${key_file}" \
  --catalog "${CATALOG}" \
  --summary-file "${SUMMARY_FILE}" \
  "$@"
