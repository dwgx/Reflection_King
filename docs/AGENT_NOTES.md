# Agent Notes

## 2026-06-25T02:21:45Z - Codex - Public web archive smoke on rk.dwgx.top

- Goal: run live public smoke for web archive / network time-machine cases against `https://rk.dwgx.top`.
- Deployment/runtime changes:
  - Rebuilt and restarted the public Docker service with `docker compose up -d --build reflection-king`.
  - Fixed workspace `.env` `RK_PUBLIC_BASE_URL` from loopback to `https://rk.dwgx.top`, then restarted the container with `docker compose up -d reflection-king`.
  - Verified `GET https://rk.dwgx.top/api/health` reports `public_base_url: https://rk.dwgx.top`.
  - Verified `GET /api/capabilities` reports 41 platform hints including `wayback` and `memento`.
- Smoke key handling:
  - Existing `.tmp/admin-key.txt`, `.env` `RK_API_KEY`, container `RK_API_KEY`, and `/data/admin-key.txt` were not accepted by the current DB-backed API.
  - Inserted a temporary low-privilege `web archive smoke 2026-06-25` user key directly into container SQLite `api_keys`, capped at 300 MiB, with browser/yt-dlp/external adapters enabled and login Profile disabled.
  - Revoked the temporary key after the run and verified it returns `HTTP 401` on `/api/capabilities`.
- Evidence:
  - JSON: `docs/evidence/public-rk-dwgx-web-archive-smoke-2026-06-25.json`.
  - Report: `docs/evidence/public-rk-dwgx-web-archive-smoke-2026-06-25.md`.
- Result:
  - 8/8 web archive cases reached `ready`.
  - Cases: `wayback-example-page-html-hint`, `wayback-cdx-api-hint`, `archive-it-cdx-collection-hint`, `perma-public-archives-api-hint`, `archive-today-search-page-hint`, `ghostarchive-public-archive-page-hint`, `webcitation-legacy-page-hint`, `memento-timegate-doc-page-hint`.
  - Every case produced required page archive files: `index.html`, `index.inline.html`, `metadata/resources.json`, `page.txt`, and `preview/screenshot.png`.
  - Public archive-file samples were readable through `https://rk.dwgx.top/api/jobs/.../archive/file` using authenticated smoke requests.
- Not verified:
  - This verifies `page_html` capture/readback routes, not media extraction from replay pages.
  - Dedicated structured extractors for Wayback CDX, Archive-It CDX/C, Memento Link/TimeMap, and Perma public archive records are still future work.

## 2026-06-25T01:52:29Z - Codex - Web archive/time-machine platform expansion

- Goal: expand parsing logic for public web archives / "network time machine" sources found through current internet research.
- Research sources:
  - Internet Archive Wayback API docs, Wayback CDX server docs, Memento RFC 7089/MementoWeb docs, Archive-It CDX/C docs, Perma.cc developer docs, Ghostarchive docs, WebCitation, and archive.ph public search patterns.
  - `docs/research/platform-expansion-survey-2026-06-25.md` now records the web-archive boundary and source URLs.
- Code/doc changes:
  - Added `wayback`, `archive_it`, `perma_cc`, `archive_today`, `ghostarchive`, `webcitation`, and `memento` platform hints.
  - Added API URL inference for `web.archive.org`, `archive.org/web/`, `archive-it.org`, `wayback.archive-it.org`, `perma.cc`, archive.today/archive.ph mirrors, Ghostarchive, WebCitation, and MementoWeb URLs.
  - API now defaults omitted outputs for those web-archive platforms to `page_html`, while still respecting explicit caller outputs.
  - Dashboard now auto-switches recognized web-archive URLs to `browser` + `page_html` + `auth none`, and adds labels/dropdown/login shortcuts.
  - Browser sidecar filters web-archive candidates to archive-provider hosts instead of treating replay pages as generic media.
  - Cookie import allowlist now includes only the archive-provider domains for these platform hints.
  - `scripts/smoke/catalogs/internet-platform-expansion-2026-06-25.json` now includes Wayback, CDX, Archive-It, Perma, archive.today, Ghostarchive, WebCitation, and Memento experimental cases.
  - `docs/WORKFLOW.md` documents that archive/time-machine cases are page_html-first and must not auto-create external captures.
