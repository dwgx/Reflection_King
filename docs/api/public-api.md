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

The job response exposes both URL forms:

- `source_url`: normalized URL used by the resolver. Bare host input such as
  `www.youtube.com/watch` is normalized to `https://www.youtube.com/watch`.
- `original_source_url`: trimmed user input before normalization. This may be
  `null` on jobs created before the field existed.

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

`outputs: ["page_html"]` requires browser discovery and produces a webpage
frontend package, not only a single HTML file. A successful job emits
`page.html`, `index.html`, `page.txt`, `screenshot.png`, `resources.json`,
`archive.zip`, and, when enabled, `archive.har`, `archive.mhtml`, and
`archive.warc`. `page.html` is the inline preview copied from the archive's
`index.inline.html`; `index.html` is the raw relative-resource entry.

The archive contains `index.html`, `index.inline.html`, downloaded
CSS/JS/images/fonts/media under `assets/`, preview files, and metadata. Remote
assets are still validated by the server URL policy and byte limits before they
are fetched. `resources.json` records URL provenance such as origin, initiator,
frame URL, redirect chain, capture source, local path, and skip reason.

Jobs that need a login-capable browser profile return `status:
needs_profile`, `issue_kind: needs_profile`, and `profile_action_url`.
Clients with a key that has `allow_login_profile=true` can open a browser
login session for the job, then resume the job:

The session uses the job's explicit `profile_id` when one is set. Jobs created
without a profile, and older jobs that used legacy `job_<id>_<actor>` profile
IDs, use the shared browser default profile. Configure that shared profile with
`RK_BROWSER_DEFAULT_PROFILE`; it defaults to `admin_default`. Cookies in that
profile are shared by every authorized key that can operate login profiles.

```text
POST /api/jobs/{id}/browser-login-session
GET  /api/jobs/{id}/browser-login-session/{session_id}/snapshot
POST /api/jobs/{id}/browser-login-session/{session_id}/click
POST /api/jobs/{id}/browser-login-session/{session_id}/move
POST /api/jobs/{id}/browser-login-session/{session_id}/mouse-down
POST /api/jobs/{id}/browser-login-session/{session_id}/mouse-up
POST /api/jobs/{id}/browser-login-session/{session_id}/type
POST /api/jobs/{id}/browser-login-session/{session_id}/insert-text
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

## `GET /api/jobs/{id}/archive/tree`

Returns the extracted webpage package tree under the job's `page/` directory.
The response lists each file path, content type, size, preview URL, and modified
time. Access uses the same job authorization rules as `GET /api/jobs/{id}`.

## `GET /api/jobs/{id}/archive/file?path=<relative-path>`

Streams one extracted archive file. The path must be a normalized relative path
inside the job's `page/` directory; absolute paths, backslashes, and `..` are
rejected. This route exists so `/media/{id}/{filename}` can stay limited to
single root artifacts. If API keys are enabled, clients must send `x-api-key`;
dashboard archive previews use authenticated fetches rather than plain links.

## `GET /media/{id}/{filename}`

Serves generated artifacts with single byte-range support.

## `GET /api/admin/cache`

Admin only. Returns storage usage grouped into public artifacts, temporary job
directories, and browser profiles. Browser profiles are reported for visibility
but are not part of the default cleanup boundary because they contain cookies
and login state.

## `POST /api/admin/cache/cleanup-preview`

Admin only. Calculates removable cache entries without deleting them.

Request:

```json
{
  "min_age_hours": 24
}
```

## `POST /api/admin/cache/cleanup`

Admin only. Deletes only cleanup-eligible cache entries and requires
`confirm: true`. The default cleanup removes old temporary job directories and
orphaned public artifact directories; it does not delete database history,
visible job records, known job artifact directories, active job temporary
directories, or browser profiles.

Request:

```json
{
  "confirm": true,
  "min_age_hours": 24
}
```
