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

## External Adapters

The API can aggregate candidates from multiple adapter routes when a job uses
`discovery=auto` or `discovery=external`:

```text
RK_YTDLP_PATH=yt-dlp
RK_YOU_GET_PATH=you-get
RK_LUX_PATH=lux
RK_STREAMLINK_PATH=streamlink
RK_EXTERNAL_PROBE_TIMEOUT_SECONDS=45
```

`yt-dlp` remains the broadest first external adapter. `you-get` is useful for
some Chinese video sites, `streamlink` is useful for live/manifest workflows,
and `lux` can be configured manually when its Go binary is installed. The
resolver deduplicates URLs across routes and records protection, route,
confidence, ad-risk, and validation hints for the dashboard.

## Browser Profile Cookies

The supported Profile path is direct Cookie JSON import from the admin page.
Export cookies from a browser profile and paste the JSON array into
`管理 -> 浏览器账号配置 -> Cookie JSON`, then import it into the target Profile
ID.

Local protocol handlers and PowerShell desktop helpers are intentionally not
supported. They are hard to trust, fail in common browser security contexts, and
do not work reliably for a remote VPS dashboard. Future interactive login should
be implemented as a server-side remote browser session behind HTTPS, such as a
noVNC/CDP gateway bound to admin-only access.

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
configures nginx on the requested public port as the reverse proxy.

Verify the deployment:

```bash
systemctl status nginx reflection-browser reflection-api
curl http://127.0.0.1:8787/api/health
curl http://127.0.0.1/api/health
```
