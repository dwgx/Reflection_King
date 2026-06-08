# Crawler Backend Survey 2026-06-09

## Scope

This survey looks for backend patterns that can help Reflection King discover
authorized media resources, download them safely, transcode them, and serve
direct playback URLs.

Out of scope:

- Bypassing DRM, paywalls, captchas, login walls, or access controls.
- Evading platform enforcement systems.
- Reusing private cookies or credentials without explicit user authorization.
- High-volume scraping against sites that disallow automated access.

The useful target is a policy-bound media acquisition backend, not an
anti-protection bypass tool.

## Useful Reference Projects And Docs

| Source | What To Reuse | Notes |
| --- | --- | --- |
| [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) | Extractor interface, format candidates, downloader selection, post-processing chain | Strong model for site-specific extraction and ffmpeg handoff. |
| [`yt-dlp` extractors wiki](https://github.com/yt-dlp/yt-dlp/wiki/Extractors) | Auth-sensitive extractor notes and site-specific caveats | Useful reminder that some sources require externally supplied authorization. |
| [`Streamlink`](https://streamlink.github.io/) | Plugin model for turning page or stream URLs into playable stream variants | Useful for live/HLS workflows. |
| [`gallery-dl`](https://github.com/mikf/gallery-dl) | Extractor naming, config-driven auth/cookies/proxy behavior, archive dedupe | Useful for multi-site extractor organization. |
| [Scrapy architecture](https://doc.scrapy.org/en/latest/topics/architecture.html) | Engine, scheduler, downloader middleware, spider middleware separation | Good reference for queue and middleware boundaries. |
| [Scrapy scheduler](https://doc.scrapy.org/en/latest/topics/scheduler.html) | Persistent/non-persistent request queues | Good reference for future lease and retry work. |
| [Scrapy downloader middleware](https://docs.scrapy.org/en/latest/topics/downloader-middleware.html) | Request/response hook chain | Good model for policy, headers, retry, and stats middleware. |
| [Crawlee request storage](https://crawlee.dev/js/docs/3.12/guides/request-storage) | Disk-backed request queue pattern | Useful for durable crawl state. |
| [Crawlee session management](https://crawlee.dev/js/docs/3.13/guides/session-management) | Session pools, cookies, and blocked-session classification | Useful as a policy-bound concept, not as a bypass mandate. |
| [Crawlee autoscaling](https://crawlee.dev/js/api/3.0/core/class/AutoscaledPool) | Resource-aware concurrency | Good reference for browser probe queue limits. |
| [Playwright network docs](https://playwright.dev/docs/network) | Network request/response observation in real browsers | Useful for page probes that expose manifest/media URLs after JS execution. |
| [Playwright request API](https://playwright.dev/docs/api/class-request) | Request/response event sequence | Useful for capturing candidate URLs without storing response bodies. |
| [RFC 9309](https://www.rfc-editor.org/rfc/rfc9309.html) | Robots Exclusion Protocol | Baseline crawl-policy input, not a permission grant by itself. |
| [FFmpeg documentation](https://www.ffmpeg.org/documentation.html) | Command tools, formats, codecs, protocols | Ground truth for capture and transcode behavior. |
| [FFmpeg protocols](https://www.ffmpeg.org/ffmpeg-protocols.html) | HLS and nested protocol handling | Relevant for manifest capture. |
| [ffprobe documentation](https://ffmpeg.org/ffprobe.html) | Machine-readable media metadata | Useful before transcode/remux decisions. |
| [MDN Range requests](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Range) | HTTP byte-range behavior | Relevant for delivery and player seeking. |

## Common Architecture Pattern

Mature crawlers and media downloaders separate these responsibilities:

```text
input URL
  -> policy precheck
  -> source resolver
  -> extractor or browser probe
  -> media candidate list
  -> candidate policy validation
  -> downloader or segment capture
  -> probe metadata
  -> transcode/remux
  -> delivery URL
```

The important design point is that media discovery is separate from media
fetching. Every candidate URL discovered by an extractor or browser session
must pass the same SSRF, host, scheme, and size policy checks as a user-supplied
direct URL.

## Extractor Layer

Reflection King should model extractors as small adapters:

```text
trait SourceExtractor {
  matches(input_url, policy_context) -> bool
  extract(input_url, job_context) -> ExtractResult
}
```

`ExtractResult` should be data only:

- canonical source URL
- title and metadata if available
- media candidates
- subtitles/thumbnails if available
- required headers if allowed by policy
- warnings and confidence

Candidate fields:

- URL
- type: direct_file, hls, dash, progressive_mp4, unknown
- MIME hint
- bitrate/height/audio-only flags
- estimated size/duration
- whether it requires cookies or authorization
- origin extractor name

Do not let extractors directly write files or spawn ffmpeg. They should only
produce candidates.

## Discovery Modes

### Direct URL

The current implementation covers this mode. Keep it as the safest fast path.

### Manifest URL

If the URL is `.m3u8` or `.mpd`, treat it as a manifest source. The backend
should:

- validate the manifest URL
- download the manifest with limits
- validate segment URLs after resolution
- cap segment count, duration, and total bytes
- hand off to ffmpeg or a segment fetcher

### External Extractor

Use an external tool such as `yt-dlp` as a constrained child process when a site
needs site-specific parsing. Recommended shape:

```text
yt-dlp --dump-single-json --no-playlist --skip-download <url>
```

Then parse JSON and select candidates locally. Avoid letting the external tool
download directly until the policy layer can inspect the chosen URL.

### Browser Probe

Use Playwright or another browser automation layer only when static extraction
fails and the user has authorization. The probe should:

- block third-party downloads that are not needed for page evaluation
- record media, manifest, and XHR responses
- stop after time, bytes, and request-count budgets
- return candidate URLs, not downloaded files
- never solve captchas or bypass explicit access gates

Browser probing is expensive and should run in a separate queue class.

## Queue And Worker Model

Borrow these ideas from Scrapy/Crawlee:

- Jobs are persistent records.
- Fetch attempts are separate from jobs.
- Retries are classified by cause.
- Concurrency is per domain and per queue class.
- Session state is explicit and policy-bound.
- Downloader middleware is a chain, not scattered conditionals.

Recommended new tables:

```text
jobs
  id, status, source_url, source_kind, selected_candidate_id, ...

source_candidates
  id, job_id, extractor, url, media_kind, mime_hint, score, metadata_json, ...

fetch_attempts
  id, job_id, candidate_id, status, started_at, ended_at, error_class, bytes_read, ...

domain_policies
  host, allow_mode, max_concurrency, crawl_delay_ms, requires_user_auth, ...
```

The current SQLite job store is a useful base, but standalone workers need a
lease/claim protocol before multiple consumers are enabled.

## Safety And Compliance Rules

- Treat robots.txt and site terms as policy inputs.
- Require explicit user authorization for non-public sources.
- Keep cookies and tokens out of logs and job views.
- Store credentials separately from source metadata.
- Re-validate DNS and resolved IPs on every redirect and every segment URL.
- Block local/private/link-local/cloud-metadata networks.
- Cap bytes, duration, file count, redirect count, manifest size, and browser
  request count.
- Prefer transparent user-visible errors over silent retries.
- Do not add captcha solving, DRM removal, token guessing, or rate-limit evasion.

## Implementation Priority

1. Add `source_resolver` and `MediaCandidate` models.
2. Extend SQLite schema for candidates and attempts.
3. Add direct-file candidate selection using the existing downloader.
4. Add `ffprobe` metadata before transcode.
5. Add HLS manifest ingest with segment URL validation.
6. Add constrained `yt-dlp` JSON extractor mode.
7. Add browser probe as a separate expensive queue class.
8. Add domain policy, per-host concurrency, and crawl-delay enforcement.

## Open Questions

- Which sites are explicitly in scope and authorized?
- Is the first output target audio-only MP3, MP4 with a still image, or both?
- Should user-provided cookies be supported, and how will they be stored?
- Is the deployment single VPS, containerized worker fleet, or local-only?
- What takedown and retention policy should be enforced automatically?
