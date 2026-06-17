# Page Archive and Cache Maintenance Evidence 2026-06-16

## Summary

This record documents the 2026-06-16 webpage archive, archive browser, and cache
maintenance changes. The work extends `outputs: ["page_html"]` beyond a single
HTML preview into an inspectable frontend package with resource provenance and
admin cache cleanup controls.

The change is not a platform-smoke result. It is implementation evidence plus
local Rust, TypeScript, and browser-sidecar verification. Docker verification is
still required on a host with Docker.

## Implemented Behavior

- Page archive jobs now produce a structured package with `index.html`,
  `index.inline.html`, `page.html`, `page.txt`, `screenshot.png`,
  `resources.json`, `archive.zip`, and optional `archive.mhtml`, `archive.har`,
  and `archive.warc`.
- Browser sidecar page capture can use CDP network events to record request and
  response metadata, initiator/frame details, redirect chains, cache/service
  worker flags, and bounded CSS/JS/image/font/manifest/wasm bodies.
- `resources.json` records provenance for archived and skipped resources,
  including origin comparison, capture source, local path, and skip reason.
- The dashboard can list extracted archive files through
  `GET /api/jobs/{id}/archive/tree`.
- Individual archive files are streamed through
  `GET /api/jobs/{id}/archive/file?path=<relative-path>` with the same job
  authorization rules as the job detail endpoint.
- The dashboard opens archive files through authenticated `fetch` plus a blob
  URL. It does not expose API keys in query strings and does not make archive
  files public.
- Admin cache inventory reports public artifacts, temporary job directories,
  and browser profiles. Browser profiles are visible for accounting but not
  eligible for cleanup.
- Cache cleanup preview and execution delete only old temporary directories and
  orphan public artifact directories. Active job temporary directories are
  excluded before deletion.

## Review Fixes Applied

Two P2 review findings were fixed before handoff:

- Archive resource links initially used plain `<a href>` navigation, which did
  not include `x-api-key` and returned 401 for authenticated dashboard users.
  The dashboard now opens archive files via authenticated fetch/blob URLs.
- Cache cleanup initially scanned every old child under `storage/tmp`. It now
  queries active job IDs and skips matching `storage/tmp/<job_id>` directories
  for queued, resolving, candidate-selected, downloading, capturing, probing,
  transcoding, and remuxing jobs.

## 2026-06-17 Follow-up Fixes

Two user-visible archive opening regressions were fixed after VPS smoke:

- `3b124e2` changes dashboard artifact and archive opening from asynchronous
  `window.open` after fetch to a synchronous blank popup followed by
  authenticated `fetch` and blob URL navigation. This avoids browser popup
  blocking while keeping `x-api-key` out of query strings.
- `10fa337` rewrites legacy public `/media/...` URLs in job detail responses as
  well as job lists and artifact lists. Old rows created with
  `http://127.0.0.1:8780` are exposed through the current runtime
  `public_base_url` when the target is a same-service media URL. External CDN
  URLs and authenticated `/api/jobs/{id}/archive/file?...` URLs are not
  rewritten.
- `63ba956` and `60c4924` improve the page archive dashboard presentation:
  the generated `archive.zip` web frontend package is ranked first, archive
  files open in an in-dashboard authenticated blob preview instead of a popup,
  and JSON/HAR/MHTML/ZIP artifacts no longer render bogus zero-duration media
  controls.
- A follow-up dashboard guard rejects Reflection King's own `/media/...` and
  `/api/jobs/...` URLs in the source URL field before job creation. Those URLs
  are generated artifacts/API endpoints, not source pages. Submitting the VPS
  service address as a source URL still remains blocked by Rust URL policy
  because `192.168.11.4` is a private RFC1918 address and must not be fetched
  as an untrusted remote source.
- Source URL handling now accepts bare host URLs such as
  `www.youtube.com/watch` and normalizes them to `https://www.youtube.com/watch`
  before policy validation. The dashboard no longer lets the browser's native
  URL input validation block this common paste form before application logic
  can normalize it.
- Job detail responses now preserve `original_source_url` for newly created
  jobs and still expose normalized `source_url` as the resolver input. The
  dashboard task detail panel shows the original input when it differs from the
  normalized URL and always shows the full source URL with copy/open actions.
  Older jobs created before this field existed can only show the normalized
  `source_url`.

VPS verification after rebuilding the Docker service from `master`:

- `/api/health` returned
  `public_base_url: "http://192.168.11.4:8780"`.
- Old MTR page archive job `5ba18559-afd1-4e81-9477-42547ab3cbab` returned
  `media_url` as
  `http://192.168.11.4:8780/media/5ba18559-afd1-4e81-9477-42547ab3cbab/archive.zip`
  from `GET /api/jobs/{id}`.
- The same job returned eight artifact URLs and none contained `127.0.0.1`,
  `localhost`, or `0.0.0.0`.
- `GET /media/5ba18559-afd1-4e81-9477-42547ab3cbab/page.html` returned
  `200 OK`, `text/html`, and `content-length: 327`.
- `GET /api/jobs/{id}/archive/file?path=index.html` without `x-api-key`
  returned `401 Unauthorized`, confirming archive file auth is still enforced.
- The same archive file request with `x-api-key` returned `200 OK` and
  `text/html`.

The old MTR archive content itself is an upstream Access Denied page returned by
the target site. This verifies file delivery and auth behavior, not that the
archived MTR page content is useful or playable.

GitHub Actions run `27668778414` passed all jobs for `10fa337`, including Rust,
dashboard, browser sidecar, Docker build, Compose config, Compose up, and health
check.

## Verification Performed

Commands that passed locally:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm.cmd --prefix apps/reflection-dashboard run build
npm.cmd --prefix services/reflection-browser run check
npm.cmd --prefix services/reflection-browser run build
git diff --check
```

Rust was installed locally for this verification under `D:\Software\Rust`
without auto-modifying PATH. The checks used `RUSTUP_HOME=D:\Software\Rust\rustup`,
`CARGO_HOME=D:\Software\Rust\cargo`, and the existing Visual Studio Community
MSVC toolchain under `D:\Software\Microsoft Visual Studio\18\Community`.

The Rust pass also fixed two verification findings:

- `BrowserProbeClient::probe_session` exceeded clippy's argument limit after
  adding CDP/MHTML/HAR budget controls. The archive capture controls are now
  grouped in `ProbeSessionOptions`.
- `GET /api/jobs/{id}/archive/file` opened its file handle as mutable even
  though it was only used for metadata and streaming construction; the unused
  `mut` was removed.

Direct sidecar smoke also passed against `https://example.com/` using a
temporary browser profile. The probe returned HTML, MHTML, HAR, one page
resource, and no warnings. The temporary profile directory was removed after
the smoke.

`git diff --check` passed.

## Verification Not Performed

Docker checks were not run locally because this Windows host does not have
Docker installed. Required follow-up checks on a Docker host:

```powershell
docker build --progress=plain -t reflection-king:ci .
docker compose --env-file .env.docker.example config
docker compose --env-file .env.docker.example up -d --build
```

## Safety Notes

- HAR output uses empty cookie arrays and only whitelisted request/response
  headers. Cookie and Authorization headers are not written to public archive
  views.
- Archive file paths are normalized relative paths under the job `page/`
  directory; absolute paths, backslashes, and `..` are rejected.
- `archive.warc` is written without introducing a new Rust compression
  dependency.
- Cache cleanup does not delete browser profiles, known public artifact
  directories, database history, or active temporary job directories.