- Verification passed:
  - `cargo fmt --all -- --check`
  - `cargo test -p reflection-api infer_platform_covers_new_public_platforms -- --nocapture`
  - `cargo test -p reflection-api web_archive_platforms_default_to_page_html_when_outputs_are_omitted -- --nocapture`
  - `cargo clippy -p reflection-api --all-targets -- -D warnings`
  - `npm run check && npm run build` in `services/reflection-browser`
  - `npm run build` in `apps/reflection-dashboard`
  - `python3 -m json.tool scripts/smoke/catalogs/internet-platform-expansion-2026-06-25.json`
  - `python3 scripts/smoke/live_smoke.py --catalog scripts/smoke/catalogs/internet-platform-expansion-2026-06-25.json --catalog-only --list`
  - `cargo test --workspace`
  - `git diff --check`
- Not verified:
  - No live public smoke was run for the new web-archive catalog cases in this pass.
  - No dedicated Wayback CDX/Memento/Perma structured extractor was added yet; this pass is platform routing, page_html defaulting, browser filtering, docs, and fixture expansion.
  - Do not implement automatic "save this page into archive service" behavior without an explicit user action.
- Next step: run selected web-archive cases against `https://rk.dwgx.top`, record JSON evidence, then add dedicated read-only extractors for Wayback CDX, Archive-It CDX/C, Memento Link/TimeMap, and Perma public archives.

## 2026-06-25T01:25:13Z - Codex - Internet platform expansion hints

- Goal: expand parsing logic from internet/upstream extractor research without claiming unverified platform success.
- Research sources:
  - `yt-dlp` supported sites, Streamlink plugin list, `you-get` README, and `gallery-dl` supported sites.
  - `docs/research/platform-expansion-survey-2026-06-25.md` records the sources, platform groups, fixture rules, and why gallery-dl is not wired as a binary adapter yet.
- Code/doc changes:
  - Expanded `PlatformHint` and API `supported_platform_hints` via `PlatformHint::SUPPORTED`.
  - Added URL inference tests for Dailymotion, Rumble, PeerTube, Archive.org, Wikimedia, Twitch, X/Twitter, Reddit, Instagram, Facebook, Pinterest, Imgur, Flickr, Bandcamp, Mixcloud, Niconico, FC2, and Spotify.
  - Expanded Dashboard platform types, labels, fallback dropdown list, and login shortcuts.
  - Expanded cookie import domain allowlists for the new platform hints.
  - Expanded browser sidecar filtering for video/social, library/archive, image/gallery, and audio/community platform groups.
  - Added `scripts/smoke/catalogs/internet-platform-expansion-2026-06-25.json` and documented its use in `docs/WORKFLOW.md`.
- Verification passed:
  - `cargo fmt --all -- --check`
  - `cargo clippy -p reflection-api --all-targets -- -D warnings`
  - `cargo test -p reflection-api infer_platform_covers_new_public_platforms -- --nocapture`
  - `npm run check && npm run build` in `services/reflection-browser`
  - `npm run build` in `apps/reflection-dashboard`
  - `python3 -m json.tool scripts/smoke/catalogs/internet-platform-expansion-2026-06-25.json`
  - `python3 scripts/smoke/live_smoke.py --catalog scripts/smoke/catalogs/internet-platform-expansion-2026-06-25.json --catalog-only --list`
  - `git diff --check`
- Not verified:
  - No live public smoke was run for the new catalog in this pass.
  - New platform hints are recognition/filtering expansion, not proof that each platform downloads/transcodes successfully.
  - Spotify is intentionally metadata/page-only; do not add Spotify media extraction.
