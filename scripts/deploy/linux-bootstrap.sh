#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root on the target Linux server." >&2
  exit 1
fi

apt-get update
apt-get install -y \
  build-essential \
  ca-certificates \
  curl \
  ffmpeg \
  git \
  nginx \
  nodejs \
  npm \
  pkg-config \
  sqlite3

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

echo "Bootstrap complete. Clone or update the public repository, then run linux-install-services.sh."
