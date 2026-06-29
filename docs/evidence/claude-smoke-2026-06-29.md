# Claude public smoke - 2026-06-29 (rk.dwgx.top via localhost:8780)

Low-priv temporary user key (revoked after run).

## Results
- bbb-mp4-direct (direct/video): READY. Raw URL served 64,618,768 B video/mp4,
  accept-ranges: bytes, Range 0-1023 -> HTTP 206, bytes are valid ISO MP4 header.
  -> Direct media download + remux + Range raw URL path VERIFIED working.
- wiki-png/jpg (direct/image): FAIL HTTP 403. Wikimedia upload.* blocks default UA.
  -> Real gap: direct downloader needs per-host UA/Referer for hotlink-protected hosts.
- platform routing (youtube/bilibili/twitter/ximalaya): jobs accepted HTTP 202.
  API echoes requested platform_hint=auto; actual inference applied internally.
- wayback-example (browser/page_html): FAIL. browser_probe sidecar returned no
  media candidates; sidecar auth returned 401 to internal curl probe.
  -> page_html capture path needs verification with a properly authorized probe call.

## Verified vs gap
- VERIFIED: SQLite key issuance, capabilities (41 hints), direct video pipeline + Range raw URL.
- GAP: hotlink-protected image hosts (403), browser_probe page_html chain produced no candidates.
