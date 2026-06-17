#!/usr/bin/env bash
set -euo pipefail

cd "${1:-$HOME/reflection-king-smoke}"

echo "== compose ps =="
sudo docker compose --env-file .env.docker ps

echo "== container fs =="
sudo docker compose --env-file .env.docker exec -T reflection-king sh -lc '
  echo "pwd=$(pwd)"
  printf "app_entries="
  ls -1 /app 2>/dev/null | head -5 | tr "\n" " "
  echo
  printf "data_entries="
  ls -1 /data 2>/dev/null | head -20 | tr "\n" " "
  echo
  if [ -s /data/admin-key.txt ]; then
    echo "admin_key_file=/data/admin-key.txt"
    wc -c /data/admin-key.txt | awk "{print \"admin_key_bytes=\" \$1}"
  else
    echo "admin_key_file=missing"
  fi
  env | cut -d= -f1 | sort | grep -E "^RK_" | tr "\n" " "
  echo
'

echo "== health =="
curl -fsS http://127.0.0.1:8780/api/health
echo
