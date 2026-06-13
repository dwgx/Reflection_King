#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${RK_REPO_URL:-https://github.com/dwgx/Reflection_King.git}"
BRANCH="${RK_BRANCH:-master}"
APP_DIR="${APP_DIR:-/opt/reflection-king}"
PUBLIC_PORT="${RK_PUBLIC_PORT:-8780}"
PUBLIC_BASE_URL="${RK_PUBLIC_BASE_URL:-}"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root: sudo bash scripts/deploy/install-vps.sh" >&2
  exit 1
fi

apt-get update
apt-get install -y ca-certificates curl git openssl

if [[ -z "${PUBLIC_BASE_URL}" ]]; then
  PUBLIC_IP="$(curl -fsS --max-time 5 https://api.ipify.org || true)"
  if [[ -n "${PUBLIC_IP}" ]]; then
    PUBLIC_BASE_URL="http://${PUBLIC_IP}:${PUBLIC_PORT}"
  else
    PUBLIC_BASE_URL="http://localhost:${PUBLIC_PORT}"
  fi
fi

if [[ -d "${APP_DIR}/.git" ]]; then
  git -C "${APP_DIR}" fetch origin "${BRANCH}"
  git -C "${APP_DIR}" reset --hard "origin/${BRANCH}"
else
  rm -rf "${APP_DIR}"
  git clone --branch "${BRANCH}" "${REPO_URL}" "${APP_DIR}"
fi

cd "${APP_DIR}"

bash scripts/deploy/linux-bootstrap.sh
RK_PUBLIC_BASE_URL="${PUBLIC_BASE_URL}" \
RK_PUBLIC_PORT="${PUBLIC_PORT}" \
APP_DIR="${APP_DIR}" \
bash scripts/deploy/linux-install-services.sh

echo
echo "One-command install complete."
echo "Open: ${PUBLIC_BASE_URL}"
