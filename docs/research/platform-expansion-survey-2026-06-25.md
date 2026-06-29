# Platform Expansion Survey - 2026-06-25

This note records the internet research behind the 2026-06-25 platform hint
expansion. It is not smoke evidence. A platform is only considered verified
after a current Reflection King build creates candidates, produces an artifact,
and passes media Range or page archive checks.

## Sources Checked

- yt-dlp supported sites:
  `https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md`
  - The upstream page says its list reflects included extractors, generic/embed
    extraction may support more sites, and the only reliable support check is to
    try a site because websites constantly change.
- Streamlink plugins:
  `https://streamlink.github.io/plugins.html`
  - Best fit for live and manifest workflows. Do not rely on it as the primary
    VOD extractor when yt-dlp already has a maintained extractor.
- you-get project:
  `https://github.com/soimort/you-get`
  - Useful secondary adapter for some Chinese and legacy sites.
- gallery-dl supported sites:
  `https://github.com/mikf/gallery-dl/blob/master/docs/supportedsites.md`
  - Strong fit for image/gallery/social collections. The upstream list warns
    that listed sites should be treated as potentially NSFW, so fixtures must be
    curated.
- Spotify Web API policy:
  `https://developer.spotify.com/documentation/web-api`
  - Spotify should stay metadata/page-only in this project. Do not add media
    download extraction for Spotify links.
- Internet Archive Wayback APIs:
  `https://archive.org/help/wayback_api.php`
  - Official entry point for Availability, Memento, and CDX APIs.
- Internet Archive CDX server docs:
  `https://github.com/internetarchive/wayback/blob/master/wayback-cdx-server/README.md`
  - Documents CDX query fields, filters, match types, output formats, paging,
    and `resumeKey`.
- Memento RFC 7089:
  `https://datatracker.ietf.org/doc/html/rfc7089`
  - Defines TimeGate, TimeMap, Memento, `Accept-Datetime`, `Memento-Datetime`,
    and `Link` relations for time-based web access.
- MementoWeb quick intro and status:
  `https://mementoweb.org/guide/quick-intro/`
  `https://mementoweb.org/about/`
  - Useful protocol background. The Time Travel aggregator is legacy/sunset, so
    Reflection King should not depend on it as a core backend.
- Archive-It CDX/C API:
  `https://support.archive-it.org/hc/en-us/articles/115001790023-Access-Archive-It-s-Wayback-index-with-the-CDX-C-API`
  - Collection-scoped CDX queries are the safer default; the `/all` endpoint is
    not a reliable core dependency.
- Perma.cc developer docs:
  `https://perma.cc/docs/developer`
  - Public Archives endpoint is available without an API key, but most API
    management endpoints require explicit user credentials.
- WebCitation:
  `https://www.webcitation.org/`
  - Legacy read-only target. The service says it no longer accepts archiving
    requests.
- Ghostarchive:
  `https://ghostarchive.org/`
  `https://ghostarchive.org/about.html`
  - Public archive/search UI and ReplayWeb.page-style replay. Treat as
    `page_html` first; no stable public JSON API was identified.
- archive.today/archive.ph:
  `https://archive.ph/faq.whatsapp.com`
  - Public search/list pattern evidence only. No stable official JSON API was
    identified; use cautious `page_html` observation.

## Expansion Decision

This pass adds low-risk platform hints, URL inference, browser filtering, UI
labels, login shortcuts, cookie-domain allowlists, and a new smoke catalog. It
does not add a gallery-dl binary adapter yet because gallery-dl output semantics
need a separate candidate model for image sets, albums, pagination, auth, and
NSFW filtering. Wiring it through the existing yt-dlp/you-get/lux/streamlink
JSON scanner would make false positives too easy.

## Platform Groups

P0/P1 targets now recognized by `PlatformHint`:

- Core video/audio: YouTube, Bilibili, SoundCloud, Vimeo, Dailymotion, Rumble,
  PeerTube, Archive.org, Wikimedia Commons.
- Live/manifest: Twitch and direct HLS/DASH URLs, with Streamlink as a likely
  future helper for live pages.
- Short video/social: TikTok, Douyin, Kuaishou, X/Twitter, Reddit, Instagram,
  Facebook public video.
- Image/gallery: Pinterest, Imgur, Flickr, Wikimedia Commons.
- Audio/community: Bandcamp, Mixcloud, Ximalaya.
- CN/JP/legacy: AcFun, Youku, iQIYI, Weibo, Niconico, FC2.
- Metadata-only: Spotify.
- Web archives/time machines: Wayback, Archive-It, Perma.cc, archive.today,
  Ghostarchive, WebCitation, Memento.

## Web Archive Boundary

The first web-archive pass adds platform hints and browser `page_html` routing
for public archive services. It deliberately does not add "save this page into
an archive service" behavior, because that has external side effects and should
only happen after an explicit user action.

Default behavior by provider:

- Wayback: recognize `web.archive.org/web/...` and `archive.org/web/...`; future
  structured extractor should use Availability/CDX before any broad crawl.
- Archive-It: recognize `archive-it.org` and `wayback.archive-it.org`; future
  CDX/C integration must be collection-scoped.
- Perma.cc: recognize public Perma links and public archive API pages; default
  to `page_html`.
- archive.today/archive.ph mirrors: recognize known mirror hosts; default to
  `page_html` and do not run unbounded search or JS-heavy replay as a media
  extractor.
- Ghostarchive: recognize archive pages; default to `page_html`; do not assume
  the presence of downloadable video.
- WebCitation: legacy read-only page parser target only.
- Memento: recognize MementoWeb/timegate-style URLs as protocol hints; future
  parser should read `Link` headers and TimeMap responses.

Media extraction should only be promoted for web archives when structured
metadata or the captured resource MIME type explicitly indicates a public
`image/*`, `audio/*`, `video/*`, PDF, document, or archive file. Replay HTML
itself stays a page archive target.

## Safe Fixture Rules

- Use public, no-login, no-DRM, non-adult, non-private, non-paywalled samples.
- Prefer official, public-domain, educational, open-media, or platform demo
  sources.
- For social/image sites, start with `page_html` observation until a stable,
  safe media URL fixture is chosen.
- For broad smoke, use explicit low bitrate/quality where possible so one
  platform cannot consume the full download budget.
- Keep unstable platforms as `experimental` and `expect_success: false` until a
  current public service run proves them.

## Next Implementation Steps

1. Add a dedicated gallery/image adapter instead of overloading the existing
   external-tool JSON scanner.
2. Add a low-cost `internet-platform-expansion` smoke run against
   `https://rk.dwgx.top` and record JSON evidence under `docs/evidence/`.
3. Promote only green, repeatable fixtures into the regular platform catalog.
4. For failures, classify them as `needs_profile`, `unsupported`, `policy`,
   `timeout`, or `tool drift` rather than treating all failures as parser bugs.
