#!/usr/bin/env bash
set -euo pipefail

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root on the target Linux server." >&2
  exit 1
fi

if ! command -v apt-get >/dev/null 2>&1; then
  echo "linux-bootstrap.sh targets Debian/Ubuntu hosts with apt-get." >&2
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
  python3 \
  python3-pip \
  python3-venv \
  pkg-config \
  sqlite3

NODE_MAJOR="0"
if command -v node >/dev/null 2>&1; then
  NODE_MAJOR="$(node -p "process.versions.node.split('.')[0]" 2>/dev/null || echo 0)"
fi

if [[ "${NODE_MAJOR}" -lt 20 ]] || ! command -v npm >/dev/null 2>&1; then
  curl -fsSL https://deb.nodesource.com/setup_22.x | bash -
  apt-get install -y nodejs
fi

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

echo "Bootstrap complete. Clone or update the public repository, then run linux-install-services.sh."
