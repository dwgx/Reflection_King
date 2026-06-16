# Page Archive and Cache Maintenance Evidence 2026-06-16

## Summary

This record documents the 2026-06-16 webpage archive, archive browser, and cache
maintenance changes. The work extends `outputs: ["page_html"]` beyond a single
HTML preview into an inspectable frontend package with resource provenance and
admin cache cleanup controls.

The change is not a platform-smoke result. It is implementation evidence plus
local TypeScript/browser-sidecar verification. Rust verification is still
required in CI or on a host with `cargo`.

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

## Verification Performed

Commands that passed locally:

```powershell
npm.cmd --prefix apps/reflection-dashboard run build
npm.cmd --prefix services/reflection-browser run check
npm.cmd --prefix services/reflection-browser run build
git diff --check
```

Direct sidecar smoke also passed against `https://example.com/` using a
temporary browser profile. The probe returned HTML, MHTML, HAR, one page
resource, and no warnings. The temporary profile directory was removed after
the smoke.

`git diff --check` only reported the existing CRLF/LF normalization warning for
`apps/reflection-dashboard/src/main.tsx`.

## Verification Not Performed

Rust checks were not run locally because this Windows host does not have
`cargo`, `rustc`, Docker, Bash, or WSL installed.

Required follow-up checks:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

If Docker is available:

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
