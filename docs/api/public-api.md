# Public API

## `GET /api/health`

Returns service status.

## `GET /api/capabilities`

Returns runtime capabilities and configured limits for the dashboard.
If `RK_API_KEY` is configured, send `x-api-key`.

## `GET /api/jobs?limit=50`

Returns recent jobs, newest first. `limit` defaults to 50 and is capped at 200.
If `RK_API_KEY` is configured, send `x-api-key`.

## `POST /api/jobs`

Creates a media processing job.

Request:

```json
{
  "url": "https://example.com/audio.m4a",
  "bitrate": "192k",
  "discovery": "direct",
  "platform_hint": "auto",
  "outputs": ["audio"],
  "auth_mode": "none",
  "profile_id": "admin_default"
}
```

If `RK_API_KEY` is configured, send `x-api-key`.

Discovery modes:

- `direct`: treat `url` as a direct downloadable media URL.
- `external`: run the constrained external extractor layer. The current adapter
  uses `yt-dlp --dump-single-json --no-playlist --skip-download` to produce
  candidates only; Reflection King still validates, downloads, and transcodes.
- `browser`: run the Playwright browser sidecar.
- `auto`: try configured external discovery first, then browser discovery.

## `GET /api/jobs/{id}`

Returns job status and final media URL when ready.

Browser jobs stop at `candidates_ready` until the client selects one or more
candidate IDs.

`outputs: ["page_html"]` requires browser discovery. A successful job emits
page artifacts instead of only media: `page.html`, `page.txt`,
`screenshot.png`, `resources.json`, and `archive.zip`. The archive contains
`index.html`, downloaded `assets/`, and metadata. Remote assets are still
validated by the server URL policy and byte limits before they are fetched.

Jobs that need a login-capable browser profile return `status:
needs_profile`, `issue_kind: needs_profile`, and `profile_action_url`.
Clients with a key that has `allow_login_profile=true` can open a job-scoped
browser login session, then resume the job:

```text
POST /api/jobs/{id}/browser-login-session
GET  /api/jobs/{id}/browser-login-session/{session_id}/snapshot
POST /api/jobs/{id}/browser-login-session/{session_id}/click
POST /api/jobs/{id}/browser-login-session/{session_id}/type
POST /api/jobs/{id}/browser-login-session/{session_id}/press
POST /api/jobs/{id}/browser-login-session/{session_id}/navigate
POST /api/jobs/{id}/browser-login-session/{session_id}/wheel
POST /api/jobs/{id}/browser-login-session/{session_id}/resize
POST /api/jobs/{id}/browser-login-session/{session_id}/close
POST /api/jobs/{id}/resume-with-profile
```

## `GET /api/jobs/{id}/candidates`

Returns browser-discovered candidate URLs and metadata. Sensitive Cookie/Auth
headers are not returned.

## `POST /api/jobs/{id}/select-candidates`

Starts server-side acquisition for selected candidates.

Request:

```json
{
  "candidate_ids": ["00000000-0000-4000-8000-000000000000"]
}
```

## `GET /api/jobs/{id}/artifacts`

Returns generated files and public media URLs.

## `GET /media/{id}/{filename}`

Serves generated artifacts with single byte-range support.