- Next step: run the new catalog against `https://rk.dwgx.top`, record JSON evidence, then promote only stable green fixtures into regular platform smoke.

## 2026-06-25T00:58:34Z - Codex - Cargo PATH repair

- Goal: verify and fix the reported missing `cargo` in this shell.
- Findings:
  - Cargo was already installed; the earlier failure was caused by the active Codex shell `PATH` missing `/home/dwgx_user/.cargo/bin`.
  - Verified both `/home/dwgx_user/.cargo/bin` and repo-local `.local/cargo/bin` contain Rust 1.96.0 tooling.
- Host change:
  - Added `/usr/local/bin` symlinks for `cargo`, `rustc`, `rustup`, `rustfmt`, `cargo-fmt`, `cargo-clippy`, `clippy-driver`, and `rustdoc`, pointing at `/home/dwgx_user/.cargo/bin/*`.
  - This makes bare `cargo` available to both root shell and `dwgx_user` shell without changing repo files or shell startup files.
- Verification:
  - `cargo --version` -> `cargo 1.96.0 (30a34c682 2026-05-25)`.
  - `rustc --version` -> `rustc 1.96.0 (ac68faa20 2026-05-25)`.
  - `su - dwgx_user -c 'cd /home/dwgx_user/reflection-king-smoke && cargo --version && rustc --version && rustup show active-toolchain'` passed.
  - Bare `cargo test -p reflection-api resolve_public_base_url -- --nocapture` passed: 5 tests ok, with only pre-existing dead-code warnings.
- Next step: future Rust commands in this repo can use bare `cargo`; no `RUSTUP_HOME`/`CARGO_HOME` prefix is required unless intentionally using the repo-local `.local` toolchain cache.

## 2026-06-25T00:43:00Z - Codex - Public rk.dwgx.top cross-site smoke

- Goal: test media/page crawling through the public URL `https://rk.dwgx.top` rather than local container ports.
- Service/key handling:
  - Created a short-lived `public website smoke` user API key capped at 300 MiB for this run.
  - All smoke requests used the public Cloudflare URL, not `127.0.0.1`.
  - The temporary key must be revoked after this note; no real admin key was exposed in repo files.
- Tooling changes:
  - `scripts/smoke/live_smoke.py` now sends a default `User-Agent: ReflectionKingSmoke/0.1`; Python `urllib` without UA was blocked by Cloudflare with `HTTP 403` / `error code: 1010` on `POST /api/jobs`.
  - `live_smoke.py` now treats `needs_profile` as terminal so auth/header failures are reported directly instead of as timeouts.
  - `live_smoke.py` page archive checks now use current archive tree paths `metadata/resources.json` and `preview/screenshot.png`.
- Public smoke evidence:
  - Summary report: `docs/evidence/public-rk-dwgx-smoke-2026-06-25.md`.
  - JSON evidence:
    - `docs/evidence/public-rk-dwgx-core-smoke-2026-06-25.json`
    - `docs/evidence/public-rk-dwgx-platform-smoke-2026-06-25.json`
    - `docs/evidence/public-rk-dwgx-youtube-360p-smoke-2026-06-25.json`
    - `docs/evidence/public-rk-dwgx-experimental-smoke-2026-06-25.json`
    - `docs/evidence/public-rk-dwgx-page-html-smoke-2026-06-25.json`
    - `docs/evidence/public-rk-dwgx-image-smoke-2026-06-25.json`
    - `docs/evidence/public-rk-dwgx-ximalaya-recheck-2026-06-25.json`
- Verified ready with public `https://rk.dwgx.top/media/...` artifact URLs and Range/file samples:
  - Direct video/audio/image, YouTube 360p, SoundCloud, Bilibili, AcFun, Youku, Vimeo, Douyin browser route, Weibo, Apple HLS stress sample, and three page_html archive cases.
