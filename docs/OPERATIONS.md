# Operations

## Environment

See `.env.example`.

## Runtime Files

```text
storage/tmp            Temporary download inputs
storage/public         Served outputs
storage/reflection.db  SQLite job records and recovery queue
storage/browser-profiles  Playwright persistent browser profiles
```

Do not commit `storage/`.

## Health

```text
GET /api/health
```

Returns service name, version, ffmpeg path, public base URL, storage path, and
database path.

## Logs

Use `RUST_LOG`:

```powershell
$env:RUST_LOG = "reflection_api=debug,reflection_core=debug,tower_http=info"
```

## Browser Sidecar

Browser discovery requires `services/reflection-browser`.

```powershell
cd services\reflection-browser
npm install
npx playwright install chromium
npm run dev
```

Set `RK_BROWSER_PROBE_URL=http://127.0.0.1:8791` for the API. On a desktop host,
set `RK_BROWSER_HEADED=1` to manually log into the persistent
`admin_default` profile. On headless Linux, put the sidecar behind an Xvfb/noVNC
wrapper before using headed login.

## Linux Deployment

If the repository is public, the server can update without a GitHub login:

```bash
git clone https://github.com/<owner>/<repo>.git /opt/reflection-king
cd /opt/reflection-king
git pull --ff-only
```

If no public remote exists yet, upload the working tree to `/opt/reflection-king`
with `scp` or `rsync`, then run:

```bash
sudo bash scripts/deploy/linux-bootstrap.sh
sudo APP_DIR=/opt/reflection-king bash scripts/deploy/linux-install-services.sh
```

For a public HTTP deployment, pass the external base URL when installing
services:

```bash
sudo RK_PUBLIC_BASE_URL=http://your-server-or-domain \
  APP_DIR=/opt/reflection-king \
  bash scripts/deploy/linux-install-services.sh
```

The install script keeps the API and browser sidecar bound to localhost and
configures nginx on port 80 as the public reverse proxy.

Verify the deployment:

```bash
systemctl status nginx reflection-browser reflection-api
curl http://127.0.0.1:8787/api/health
curl http://127.0.0.1/api/health
```
