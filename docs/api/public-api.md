# Public API

## `GET /api/health`

Returns service status.

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

## `GET /api/jobs/{id}`

Returns job status and final media URL when ready.

Browser jobs stop at `candidates_ready` until the client selects one or more
candidate IDs.

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