- Non-green findings:
  - Ximalaya reaches `needs_profile`: selected yt-dlp audio candidates require headers that are not persisted.
  - TikTok reaches `needs_profile`: candidates are discovered but CDN downloads return `HTTP 403 Forbidden`.
  - Kuaishou browser sample returns no media candidates.
  - Direct image smoke succeeds, but artifact content type/name degrade to `application/octet-stream` / `.bin`; preserve source MIME/extension later.
  - The auto-quality YouTube platform catalog case timed out at 300 seconds, but the backend job later finished; broad smoke should request low quality such as `360p` or improve test-time auto-selection.
- Verification commands:
  - `python3 -m py_compile scripts/smoke/live_smoke.py`
  - Multiple `python3 scripts/smoke/live_smoke.py --base-url https://rk.dwgx.top ... --summary-file docs/evidence/public-rk-dwgx-*-2026-06-25.json`
- Next step: fix direct-image MIME/extension preservation, add low-cost platform quality defaults, then investigate Ximalaya header persistence, TikTok 403 handling, and Kuaishou adapter/sample drift.

## 2026-06-25T00:09:18Z - Codex - Public container rebuild and URL rewrite hardening

- Goal: deploy the current Reflection King code to the public Compose container `reflection-king-smoke-reflection-king-1` for `https://rk.dwgx.top`.
- Service changes:
  - Rebuilt and recreated the Compose service with `RK_PUBLIC_BASE_URL=https://rk.dwgx.top RK_API_KEY=local-dev-admin-key docker compose up -d --build reflection-king`.
  - Hardened `crates/reflection-api/src/main.rs` public-base selection so a configured public URL is not overwritten by loopback/private `Host` headers; trusted `x-forwarded-host` / `x-forwarded-proto` still take priority.
  - Added unit coverage for loopback/private Host fallback behavior in `resolve_public_base_url`.
  - Created short-lived deploy verification API keys directly in `/data/reflection.db` only for runtime checks, then revoked them and removed local temp key files.
- Verification:
  - Docker release build compiled `reflection-api` successfully; only pre-existing dead-code warnings were emitted for unused base-URL-neutral helper methods.
  - `git diff --check` passed before the deployment rebuild.
  - Container is running from image `reflection-king:local` with `0.0.0.0:8780->8780/tcp`.
  - Container env readback: `RK_PUBLIC_BASE_URL=https://rk.dwgx.top`, `RK_PUBLIC_PORT=8780`, `RK_BIND_ADDRESS=0.0.0.0:8780`.
  - `curl http://127.0.0.1:8780/api/health` returned `ok: true` and `public_base_url: https://rk.dwgx.top`.
  - Capabilities with a temporary admin key showed browser probe, yt-dlp, and external adapters configured.
  - Existing Bilibili video job `28c7268d-fc5f-45c3-99d0-27fa96934f0c` now returns `media_url`, `status_url`, `artifacts_url`, and `trace_url` under `https://rk.dwgx.top/...` even when queried through the local container port.
  - The same job's artifact endpoint returns one artifact URL under `https://rk.dwgx.top/media/...`.
  - Local media HEAD check for the Bilibili MP4 returned `HTTP/1.1 200 OK`, `content-type: video/mp4`, `content-length: 9357598`.
  - Dashboard bundle in the container contains the incomplete YouTube `/watch` and empty `youtu.be` input guards.
- Not verified:
  - The public hostname `https://rk.dwgx.top` was not externally requested from this agent; verification used local origin plus API URL rewriting checks.
  - Superseded by 2026-06-25T00:58:34Z note: cargo was installed but not on the active shell `PATH`; bare `cargo test -p reflection-api resolve_public_base_url -- --nocapture` now passes after PATH repair.
- Next step: persist the production `RK_PUBLIC_BASE_URL` into an explicit production env/override workflow so future plain `docker compose up -d --build` commands cannot accidentally reintroduce local debug URLs from `.env`.

## 2026-06-24T09:31:08Z - Codex - Bilibili auto-flow repair and verification

