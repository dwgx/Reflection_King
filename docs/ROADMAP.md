# Roadmap

## Phase 1: Foundation

- Rust workspace and Git root.
- Axum API.
- Direct URL to MP3 job.
- Basic SSRF protection.
- Local storage.
- SQLite-backed job records and startup recovery.
- Single-range HTTP media delivery.

## Phase 2: Production Readiness

- Lease-based queue claiming for standalone workers.
- Fetch attempts table and retry classification.
- Cleanup scheduler.
- API authentication.
- Rate limits and quotas.
- Structured tracing and request IDs.

## Phase 3: Media Discovery

- `source_resolver` input classification.
- `MediaCandidate` model and candidate table.
- Direct URL path refactored through candidate selection.
- `ffprobe` metadata extraction.
- HLS/DASH capture.
- Constrained `yt-dlp --dump-single-json` adapter.
- Browser probe queue with strict budgets.

## Phase 4: Rich Media Outputs

- MP4 output profiles.
- Thumbnail generation.
- Storage adapters.

## Phase 5: Online Video And Live

- Live stream capture windows.
- Segment reconciliation.
- Source policy engine.
- UI/admin dashboard.
