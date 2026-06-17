# Public source catalog smoke 2026-06-17

Date: 2026-06-17
Branch: `master`
Local HEAD during this work: `37281bb Fix platform smoke media handling` plus uncommitted smoke tooling/catalog changes.
VPS: SSH alias `dwgx-home-cloud`

## Purpose

This round expanded smoke testing beyond platform-specific samples by adding a
data-driven public source catalog. The goal is to make future source discovery
repeatable without editing the smoke script for every new URL.

The catalog deliberately separates:

- small public media samples that can be expected to pass in regular smoke;
- long or manifest stress samples that should be run manually;
- public information pages, including MTR/Tsuen Wan pages, that exercise
  `page_html` archive behavior rather than media extraction.

Do not treat a browser-visible page URL as proof of playable media support.
Media support still requires a generated artifact and Range/readback evidence.

## Source catalog

Catalog file:

```text
scripts/smoke/catalogs/public-media-2026-06-17.json
```

Low-cost media cases:

| Case | URL type | Expected evidence |
| --- | --- | --- |
| `w3schools-bbb-direct-video` | direct MP4 | generated MP4 artifact + Range `206` |
| `w3schools-horse-direct-audio` | direct MP3 | generated MP3 artifact + Range `206` |
| `blender-sintel-trailer-direct-video` | direct MP4 | generated MP4 artifact + Range `206` |

Experimental/manual cases:

| Case | URL type | Reason |
| --- | --- | --- |
| `dashif-bbb-30fps-dash-video` | DASH MPD | manifest handling and size/runtime still need targeted evidence |
| `apple-bipbop-hls-manual-video` | HLS master playlist | long stream; marked `expect_success=false` for manual stress use |
| `mtr-system-map-page-archive` | public HTML page | page archive source, not media support |
| `mtr-tsuen-wan-station-page-archive` | public HTML page | page archive source for MTR Tsuen Wan station information |
| `tsuen-wan-district-council-page-archive` | public HTML page | page archive source for Tsuen Wan government information |

The source URLs were HEAD-checked locally on 2026-06-17 for the small direct
media and MTR/Tsuen Wan pages. The public pages should still be considered
drift-prone because their content and CDN behavior can change.

## Tooling changes

`scripts/smoke/live_smoke.py` now supports:

- `--catalog <path>` to load JSON smoke cases;
- `--catalog-only` to run only catalog cases;
- `--summary-file <path>` to write machine-readable results;
- `--output page_html` for ad-hoc page archive smoke;
- authenticated `GET /api/jobs/{id}/archive/tree` and
  `GET /api/jobs/{id}/archive/file?path=...` sampling for `page_html`
  artifacts.

The page archive check expects these files when the current page archive
implementation is active:

```text
index.html
index.inline.html
page.html
page.txt
screenshot.png
resources.json
archive.zip
```

VPS helper scripts added:

- `scripts/smoke/vps_smoke_inspect.sh`
- `scripts/smoke/vps_run_catalog_smoke.sh`

These scripts are intended for temporary smoke environments. They read the
Docker admin key from `/data/admin-key.txt` inside the running container into a
0600 temp file, pass it to the smoke script, and delete the temp file on exit.
They do not print the key.

## VPS core smoke

Command shape:

```bash
bash /tmp/rk-vps-run-catalog-smoke.sh ~/reflection-king-smoke \
  --catalog-only \
  --tier core \
  --timeout-seconds 240 \
  --summary-file /tmp/rk-public-core-summary-2026-06-17.json
```

Result: 3/3 ready on `http://127.0.0.1:8780`.

| Case | Job | Result |
| --- | --- | --- |
| `w3schools-bbb-direct-video` | `38f94292-d469-44af-a179-1d8b3aa85e2f` | ready, `video/mp4`, Range `bytes 0-511/586538` |
| `w3schools-horse-direct-audio` | `9be10048-2a3b-45b3-8c17-78de1a5bfc0d` | ready, `audio/mpeg`, Range `bytes 0-511/25539` |
| `blender-sintel-trailer-direct-video` | `d1905fe3-de78-4463-9dd8-93f835a674f9` | ready, `video/mp4`, Range `bytes 0-511/4369173` |

Machine-readable evidence:

```text
docs/evidence/public-catalog-core-smoke-2026-06-17.json
```

## Page archive smoke on old VPS container

Command shape:

```bash
bash /tmp/rk-vps-run-catalog-smoke.sh ~/reflection-king-smoke \
  --catalog-only \
  --case mtr-tsuen-wan-station-page-archive \
  --timeout-seconds 260 \
  --summary-file /tmp/rk-public-page-summary-2026-06-17.json
```

Result on the currently running temporary VPS container:

- job `5ba18559-afd1-4e81-9477-42547ab3cbab` reached `ready`;
- authenticated `archive/tree` and `archive/file?path=index.html` worked;
- archive file set was incomplete: only `index.html`, `index.inline.html`, and
  `page.txt` were present from the expected core archive files.

This VPS container was still built from an older local tree (`d59ffd0` plus
uncommitted smoke fixes), so this is evidence that the old temporary container
does not exercise the current page archive implementation. It is not evidence
that current `master` page archive support regressed.

Machine-readable evidence:

```text
docs/evidence/public-catalog-page-old-container-2026-06-17.json
```

## Current limitation

The public catalog core media smoke has fresh VPS evidence. The `page_html`
catalog case still needs to be rerun on a clean container built from current
`master` or a local dev server at current HEAD before claiming the MTR/Tsuen Wan
page archive path is fully validated.