- Goal: explain and fix why `https://www.bilibili.com/video/BV1jFJ36rEAx/` looked unparseable in the local dashboard.
- Findings:
  - The source URL itself is live and discoverable; current HEAD produced candidate sets from both `yt_dlp` and `browser_probe`.
  - The main product bug was local workflow mismatch: create-job flow stopped at `candidates_ready` for non-`direct` tasks, while the dashboard copy promised automatic highest-quality selection.
  - A separate dashboard race allowed an older job detail poll to overwrite the currently selected task panel, which can make a Bilibili row show YouTube details.
  - A backend completion bug previously allowed a media job to be marked `ready` after only a partial artifact set succeeded; this was already fixed in the same pass.
- Code changes:
  - Added backend `recommended_candidates_for_job()` auto-selection and `auto_select_recommended_candidates()` so normal create-job flow now auto-selects recommended candidates after discovery and re-enqueues processing.
  - Added backend output-completeness guard so `ready` requires an artifact that satisfies the requested primary output kind.
  - Added dashboard request sequencing guard to ignore stale `loadJob()` responses after task switches.
  - Added targeted Rust tests for media-job recommendation and output-completeness behavior.
- Verification:
  - `cargo test -p reflection-api recommended_media_job_selects_video_with_audio_companion -- --nocapture`
  - `cargo test -p reflection-api ready_requires_video_artifact_for_video_jobs -- --nocapture`
  - `cargo test -p reflection-api ready_accepts_video_artifact_for_media_jobs -- --nocapture`
  - `npm run build` in `apps/reflection-dashboard`
  - `git diff --check`
  - Live local repro on current HEAD:
    - Created job `8c067461-b7b0-4db1-a687-cc60cafcb95d` for `BV1jFJ36rEAx`
    - Trace advanced through `candidates_ready -> candidate_selected -> downloading -> remuxing -> ready`
    - Produced artifact `video-b0712456-d952-4cb7-85a6-64e07b2b56e9.mp4` at `/media/8c067461-b7b0-4db1-a687-cc60cafcb95d/...`
- Not verified:
  - The browser-probe warning `generic discovery failed: page.evaluate: ReferenceError: __name is not defined` still exists as diagnostic noise and should be cleaned separately.
  - Full dashboard click-through UI validation for the stale-detail race was not browser-automated in this pass, but the request-order guard is now in code and the rebuilt bundle is present.
- Next step: audit and remove the `__name is not defined` browser-probe warning, then run a broader cross-platform live smoke pass on current HEAD instead of the earlier stale binary.

## 2026-06-24T08:03:00Z - Codex - Platform expansion pass for Weibo and Ximalaya

- Goal: continue expanding publicly discoverable platforms with real adapter evidence instead of only growing a candidate list.
- Code/doc changes:
  - Added `ximalaya` and `weibo` to `PlatformHint` in Rust models, API capabilities, source-URL inference, dashboard labels/login shortcuts, and cookie-import domain allowlists.
  - Added API unit coverage for `infer_platform()` on Ximalaya and Weibo sample URLs.
  - Updated browser sidecar filtering so platform-specific branches can trigger from explicit `platformHint` as well as final URL host, then added dedicated lightweight filters for Ximalaya and Weibo candidate cleanup.
  - Expanded `scripts/smoke/catalogs/platform-discovery-2026-06-24.json` with public Ximalaya and Weibo cases and noted the new priority in `docs/WORKFLOW.md`.
- Validation: pending in this note entry; next step is to run focused Rust/TypeScript checks after the code patch set settles.
- Not verified:
  - Live Ximalaya/Weibo smoke was not run yet.
  - Weibo visitor/anti-bot flow still likely needs either browser fallback or external-tool tuning depending on environment.
- Next step: run `cargo test` for API inference coverage plus browser/dashboard TypeScript build checks, then decide whether the next batch should be `xiaohongshu` or `douyu/huya`.

