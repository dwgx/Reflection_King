# Generic browser discovery evidence - 2026-06-09

## Scope

This evidence covers unknown-page media discovery that does not rely on a
predefined platform extractor. The fixture is a local synthetic page that acts
like a newly written site: media URLs appear in DOM media tags, runtime media
loads, and inline script data.

No login, captcha, paywall, DRM, or access-control bypass is involved.

## Implementation

The browser sidecar now collects candidates from these generic sources:

- Browser network responses.
- `video`, `audio`, and nested `source` DOM elements.
- `a[href]` media/download links.
- `meta[property]` and `meta[name]` fields containing audio/video/image URLs.
- `performance.getEntriesByType("resource")` runtime resource names.
- Inline script text containing HTTP(S), root-relative, or dot-relative media
  and manifest URLs.

For audio jobs, the sidecar allows audio, video, and manifest candidates. The
Rust backend then extracts MP3 from a selected video or manifest candidate when
the job requests only `audio`.

## Local fixture

Fixture characteristics:

- HTML title: `Unknown Media Fixture`
- `<audio><source src="/media/tone.m4a" type="audio/mp4"></audio>`
- `<video src="/media/clip.mp4"></video>`
- `<a href="/media/tone.m4a" download>`
- `<meta property="og:video" content="/media/clip.mp4">`
- Inline script data:
  `window.__MEDIA_DATA__ = { stream: "./media/clip.mp4", inlineAudio: "./media/inline-only.m4a" }`
- Runtime media load:
  `new Audio('/media/tone.m4a?from=runtime')`

Local sidecar probe for `outputs=["audio"]` produced:

- `video`, source `dom_video`, URL path `/media/clip.mp4`
- `audio`, source `dom_audio`, URL path `/media/tone.m4a`
- `audio`, source `dom_audio`, URL path `/media/tone.m4a?from=runtime`
- `audio`, source `inline_script_url`, URL path `/media/inline-only.m4a`

Local sidecar probe for `outputs=["video"]` produced:

- `video`, source `dom_video`, URL path `/media/clip.mp4`

## SSRF boundary

The local fixture used `127.0.0.1` URLs, so it intentionally only proves
sidecar discovery behavior. Rust API end-to-end fetching is not tested against
loopback URLs because the URL policy correctly blocks loopback/private
addresses.

Production or integration end-to-end tests must use a policy-allowed host.

## Current conclusion

The sidecar now has a generic discovery layer that can find media on simple
unknown pages without a prewritten site extractor. Platform extractors are still
useful for complex sites, but they are no longer the only discovery mechanism.

## Production end-to-end fixture

Validation time: 2026-06-09.

A temporary public static fixture was served from the production host on a
non-default test port. The fixture used the same page structure as the local
unknown-page fixture and was removed from the repository scope; it was not
committed.

Production job:

- Job ID: `3455f498-a290-4105-9552-52c8da00ad06`
- Discovery: `browser`
- Platform hint: `auto`
- Outputs: `audio`
- Page title: `Unknown Media Fixture`

Filtered candidate order:

| kind | source | path | score |
| --- | --- | --- | --- |
| audio | `dom_audio` | `/media/tone.m4a` | 170 |
| audio | `dom_audio` | `/media/tone.m4a` | 170 |
| audio | `inline_script_url` | `/media/inline-only.m4a` | 156 |
| video | `dom_video` | `/media/clip.mp4` | 125 |

The test intentionally selected the `dom_video` candidate while the job requested
only `audio`. The Rust backend extracted MP3 audio from the selected video URL.

Generated artifact:

- Filename: `audio-d5843769-1bc9-4fee-b7df-80c6aea5a063.mp3`
- Content type: `audio/mpeg`
- Bytes: `26375`

Public playback response:

```text
HEAD /media/.../audio-d5843769-1bc9-4fee-b7df-80c6aea5a063.mp3
HTTP/1.1 200 OK
Content-Type: audio/mpeg
Content-Length: 26375
Accept-Ranges: bytes
```

```text
GET /media/.../audio-d5843769-1bc9-4fee-b7df-80c6aea5a063.mp3
Range: bytes=0-1023

HTTP/1.1 206 Partial Content
Content-Length: 1024
Content-Range: bytes 0-1023/26375
```

Conclusion: the production deployment can discover media on an unknown public
page, return audio-prioritized candidates, extract MP3 from a selected video
candidate for an audio-only job, and serve the result with byte-range support.
