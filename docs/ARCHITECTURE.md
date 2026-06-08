# Architecture

Reflection King starts as a single deployable API with a shared core crate and a future worker crate.

## Crates

- `reflection-core`: runtime config, API models, SQLite job store, public URL validation, downloader, transcoder, storage paths, and shared errors.
- `reflection-api`: Axum HTTP server, persistent job registry, in-process dispatcher, worker loop, health endpoint, and `/media` byte-range serving.
- `reflection-worker`: placeholder binary for future separate process queue consumers.

## Current Flow

```text
POST /api/jobs
  -> validate API key
  -> normalize bitrate
  -> create queued job in SQLite
  -> enqueue job id for local dispatch
  -> worker downloads source
  -> worker transcodes audio to MP3
  -> worker publishes /media/<job-id>/audio.mp3 with Range support
```

The API stores job records in `storage/reflection.db` by default. At startup it resets any `queued`, `downloading`, or `transcoding` job back to `queued` and re-enqueues it. The dispatcher is still local to the API process, so the next scaling step is a lease-based worker protocol before running multiple consumers.

## Crawler Direction

The crawler backend should be split into discovery and capture. Discovery
returns media candidates; capture downloads or records one selected candidate.
Every candidate URL is untrusted until it passes the same fetch policy as a
direct user URL.

See [crawler/media-acquisition-design.md](crawler/media-acquisition-design.md)
and
[research/crawler-backend-survey-2026-06-09.md](research/crawler-backend-survey-2026-06-09.md).

## Future Media Layers

- `source_resolver`: identify source type, direct file, HLS, DASH, platform extractor, or live stream.
- `capture`: acquire direct file, segment stream, or hand off to an extractor such as `yt-dlp`.
- `processing`: ffmpeg profiles for MP3, AAC, MP4, HLS, thumbnails, probes, and loudness normalization.
- `storage`: local disk first, then S3-compatible object storage.
- `delivery`: direct file URLs, signed URLs, range requests, and optional CDN.
- `policy`: domain allowlists, per-user quotas, retention, copyright rules, and platform restrictions.