## 2026-06-24T04:35:34Z - Codex - Security fixes and platform discovery expansion

- Goal: fix the obvious severe audit findings, then expand public platform discovery inputs.
- Code/doc changes:
  - Added policy-aware Rust HTTP client wrapper in `crates/reflection-core/src/policy_http.rs`.
  - Routed direct downloads, HLS manifest validation, Hanime, and MacCMS extractor HTTP requests through DNS filtering, proxy-disabled clients, and response peer-address validation.
  - Added browser sidecar DNS/IP policy checks and Playwright request routing so browser probes and login-session navigation abort localhost/private/link-local/documentation/multicast targets.
  - Added API-layer browser login session ownership tracking in `crates/reflection-api/src/state.rs` and enforced owner/job checks in `crates/reflection-api/src/main.rs`.
  - Expanded browser generic discovery in `services/reflection-browser/src/probe.ts` for encoded URLs and common platform JSON fields such as `playUrl`, `baseUrl`, `backupUrl`, `host` + `path`, HLS/DASH/media URL lists.
  - Added Dashboard login shortcuts for SoundCloud, Vimeo, and a MacCMS-style resource site entry.
  - Added `scripts/smoke/catalogs/platform-discovery-2026-06-24.json` and documented the platform catalog smoke command in `docs/WORKFLOW.md`.
- Validation passed:
  - `RUSTUP_HOME=$PWD/.local/rustup CARGO_HOME=$PWD/.local/cargo PATH=$PWD/.local/cargo/bin:$PATH cargo fmt --all -- --check`.
  - `RUSTUP_HOME=$PWD/.local/rustup CARGO_HOME=$PWD/.local/cargo PATH=$PWD/.local/cargo/bin:$PATH cargo clippy --workspace --all-targets -- -D warnings`.
  - `RUSTUP_HOME=$PWD/.local/rustup CARGO_HOME=$PWD/.local/cargo PATH=$PWD/.local/cargo/bin:$PATH cargo test --workspace`.
  - `npm run check` and `npm run build` in `services/reflection-browser`.
  - `npm run build` in `apps/reflection-dashboard`.
  - `git diff --check`.
  - `bash -n install.sh scripts/deploy/*.sh scripts/smoke/*.sh`.
  - `python3 scripts/smoke/live_smoke.py --catalog scripts/smoke/catalogs/platform-discovery-2026-06-24.json --catalog-only --list`.
  - Restarted local API on `http://127.0.0.1:8787` and browser sidecar on `http://127.0.0.1:8791`; verified `/api/health`, browser `/health`, and `/api/capabilities` with `local-dev-admin-key`.
- Not verified:
  - Live platform smoke against YouTube/SoundCloud/Bilibili/AcFun/Youku/TikTok/Douyin/Kuaishou/Vimeo was not run.
  - ffmpeg external network access still needs a stronger future egress control/proxy if the project requires DNS pinning beyond Rust-owned HTTP fetches and browser-side request blocking.
  - Docker build/Compose was not run in this turn.
- Next step: run platform catalog smoke with the local or VPS API, collect summary JSON under `docs/evidence/`, and use failures to prioritize dedicated adapters.

## 2026-06-23T06:02:02Z - Codex - Local takeover and smoke run

