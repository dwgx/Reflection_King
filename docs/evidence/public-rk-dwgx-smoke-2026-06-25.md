# Public rk.dwgx.top smoke evidence 2026-06-25

Base URL under test: `https://rk.dwgx.top`.

All jobs in this report were created with a short-lived `public website smoke` user key capped at 300 MiB. The key was created only for this run and must be revoked after the evidence is written.

## Tooling fixes made during this run

- `scripts/smoke/live_smoke.py` now sends `User-Agent: ReflectionKingSmoke/0.1` by default. Without a User-Agent, Cloudflare returned `HTTP 403` with `error code: 1010` for `POST /api/jobs`, while curl and Python with any explicit UA returned `202`.
- `live_smoke.py` now treats `needs_profile` as a terminal state, so authorization/header failures are recorded directly instead of timing out.
- `live_smoke.py` archive checks now match the current archive tree layout: `metadata/resources.json` and `preview/screenshot.png` rather than old root-level paths.

## Public API and capability checks

- `GET https://rk.dwgx.top/api/health`: `ok: true`, `public_base_url: https://rk.dwgx.top`.
- `GET /api/capabilities` with smoke key: browser probe, yt-dlp, and external adapters all configured; public base URL is `https://rk.dwgx.top`.

## Verified success on public URL

| Area | Case | Result | Evidence file |
| --- | --- | --- | --- |
| Direct video | `w3schools-bbb-direct-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-core-smoke-2026-06-25.json` |
| Direct audio | `w3schools-horse-direct-audio` | ready, MP3 Range 206 | `docs/evidence/public-rk-dwgx-core-smoke-2026-06-25.json` |
| Direct video | `blender-sintel-trailer-direct-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-core-smoke-2026-06-25.json` |
| Direct image | `w3c-direct-image` | ready, Range 206 | `docs/evidence/public-rk-dwgx-image-smoke-2026-06-25.json` |
| YouTube | `youtube-bbb-external-video-360p` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-youtube-360p-smoke-2026-06-25.json` |
| SoundCloud | `soundcloud-nasa-public-audio` | ready, MP3 Range 206 | `docs/evidence/public-rk-dwgx-platform-smoke-2026-06-25.json` |
| Bilibili | `bilibili-public-browser-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-platform-smoke-2026-06-25.json` |
| AcFun | `acfun-public-external-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-platform-smoke-2026-06-25.json` |
| Youku | `youku-public-trailer-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-platform-smoke-2026-06-25.json` |
| Vimeo | `vimeo-external-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-platform-smoke-2026-06-25.json` |
| Douyin | `douyin-public-browser-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-experimental-smoke-2026-06-25.json` |
| Weibo | `weibo-public-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-experimental-smoke-2026-06-25.json` |
| HLS stress | `apple-bipbop-hls-live-video` | ready, MP4 Range 206 | `docs/evidence/public-rk-dwgx-experimental-smoke-2026-06-25.json` |
| Page archive | three MTR / District Council page_html cases | ready, archive tree + file samples pass | `docs/evidence/public-rk-dwgx-page-html-smoke-2026-06-25.json` |

All successful media artifact URLs in these summaries use `https://rk.dwgx.top/media/...`, not loopback URLs.

## Non-green platform results

| Case | Status | Interpretation | Evidence file |
| --- | --- | --- | --- |
| `ximalaya-public-audio-recheck` | `needs_profile` | yt-dlp candidates require headers that are not persisted; this is an auth/header persistence adapter issue, not a public URL rewrite issue. | `docs/evidence/public-rk-dwgx-ximalaya-recheck-2026-06-25.json` |
| `tiktok-public-external-video` | `needs_profile` | Candidates were found, but target media URLs returned `HTTP 403 Forbidden`; needs headers/cookie/profile handling or a better TikTok route. | `docs/evidence/public-rk-dwgx-experimental-smoke-2026-06-25.json` |
| `kuaishou-public-browser-video` | `error` | Browser probe found no media candidates for the current sample. Needs new sample or Kuaishou-specific adapter work. | `docs/evidence/public-rk-dwgx-experimental-smoke-2026-06-25.json` |

## Observed smoke-design issue

The catalog case `youtube-bbb-external-video` with `auto` quality timed out at 300 seconds in the smoke script, but the backend job later finished successfully as a 28.9 MB 360p MP4. A dedicated `360p` YouTube smoke completed quickly. For broad platform smoke, use explicit lower quality such as `360p`/`480p` or improve auto-selection to avoid picking oversized candidates during automated tests.

## Follow-up engineering items

1. Preserve image MIME/extension for direct image outputs. The `w3c-direct-image` job succeeded, but the artifact came back as `application/octet-stream` with a `.bin` name.
2. Add a low-cost platform catalog tier or per-case bitrate/quality defaults so YouTube/Bilibili/etc. smoke does not select oversized formats by default.
3. Investigate Ximalaya header persistence and TikTok 403 candidate download behavior.
4. Add or update Kuaishou samples/adapters; the current browser sample no longer yields candidates.
5. Consider whether page_html smoke should verify artifact list URLs as well as archive tree paths.
