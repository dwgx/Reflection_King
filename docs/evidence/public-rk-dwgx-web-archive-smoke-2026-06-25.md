# Public rk.dwgx.top web archive smoke evidence 2026-06-25

Base URL under test: `https://rk.dwgx.top`.

This run used a temporary low-privilege user API key capped at 300 MiB with browser/yt-dlp/external adapters enabled and login Profile disabled. The key was revoked after the run; revocation was verified with `HTTP 401` on `/api/capabilities`.

Before the smoke run, the public Docker container was rebuilt from the current workspace and restarted with `docker compose up -d --build reflection-king`. The runtime public base URL was corrected to `https://rk.dwgx.top` before creating jobs; `/api/capabilities` reported 41 platform hints including `wayback` and `memento`.

## Result

- Web archive cases passed: `8 / 8`.
- Every case reached `ready`.
- Every case produced the required page archive files: `index.html`, `index.inline.html`, `metadata/resources.json`, `page.txt`, and `preview/screenshot.png`.
- Archive file samples were read back through public `https://rk.dwgx.top/api/jobs/.../archive/file` URLs with authenticated smoke requests.

| Case | Job | Files | Sample checks |
| --- | --- | ---: | --- |
| `wayback-example-page-html-hint` | `12300fdf-013e-4501-8c60-5890eca0edf8` | 24 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `wayback-cdx-api-hint` | `bf18b76d-c22c-4949-a419-71b4a26ec80e` | 8 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `archive-it-cdx-collection-hint` | `a2fa9a4f-c261-4069-9ca6-0ad3efd08ddc` | 8 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `perma-public-archives-api-hint` | `fa6de583-c520-4e93-9fa3-2723625783b1` | 18 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `archive-today-search-page-hint` | `71daeddf-89c4-4d49-a8b4-921b62ad05cf` | 43 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `ghostarchive-public-archive-page-hint` | `2950ff33-d1a6-4b82-be01-18d4ce0907cc` | 40 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `webcitation-legacy-page-hint` | `1b639e6e-4498-4f40-aa6e-7c578296f2bf` | 21 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |
| `memento-timegate-doc-page-hint` | `9f32e515-783c-42bb-b6a1-22a01cbfa880` | 18 | index.html 200 text/html; charset=utf-8, metadata/resources.json 200 application/json, preview/screenshot.png 200 image/png |

## Interpretation

- `wayback`, `archive_it`, `perma_cc`, `archive_today`, `ghostarchive`, `webcitation`, and `memento` are now verified as public `page_html` archive capture routes on the deployed public service.
- These results do not claim media extraction support for replay pages. The current verified behavior is page archive capture and public archive-file readback.
- Dedicated structured extractors are still the next step for API/protocol metadata such as Wayback CDX rows, Archive-It CDX/C, Memento Link/TimeMap, and Perma public archive records.

Machine-readable evidence: `docs/evidence/public-rk-dwgx-web-archive-smoke-2026-06-25.json`.
