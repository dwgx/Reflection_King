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

Set `RK_BROWSER_PROBE_URL=http://127.0.0.1:8791` for the API. The admin
dashboard can start a server-side remote browser session for a persistent
Profile. The browser process stays inside the sidecar; the dashboard receives a
short-lived screenshot controller and never receives Cookie values.

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

## Browser Profile Login And Cookies

The primary Profile path is the admin page remote browser:

1. Open `管理 -> 浏览器账号配置`.
2. Set the target `Profile ID`, for example `admin_default`.
3. Enter the site URL and start the server browser session.
4. Click the screenshot, type through the input bar, or scan a QR code with a
   phone.
5. Close the session after login. The Profile directory keeps the site cookies
   for later browser probing and header replay.

Direct Cookie JSON import is still supported from the same admin card. Export
cookies from a browser profile and paste the JSON array into `Cookie JSON`, then
import it into the target Profile ID.

For Windows machines where the operator is already logged into Edge, Chrome, or
Firefox, the local Python importer can extract only the requested site domains
and upload them to the server profile:

```powershell
python -m pip install --user -U yt-dlp browser-cookie3
python scripts/cookies/import_browser_cookies.py `
  --base-url http://154.40.36.22:8780 `
  --api-key "<admin-key>" `
  --browser edge `
  --platform bilibili `
  --profile-id admin_default
```

Use `--dry-run` first to confirm only cookie counts and domains. Cookie values
are not printed. If Chromium reports that the cookie database cannot be copied,
close that browser and retry the default `--engine yt-dlp`. If Windows requires
shadow-copy access, run `--engine browser-cookie3` from an administrator
terminal. This is an explicit local import command, not a protocol handler.

Local protocol handlers and PowerShell desktop helpers are intentionally not
supported. They are hard to trust, fail in common browser security contexts, and
do not work reliably for a remote VPS dashboard. Do not expose browser CDP/VNC
ports directly to the public internet; keep interaction behind the API
permission checks.

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
