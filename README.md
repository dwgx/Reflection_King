# Reflection King

Reflection King is a Rust backend for media acquisition, processing, and delivery.

The implemented slice has two paths:

- Direct media URL to MP3 for VRChat-style players.
- External extractor page URL to media candidates through constrained `yt-dlp`
  metadata discovery.
- Browser-probed page URL to media candidates, followed by explicit candidate
  selection and server-side artifact generation.

1. Accept a remote direct media URL.
2. Validate the URL and block private network targets.
3. Persist the job in a local SQLite queue.
4. Download the source with size limits.
5. Transcode audio to MP3 with `ffmpeg`.
6. Serve a public direct `audio/mpeg` URL under `/media/...` with HTTP Range support.

The browser path is intended for authorized Bilibili, YouTube, SoundCloud,
Douyin, Kuaishou, Pornhub, AcFun, iQIYI, Youku, TikTok, Vimeo, and similar
public page workflows. It does not bypass DRM, captchas, paywalls, login walls,
or access controls.

## Layout

```text
crates/reflection-core     Shared config, models, job store, URL safety, download, transcode
crates/reflection-api      Axum HTTP API, persistent queue dispatch, and media serving
crates/reflection-worker   Future standalone worker entrypoint
services/reflection-browser Playwright sidecar for browser-based candidate discovery
docs/                      Architecture, operations, media pipeline, security
config/                    Example runtime policy/config files
scripts/                   Local maintenance and verification scripts
tests/                     Integration test notes and future fixtures
```

## Local Requirements

- Rust stable toolchain
- Visual Studio C++ Build Tools on Windows for the default MSVC Rust target
- `ffmpeg` on `PATH`, or set `RK_FFMPEG_PATH`
- `yt-dlp` on `PATH`, or set `RK_YTDLP_PATH`, for external discovery
- Optional external probes: `you-get` (`RK_YOU_GET_PATH`), `lux`
  (`RK_LUX_PATH`), and `streamlink` (`RK_STREAMLINK_PATH`)
- Node.js for the Playwright browser sidecar

Run `.\scripts\dev\bootstrap.ps1` on a fresh Windows machine to install Rust,
the MSVC build tools, FFmpeg, Node dependencies, and Playwright Chromium.

Runtime job state is stored in `storage/reflection.db` by default. The API will
recover queued or interrupted jobs when it starts.

## Run

```powershell
cd D:\Project\Reflection_King
copy .env.example .env
.\scripts\dev\run-local.ps1
```

Open:

```text
http://localhost:8787
```

The root page serves the Reflection Dashboard after `apps/reflection-dashboard`
has been built. The dashboard can create jobs, inspect recent jobs, submit
browser-discovered candidates, and open generated media artifacts.

Create a job:

```powershell
$body = @{
  url = "https://example.com/audio.m4a"
  bitrate = "192k"
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri "http://localhost:8787/api/jobs" `
  -ContentType "application/json" `
  -Body $body
```

Poll the returned `status_url`. When the job is `ready`, paste `media_url` into the VRC player.

With `discovery = "auto"`, the resolver aggregates every configured route that
is allowed by the current key: direct URL detection, yt-dlp, optional external
adapters, and browser probing. Candidates are deduplicated and scored with
quality, output type, signature/protection hints, ad risk, route confidence, and
basic validation state. The dashboard shows the recommended resource first, but
still lets an operator inspect or override the selection.

Create an external extractor job:

```powershell
$body = @{
  url = "https://www.youtube.com/watch?v=..."
  discovery = "external"
  platform_hint = "youtube"
  outputs = @("audio")
} | ConvertTo-Json

$job = Invoke-RestMethod `
  -Method Post `
  -Uri "http://localhost:8787/api/jobs" `
  -ContentType "application/json" `
  -Body $body
```

Create a browser-probed job:

```powershell
$body = @{
  url = "https://www.bilibili.com/video/BV..."
  discovery = "browser"
  platform_hint = "bilibili"
  outputs = @("audio", "video")
  auth_mode = "profile"
  profile_id = "admin_default"
} | ConvertTo-Json

$job = Invoke-RestMethod `
  -Method Post `
  -Uri "http://localhost:8787/api/jobs" `
  -ContentType "application/json" `
  -Body $body
```

Poll until the job status is `candidates_ready`, then inspect:

```powershell
Invoke-RestMethod "http://localhost:8787/api/jobs/$($job.id)/candidates"
```

Select candidate IDs:

```powershell
$selection = @{
  candidate_ids = @("candidate-uuid-1", "candidate-uuid-2")
} | ConvertTo-Json

Invoke-RestMethod `
  -Method Post `
  -Uri "http://localhost:8787/api/jobs/$($job.id)/select-candidates" `
  -ContentType "application/json" `
  -Body $selection
```

Browser Profile cookies can be imported from the admin page by pasting Cookie
JSON exported from a browser profile. The removed local protocol/PowerShell
helper path is intentionally unsupported; future interactive login should run as
a server-side remote browser session behind HTTPS.

## Public URLs

VRChat clients cannot access your local `localhost`. Use a VPS, reverse proxy, or HTTPS tunnel. Set:

```powershell
$env:RK_PUBLIC_BASE_URL = "https://your-public-domain.example"
cargo run -p reflection-api
```

Generated media URLs will use that public base.

## Safety

Use this backend only for media you own or have permission to use. The current code includes baseline SSRF blocking, download size limits, optional API key protection, and simple storage expiry planning. See [docs/SECURITY.md](docs/SECURITY.md).
