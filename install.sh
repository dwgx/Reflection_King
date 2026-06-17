#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${RK_REPO_URL:-https://github.com/dwgx/Reflection_King.git}"
BRANCH="${RK_BRANCH:-master}"
APP_DIR="${APP_DIR:-/opt/reflection-king}"
PUBLIC_PORT="${RK_PUBLIC_PORT:-8780}"
PUBLIC_BASE_URL="${RK_PUBLIC_BASE_URL:-}"

usage() {
  cat <<'EOF'
Reflection King VPS installer

Usage:
  curl -fsSL https://raw.githubusercontent.com/dwgx/Reflection_King/master/install.sh | sudo bash
  sudo bash install.sh --public-base-url http://1.2.3.4:8780

Options:
  --repo URL             Git repository URL. Default: https://github.com/dwgx/Reflection_King.git
  --branch NAME          Git branch. Default: master
  --app-dir PATH         Install directory. Default: /opt/reflection-king
  --public-base-url URL  Public dashboard/media base URL.
  --port PORT            Public nginx port. Default: 8780
  -h, --help             Show help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo)
      REPO_URL="$2"
      shift 2
      ;;
    --branch)
      BRANCH="$2"
      shift 2
      ;;
    --app-dir)
      APP_DIR="$2"
      shift 2
      ;;
    --public-base-url)
      PUBLIC_BASE_URL="$2"
      shift 2
      ;;
    --port)
      PUBLIC_PORT="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "${EUID}" -ne 0 ]]; then
  echo "Run as root, for example: curl -fsSL ... | sudo bash" >&2
  exit 1
fi

if ! command -v apt-get >/dev/null 2>&1; then
  echo "This installer targets Debian/Ubuntu VPS hosts with apt-get." >&2
  echo "Use Docker Compose on other Linux distributions." >&2
  exit 1
fi

if ! command -v systemctl >/dev/null 2>&1; then
  echo "This installer requires systemd/systemctl." >&2
  echo "Use Docker Compose in containers or non-systemd environments." >&2
  exit 1
fi

if [[ "${APP_DIR}" != /* ]] || [[ "${APP_DIR}" =~ [[:space:]] ]]; then
  echo "--app-dir must be an absolute path without whitespace." >&2
  exit 1
fi
APP_DIR="${APP_DIR%/}"
if [[ -z "${APP_DIR}" ]]; then
  APP_DIR="/"
fi

case "${APP_DIR}" in
  "/"|"/bin"|"/boot"|"/dev"|"/etc"|"/home"|"/lib"|"/lib64"|"/media"|"/mnt"|"/opt"|"/proc"|"/root"|"/run"|"/sbin"|"/srv"|"/sys"|"/tmp"|"/usr"|"/var"|"/var/www")
    echo "--app-dir points at a protected system or parent directory: ${APP_DIR}" >&2
    echo "Choose a dedicated application directory such as /opt/reflection-king." >&2
    exit 1
    ;;
  *"/.."*|*"../"*)
    echo "--app-dir must not contain '..' path components: ${APP_DIR}" >&2
    exit 1
    ;;
esac

if [[ -L "${APP_DIR}" ]]; then
  echo "--app-dir must not be a symlink: ${APP_DIR}" >&2
  exit 1
fi

export RK_REPO_URL="${REPO_URL}"
export RK_BRANCH="${BRANCH}"
export APP_DIR
export RK_PUBLIC_PORT="${PUBLIC_PORT}"
if [[ -n "${PUBLIC_BASE_URL}" ]]; then
  export RK_PUBLIC_BASE_URL="${PUBLIC_BASE_URL}"
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

apt-get update
apt-get install -y ca-certificates curl git

if [[ -d "${APP_DIR}/.git" ]]; then
  git -C "${APP_DIR}" fetch origin "${BRANCH}"
  if ! git -C "${APP_DIR}" diff --quiet || ! git -C "${APP_DIR}" diff --cached --quiet; then
    echo "--app-dir has local changes: ${APP_DIR}" >&2
    echo "Commit, stash, or remove those changes before rerunning the installer." >&2
    exit 1
  fi
  git -C "${APP_DIR}" merge --ff-only "origin/${BRANCH}"
else
  if [[ -e "${APP_DIR}" ]]; then
    if [[ ! -d "${APP_DIR}" ]]; then
      echo "--app-dir exists but is not a directory: ${APP_DIR}" >&2
      exit 1
    fi
    if [[ -n "$(find "${APP_DIR}" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
      echo "--app-dir exists, is not a Git checkout, and is not empty: ${APP_DIR}" >&2
      echo "Refusing to delete it. Move it aside or choose an empty dedicated directory." >&2
      exit 1
    fi
  else
    mkdir -p "$(dirname "${APP_DIR}")"
  fi
  git clone --branch "${BRANCH}" "${REPO_URL}" "${APP_DIR}"
fi

bash "${APP_DIR}/scripts/deploy/install-vps.sh"
