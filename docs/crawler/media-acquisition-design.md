# Media Acquisition Design

Reflection King should grow from direct URL download into a policy-bound media
acquisition backend.

## Goals

- Discover playable media candidates from authorized pages and manifests.
- Keep discovery separate from downloading and transcoding.
- Reuse the same URL, network, and quota policy for all discovered URLs.
- Produce direct playback files owned by the Reflection King server.

## Non-Goals

- DRM removal.
- Captcha solving.
- Paywall, login-wall, or access-control bypass.
- Hidden credential reuse.
- Bulk crawling where site policy disallows automation.

## Proposed Core Modules

```text
reflection-core::source_resolver
  Classifies input URLs and chooses discovery mode.

reflection-core::extractors
  Direct URL, manifest, external tool, and browser-probe adapters.

reflection-core::candidates
  MediaCandidate and candidate scoring.

reflection-core::fetch_policy
  Host policy, robots input, quotas, per-host concurrency, and header policy.

reflection-core::probe
  ffprobe wrapper and metadata normalization.

reflection-core::capture
  Direct file download, HLS/DASH capture, segment validation.
```

## Candidate Contract

```text
MediaCandidate
  id
  job_id
  source_url
  resolved_url
  kind: direct_file | hls | dash | progressive_mp4 | unknown
  extractor
  mime_hint
  quality_label
  bitrate
  width
  height
  duration_seconds
  estimated_bytes
  requires_authorization
  score
  metadata_json
```

Candidate URLs are untrusted until the fetch policy validates them.

## Job State Machine

```text
queued
  -> resolving
  -> candidate_selected
  -> downloading | capturing
  -> probing
  -> transcoding | remuxing
  -> ready
```

Error records should include:

- `error_class`: policy_denied, network, extractor, no_candidate, too_large,
  ffprobe, transcode, storage, internal
- user-safe message
- internal diagnostic message
- retryable flag

## First API Expansion

Keep `POST /api/jobs` stable, but accept optional fields:

```json
{
  "url": "https://example.com/watch/123",
  "output": "audio_mp3",
  "bitrate": "192k",
  "discovery": "auto"
}
```

Add:

```text
GET /api/jobs/{id}/candidates
POST /api/jobs/{id}/select-candidate
```

Automatic selection should be allowed for simple direct-file and single-manifest
cases. Manual selection is useful when many formats are found.

## Browser Probe Guardrails

- Runs only when `discovery=browser` or policy permits auto fallback.
- Separate queue class and lower concurrency.
- Fixed wall-clock timeout.
- Request count and byte budgets.
- Capture only URL, method, response headers, status, and timing.
- Do not store page screenshots or sensitive response bodies by default.
- Stop at explicit access gates instead of bypassing them.

## Generic Browser Discovery

The browser sidecar must not depend only on per-platform extractor tables.
Site-specific extractors can improve scoring and reliability, but the first
layer is generic discovery for unknown authorized pages.

Generic sources:

- Network responses classified by content type, resource type, and URL path.
- DOM media elements: `video`, `audio`, and nested `source` URLs.
- Download/navigation anchors that point at media or manifest URLs.
- Open Graph and metadata fields that contain audio, video, or image URLs.
- Browser `performance.getEntriesByType("resource")` entries for runtime media
  loads.
- Inline script URL scanning for HTTP(S), root-relative, and dot-relative media
  or manifest URLs.

All discovered URLs still pass through the Rust URL policy before download or
transcode. `blob:` and `data:` URLs are not treated as downloadable candidates;
future work should record them as diagnostic references instead of pretending
they are server-fetchable.

When the caller requests audio, video and manifest candidates may still be
returned because ffmpeg can extract an audio artifact from a playable video or
manifest URL. When the caller requests video, audio-only candidates are filtered
out.

## Near-Term Coding Order

1. Add enums and structs for source kind, candidate kind, output profile, and
   error class.
2. Extend SQLite with `source_candidates` and `fetch_attempts`.
3. Refactor current direct URL path to emit one direct-file candidate.
4. Add `ffprobe` step before transcode.
5. Add HLS manifest classification and ffmpeg capture.
6. Add constrained `yt-dlp --dump-single-json` adapter.