- Goal: learn the repository and get Reflection King running locally in `/home/dwgx_user/reflection-king-smoke`.
- Code/config changes: added local ignored `.env` with localhost-only settings and `RK_API_KEY=local-dev-admin-key`; generated ignored local runtime/build artifacts under `.local/`, `target/`, `storage/`, `services/reflection-browser/node_modules/`, `services/reflection-browser/dist/`, `apps/reflection-dashboard/node_modules/`, and `crates/reflection-api/dashboard-dist/`.
- Running services: `npm run dev` for `services/reflection-browser` is listening on `http://127.0.0.1:8791`; `cargo run -p reflection-api` is listening on `http://127.0.0.1:8787`.
- Validation passed:
  - `npm ci` in `services/reflection-browser` and `apps/reflection-dashboard`.
  - `npm run check && npm run build` in `services/reflection-browser`.
  - `npm run build` in `apps/reflection-dashboard`.
  - `RUSTUP_HOME=$PWD/.local/rustup CARGO_HOME=$PWD/.local/cargo PATH=$PWD/.local/cargo/bin:$PATH cargo check --workspace`.
  - `npx playwright install chromium` in `services/reflection-browser`.
  - `curl -fsS http://127.0.0.1:8791/health`.
  - `curl -fsS http://127.0.0.1:8787/api/health`.
  - `curl -fsS -H 'x-api-key: local-dev-admin-key' http://127.0.0.1:8787/api/capabilities`.
  - Browser-backed `page_html` smoke against `https://example.com/`: job `bc9e1842-d15f-43d9-9821-ec73315c2d72` reached `ready`; `archive/tree`, `archive/file?path=index.html`, and `artifacts` returned data.
  - Bundled Playwright Chromium UI smoke opened `http://127.0.0.1:8787`, set `reflection_api_key=local-dev-admin-key`, reloaded, and confirmed title `Reflection King`, API normal text, browser status text, and key status text.
- Verified state: `git status --short --branch` was clean after ignored runtime artifacts were generated.
- Not verified: full `cargo fmt`, `cargo clippy`, `cargo test`, Docker build/Compose, full media download/transcode catalog smoke, external adapters (`yt-dlp`, `you-get`, `streamlink`) because those tools are not configured in the local `.env`.
- Next step: use the running dashboard at `http://127.0.0.1:8787` with local key `local-dev-admin-key`, then run full checks once more code changes are made.
## 2026-06-24T10:42:00Z - Codex - Public URL rewrite and invalid YouTube input guard

- Goal: explain why `https://rk.dwgx.top/` could not open finished artifacts, and stop obviously broken `youtube.com/watch` inputs from creating doomed jobs.
- Findings:
  - The public-facing container on `0.0.0.0:8780` is a different runtime from the local debug API on `127.0.0.1:8787`.
  - The public container environment currently sets `RK_PUBLIC_BASE_URL=http://127.0.0.1:8787`, so completed jobs on the public site return loopback `media_url` values that external browsers cannot open.
  - Public DB evidence: job `3a7cc35f-bb67-4fdf-82c4-c8c7f31a3aac` is `ready` for Bilibili, while job `20e17df9-67e7-4981-b344-f7d097489d24` stored a truncated source URL `https://www.youtube.com/watch` and failed after yt-dlp timeout plus zero candidates.
- Code changes:
  - `crates/reflection-api/src/main.rs`: derive an effective public base URL from request headers (`x-forwarded-proto`, `x-forwarded-host`, `Host`) and use it for job create/list/detail/artifact/archive/trace responses.
  - `crates/reflection-api/src/state.rs`: added base-URL-aware variants for job/artifact/trace views and widened response URL rewriting to cover `status_url`, relative API endpoints, and trace `media_url`.
  - `apps/reflection-dashboard/src/main.tsx`: reject incomplete YouTube `/watch` and empty `youtu.be` inputs before task creation.
- Verification:
  - `npm run build` in `apps/reflection-dashboard` passed.
  - `cargo test -p reflection-api resolve_public_base_url_prefers_forwarded_headers -- --nocapture` passed.
  - `cargo test -p reflection-api public_media_urls_are_rewritten_to_current_base_url -- --nocapture` passed.
  - After restarting the local debug API, `curl` with `Host: rk.dwgx.top` plus `x-forwarded-proto: https` and `x-forwarded-host: rk.dwgx.top` returned rewritten `status_url`, `media_url`, artifact URLs, and trace `ready` event URLs under `https://rk.dwgx.top/...`.
- Next step: rebuild dashboard/API, run focused tests, then update the public container config so requests through `rk.dwgx.top` return HTTPS public media URLs instead of `127.0.0.1`.
