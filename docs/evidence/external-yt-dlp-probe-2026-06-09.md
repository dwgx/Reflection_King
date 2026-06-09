# External yt-dlp Probe Evidence - 2026-06-09

## Scope

This evidence covers the constrained external extractor path. The backend calls
`yt-dlp --dump-single-json --no-playlist --skip-download` to produce media
candidates only. Reflection King still owns URL policy checks, download limits,
transcoding, artifact storage, and public playback URLs.

No cookies, authorization headers, DRM keys, captcha solving, paid access, or
login-wall bypass were used.

## Server

- Repository: `https://github.com/dwgx/Reflection_King`
- Runtime commit: `f303344`
- Deployment path: `/opt/reflection-king`
- `yt-dlp` version: `2026.03.17`
- `reflection-api`: active
- `reflection-browser`: active
- `nginx`: active

## YouTube Public Sample

- URL: `https://www.youtube.com/watch?v=xSZqX5Io6AY`
- Discovery: `external`
- Platform hint: `youtube`
- Outputs: `audio`
- Job ID: `d6c7ef29-163b-461b-8827-7ee434454604`
- Final status: `candidates_ready`
- Candidate count: `17`

First ranked candidate after audio-only scoring:

| field | value |
| --- | --- |
| kind | `audio` |
| extractor | `yt_dlp` |
| method | `dump_single_json` |
| resource_type | `https` |
| quality_label | `English (US) original (default), medium` |
| score | `123` |
| content_type | `audio/mp4` |
| content_length | `9902879` |
| requires_authorization | `false` |

Conclusion: the external path can discover real YouTube media candidates for
this public sample without treating YouTube UI sound effects as media.

## SoundCloud Public Sample

- URL: `https://soundcloud.com/flowkingbrave/baddie`
- Discovery: `external`
- Platform hint: `soundcloud`
- Outputs: `audio`
- Job ID: `bc112f2e-ed2b-4171-8d23-a07c93ee8a6c`
- Final status: `candidates_ready`
- Candidate count: `4`

First ranked candidate:

| field | value |
| --- | --- |
| kind | `audio` |
| extractor | `yt_dlp` |
| method | `dump_single_json` |
| resource_type | `http` |
| quality_label | `128k audio` |
| score | `123` |
| content_type | `audio/mpeg` |
| content_length | `2140656` |
| requires_authorization | `false` |

Conclusion: the external path can discover real SoundCloud audio candidates for
this public sample.

## Bilibili Public Sample

- URL: `https://www.bilibili.com/video/BV1AUkBBpELC`
- Discovery: `external`
- Platform hint: `bilibili`
- Outputs: `audio`
- Job ID: `d448051d-05ed-45d0-bfea-d23c87861c36`
- Final status: `error`
- Error: `yt-dlp probe exited with 1: ERROR: [BiliBili] 1AUkBBpELC: Unable to download webpage: HTTP Error 412: Precondition Failed`
- Candidate count: `0`

Conclusion: the current external `yt-dlp` path does not work for this Bilibili
sample from the server environment. Keep the browser sidecar Bilibili
`__playinfo__` extractor as the working path for this sample.

## Engineering Notes

- The deprecated `--no-call-home` yt-dlp option was removed after a real server
  run showed it polluting stderr.
- Audio-only jobs now rank audio candidates above video candidates while still
  keeping video/manifest candidates available for manual selection.
- Candidates carrying sensitive `Cookie`, `Authorization`, or `x-*` headers are
  marked `requires_authorization`; yt-dlp candidates with such requirements are
  rejected at processing time because Reflection King does not persist external
  extractor headers.
