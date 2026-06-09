# Generic Media Discovery Survey 2026-06-09

## Scope

This survey focuses on generic webpage media discovery: finding downloadable
or transcodable media candidates on unknown or newly built public pages without
depending only on a platform rule table.

Out of scope:

- DRM removal or encrypted-media bypass.
- Captcha solving.
- Paywall, login-wall, or access-control bypass.
- Guessing private tokens, signature secrets, or hidden authorization flows.
- Bulk scraping against sites that disallow automation.

The target is a policy-bound candidate discovery pipeline. It should work well
for public pages, owned pages, licensed sources, and pages where the operator
has explicit authorization.

## Reference Projects And Official Docs

| Source | Generic logic | Useful design for Reflection King |
| --- | --- | --- |
| [Chrome DevTools Protocol Network domain](https://chromedevtools.github.io/devtools-protocol/tot/Network/) | Low-level browser network events such as `requestWillBeSent`, `responseReceived`, response body access, cookies, user agent, and service-worker bypass controls. | Add an optional CDP collector beside Playwright `page.on("response")` to capture request ids, initiators, redirects, MIME hints, headers, and small JSON/manifest bodies under strict byte limits. |
| [Playwright network docs](https://playwright.dev/docs/network) and [service worker docs](https://playwright.dev/docs/service-workers) | Browser-context request/response observation, routing, HAR-style network capture, and documented service-worker caveats where page routing may miss SW-owned requests. | Keep the current sidecar model, but add a `serviceWorkerMode` policy: default normal browsing, optional block/bypass mode only for owned test fixtures or authorized diagnostics. Record when service workers may have hidden network events. |
| [`yt-dlp` GenericIE](https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/generic.py) and [`InfoExtractor` helpers](https://github.com/yt-dlp/yt-dlp/blob/master/yt_dlp/extractor/common.py) | Generic extraction from direct links, HTML5 media tags, OpenGraph/Twitter metadata, embeds, JSON-LD, player configs, and HLS/DASH helpers such as HTML5 media parsing and manifest format extraction. | Model generic discovery as layered extractors, not one regex. Add source labels like `dom_media`, `metadata`, `json_ld`, `player_config`, `manifest`, and preserve confidence/evidence per candidate. |
| [Streamlink plugin API](https://streamlink.github.io/api/plugin.html) and [HLS stream implementation](https://github.com/streamlink/streamlink/blob/master/src/streamlink/stream/hls/hls.py) | Plugin turns a page or stream URL into named stream variants; HLS variant playlists are parsed and optionally checked before playback. | Add a manifest parser stage that expands `.m3u8` into variants with bitrate/resolution/duration metadata, while keeping segment fetching and ffmpeg handoff behind Rust policy checks. |
| [`hls.js` playlist loader](https://github.com/video-dev/hls.js/blob/master/src/loader/playlist-loader.ts) | Separates manifest loading, parsing, level details, track details, and network loader behavior. | Treat HLS manifest discovery as a first-class candidate kind with parent/child relationships: master playlist, media playlist, rendition, segment estimate, encryption markers, and parsing errors. |
| [`dash.js` DashParser](https://github.com/Dash-Industry-Forum/dash.js/blob/development/src/dash/parser/DashParser.js) | Parses MPD XML into periods, adaptation sets, representations, base URLs, and segment templates. | Add DASH MPD parsing before ffmpeg selection so candidates can expose audio/video representation metadata and reject protected/unsupported MPD features early. |
| [`gallery-dl` extractor base](https://github.com/mikf/gallery-dl/blob/master/gallery_dl/extractor/common.py) | Extractors share session initialization, cookies, headers, retries, config, parent/child extractors, and archive/dedupe behavior. | Keep cookies and headers in profile-scoped sidecar storage, never in public job views. Add per-extractor config, retry policy, and dedupe keys for repeated public URLs. |
| [mitmproxy addon examples](https://docs.mitmproxy.org/stable/addons/examples/) | HTTP flows expose request/response events and are scriptable for diagnostics. | Do not put a MITM proxy in the normal production path, but use the flow model as inspiration for local diagnostics: request metadata, response metadata, body-size limits, and redacted event logs. |

Supporting browser API references:

- [MDN `HTMLMediaElement`](https://developer.mozilla.org/en-US/docs/Web/API/HTMLMediaElement)
- [MDN `PerformanceResourceTiming`](https://developer.mozilla.org/en-US/docs/Web/API/PerformanceResourceTiming)
- [MDN Media Source Extensions](https://developer.mozilla.org/en-US/docs/Web/API/Media_Source_Extensions_API)
- [MDN Service Worker API](https://developer.mozilla.org/en-US/docs/Web/API/Service_Worker_API)

## Generic Discovery Layers

### 1. Direct URL And HEAD/GET Probe

If the input URL already points to a public media file or manifest, this is the
safest path. Use URL policy first, then a bounded request:

- URL scheme must be HTTP(S).
- DNS and redirects must not resolve to local, private, link-local, or metadata
  networks.
- MIME type, extension, and `Content-Length` are recorded as hints, not trusted
  as proof.
- HTML documents should route into webpage discovery, not direct media download.

Reflection King already has the direct URL path and SSRF policy. The next
improvement is to convert direct URLs into first-class candidates before
fetching, so every source uses the same selection and attempt records.

### 2. Browser Network Sniffing

Use Playwright and optionally CDP to observe real browser activity after page
JavaScript runs:

- request URL, method, resource type, initiator frame, redirects
- response status, MIME type, content length, cache/service-worker hints
- manifest/media extension detection: `.m3u8`, `.mpd`, `.mp4`, `.m4s`, `.m4a`,
  `.mp3`, `.webm`, `.ts`
- small JSON or manifest body capture only under explicit byte limits

This is good for unknown single-page apps, player widgets, and self-built pages
that construct media URLs at runtime. It is weak when the browser only exposes
`blob:` URLs, when a service worker serves cached responses, or when media is
behind protected encrypted-media flows.

Reflection King already uses Playwright `response` events. Add CDP later for
better initiator data, redirect chains, and optional small-body capture.

### 3. HTML Media And Metadata Scanning

Parse the live DOM after page load:

- `<video src>`, `<audio src>`, and nested `<source src type>`
- `<track src>` for subtitles if subtitle support is added
- `<a href>` download/media links
- `<link rel=preload as=audio|video href>`
- OpenGraph/Twitter/media metadata such as `og:video`, `og:audio`,
  `twitter:player:stream`
- `currentSrc`, not only the literal `src` attribute, because browsers may pick
  a source after media selection

This works well for owned/new sites because many players still expose plain
media URLs in the DOM or metadata. The current sidecar already implements most
of this for media, anchors, metadata, and link-preload hints.

### 4. Performance Entries

After playback or page initialization, call `performance.getEntriesByType("resource")`
inside the page. Resource timing can expose URLs that were loaded by fetch/XHR,
media elements, images, scripts, and player libraries.

Useful fields:

- `name`: absolute resource URL
- `initiatorType`: `audio`, `video`, `fetch`, `xmlhttprequest`, `script`,
  `img`, or other initiator names
- `transferSize` and `decodedBodySize`: rough size hints when available

Limits:

- Timing entries can be incomplete or size-redacted by browser privacy rules.
- It is metadata only; it does not preserve request headers or body.
- Cached and service-worker responses can be misleading.

Reflection King already scans performance entries. Store the source label and
size hint in candidate metadata so the selector can rank runtime-discovered
media above thumbnails and UI assets.

### 5. Script And JSON URL Extraction

Many unknown pages put media URLs in app-state JSON or player configs:

- JSON-LD and structured metadata
- `application/json` or `application/ld+json` script tags
- Next.js/Nuxt/SvelteKit data blobs
- inline player configs such as `sources`, `file`, `url`, `hls`, `dash`,
  `audio`, `video`, `manifest`
- escaped URLs like `https:\/\/...`, `\u002F`, `\u0026`, `\u003D`

Engineering rules:

- Parse JSON with bounded depth and item limits.
- Run regex only on bounded text slices.
- Do not execute arbitrary page scripts outside the browser sandbox.
- Treat found strings as candidates only after URL policy and media/manifest
  classification.

Reflection King already scans bounded inline script text and JSON-like data.
The next improvement is weighted key-aware extraction: URLs found under keys
named `source`, `sources`, `media`, `audio`, `video`, `manifest`, `hls`, or
`dash` should score higher than random URL strings.

### 6. Manifest Parsing

HLS and DASH are common on both large platforms and custom sites. Generic
manifest handling should not be delegated blindly to ffmpeg:

- Resolve relative URLs against the manifest URL.
- Parse HLS master playlists, media playlists, variants, renditions, byte-range
  segments, and duration estimates.
- Parse DASH MPD periods, adaptation sets, representations, base URLs, segment
  templates, and segment lists.
- Record encryption/protection markers and reject DRM/protected media paths.
- Validate every child URL with the same SSRF and redirect policy.
- Cap manifest size, variant count, segment count, duration, and total bytes.

Reflection King should add a Rust manifest parser module before selected
candidate processing. ffmpeg can still do final capture/remux/transcode, but
the policy layer must understand what it is about to fetch.

### 7. Blob URLs And Media Source Extensions

`blob:` URLs are not durable source URLs. They usually point to in-memory browser
objects created from fetched segments or Media Source Extensions buffers.

Generic handling:

- Do not return `blob:` as a downloadable candidate.
- Look upstream: network responses, performance entries, manifests, and player
  config JSON that produced the blob.
- If Media Source Extensions are used with public HLS/DASH segments, discover
  the manifest/segments, not the blob.
- If Encrypted Media Extensions or DRM/CENC markers are present, return a clear
  unsupported/protected-media error.

This is important for YouTube-like and modern SPA players. It explains why a
page can visibly play while a generic downloader still has no safe public media
URL.

### 8. Headers, Cookies, Referer, And Session Scope

Discovery and download must preserve enough request context without leaking
secrets:

- Store cookies in browser profile storage, not in public job records.
- When a selected browser candidate is fetched, derive scoped `Cookie`,
  `User-Agent`, `Referer`, and `Origin` from the sidecar for that exact URL.
- Never expose Cookie/Auth headers through candidate APIs.
- Mark whether a candidate likely requires authorization.
- Give candidates a TTL when they contain signed query parameters or short-lived
  URLs.

Reflection King already requests sidecar headers for browser-probe candidates
and passes them to ffmpeg/downloader. The next improvement is candidate expiry
metadata and retry refresh through the same profile.

## Candidate Scoring Model

Generic discovery needs scoring because unknown pages produce noise.

Recommended score inputs:

- Strong positive: `video/audio` response MIME, media resource type, DOM
  `currentSrc`, HLS/DASH manifest, JSON key names like `sources` or `manifest`.
- Medium positive: media file extension, OpenGraph/Twitter stream metadata,
  link preload with `as=audio|video`.
- Negative: tiny `Content-Length`, tracking/ad hosts, UI sound paths, thumbnail
  image only, segment/chunk URLs without parent manifest.
- Confidence evidence: `source`, `initiator`, `status`, `content_type`,
  `content_length`, `parent_manifest_url`, `referer`, `warnings`.

Reflection King should keep returning a candidate list, not auto-download every
candidate. Auto-selection should be limited to single strong direct-file or
single strong manifest cases.

## Recommended Reflection King Architecture

```text
source URL
  -> source policy precheck
  -> discovery plan
       1 direct URL classifier
       2 static HTML/DOM/media scanner
       3 browser network observer
       4 performance entry scanner
       5 script/JSON extractor
       6 manifest parser
       7 optional site extractor
  -> normalized candidates
  -> candidate policy validation
  -> user/auto selection
  -> scoped header lookup
  -> downloader/ffmpeg
  -> artifact + byte-range public media URL
```

Concrete data additions:

- `candidate.source`: `network_response`, `dom_media`, `metadata`, `performance`,
  `script_json`, `manifest_parser`, `site_extractor`
- `candidate.parent_url`: page URL or manifest URL
- `candidate.evidence_json`: response headers summary, DOM selector, JSON key
  path, performance initiator, manifest variant metadata
- `candidate.expires_at`: optional, for signed URLs
- `candidate.protection`: `none`, `encrypted_hls`, `eme_drm`, `unknown`
- `fetch_attempts`: selected candidate, headers source, retry class, bytes read,
  ffmpeg stderr summary

Near-term implementation order:

1. Keep the current generic sidecar discovery and add focused fixture tests.
2. Add CDP network capture for initiators, redirects, and small JSON/manifest
   bodies.
3. Add key-aware JSON extraction and JSON-LD/OpenGraph/Twitter stream handling.
4. Add HLS parser with variant metadata and child URL policy validation.
5. Add DASH MPD parser with representation metadata and protected-media flags.
6. Add candidate TTL/refresh for signed URLs.
7. Only then add site-specific extractors for sources where generic discovery
   has evidence of insufficiency.

## Current Project Mapping

Implemented in `services/reflection-browser/src/probe.ts`:

- Playwright network response candidate sniffing.
- DOM media/source/anchor/metadata discovery.
- Link preload hints.
- Performance resource discovery.
- Bounded inline script and JSON URL extraction.
- Output-aware candidate filtering.
- YouTube UI sound-effect rejection based on observed false positives.
- Bilibili `__playinfo__` parsing as a site-specific supplement.

Implemented in Rust:

- Browser-probe candidates pass URL policy validation before storage.
- Candidate selection is separate from discovery.
- Browser-probe candidates can request scoped headers from the sidecar.
- ffmpeg can transcode selected video/manifest candidates into MP3 for
  audio-only jobs.
- Public artifacts are served with byte-range support.

Not yet implemented:

- CDP collector.
- HLS/DASH manifest parser with child URL validation.
- Protected-media marker handling.
- Candidate expiry and refresh.
- Automated fixture tests for every generic discovery layer.

## Risks And Limits

- Generic discovery can find public media candidates, but it cannot turn every
  visible player into a downloadable URL.
- Blob/MSE playback requires upstream manifest/segment discovery; the blob
  itself is not useful.
- DRM/EME/CENC should return unsupported, not a bypass attempt.
- Service workers can hide or synthesize network responses.
- Signed URLs may expire before selection or ffmpeg fetch.
- Cookies and auth headers must remain profile-scoped and redacted.
- Every discovered URL, manifest child URL, and redirect target must repeat the
  SSRF checks.
- Large manifests and segment lists need hard limits before ffmpeg sees them.

## Conclusion

The strongest generic approach is layered discovery, not a single extractor:
browser network observation, DOM/media scanning, performance entries,
script/JSON URL extraction, and manifest parsing should all feed the same
candidate table. Site rules still matter for complex platforms, but they should
be supplements after the generic pipeline has recorded evidence.
