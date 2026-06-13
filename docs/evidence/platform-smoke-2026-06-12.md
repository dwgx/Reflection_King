# Platform Smoke Evidence 2026-06-12

## Scope

This evidence records real public-platform smoke results against the deployed
Reflection King service at:

```text
http://154.40.36.22:8780
```

The tested URLs are public examples. The smoke process does not bypass DRM,
paywalls, captchas, or login walls. Platform URLs can still fail later when a
site changes signatures, bot checks, regional policy, or copyright availability.

## Tool Research Summary

Main parser path:

- `yt-dlp --dump-single-json --no-playlist --skip-download`

Useful open-source references:

- yt-dlp extractors: https://github.com/yt-dlp/yt-dlp/tree/master/yt_dlp/extractor
- yt-dlp supported sites: https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md
- Streamlink plugins: https://streamlink.github.io/plugins.html
- you-get project: https://github.com/soimort/you-get

Observed CLI behavior on the VPS:

| Platform | yt-dlp | you-get | streamlink | Notes |
| --- | --- | --- | --- | --- |
| YouTube | OK | Failed on sample | OK | yt-dlp remains primary. |
| SoundCloud | OK | Failed on sample | Unsupported | yt-dlp remains primary. |
| Bilibili | Browser/API path | Not part of this smoke | Not part of this smoke | Browser probe finds public DASH candidates. |
| AcFun | OK | Failed on sample | Unsupported | Good platform smoke target. |
| Youku CN | OK | OK | Unsupported | Signed HLS URLs expire; keep in platform tier. |
| Youku international | Unsupported | Cannot fetch vid | Unsupported | Do not use for automatic smoke. |
| iQIYI/iQ.com | OK with PhantomJS | Returned tiny unknown file | Unsupported | Experimental: yt-dlp returns inline HLS manifests that are downloaded through delegated yt-dlp. |
| TikTok | OK for JSON and yt-dlp delegated download | Not used | Not used | Raw CDN URLs returned 403; server now falls back to constrained yt-dlp download. |
| Douyin | OK for some public short videos | Not used | Unsupported for short videos | Some URLs require fresh cookies, not necessarily a logged-in account. |
| Kuaishou | Unsupported or connection reset on samples | Failed on samples | Unsupported | Keep as experimental; current VPS has no reliable automatic path. |

## Smoke Tiers

`scripts/smoke/live_smoke.py` now has tiers:

- `core`: stable generic fixtures. Default tier.
- `platform`: real public platform URLs that can change but should be run
  regularly against staging/VPS.
- `experimental`: known unstable or incomplete platform probes.

Commands:

```powershell
python scripts\smoke\live_smoke.py --list
python scripts\smoke\live_smoke.py --base-url http://154.40.36.22:8780
python scripts\smoke\live_smoke.py --base-url http://154.40.36.22:8780 --tier platform
python scripts\smoke\live_smoke.py --base-url http://154.40.36.22:8780 --case acfun-public-external-video
```

Default `core` now uses `mux-hls-low-video` for HLS video. The previous Apple
`bipbop_16x9_variant.m3u8` sample is about 30 minutes and generated a 402 MB
artifact during testing, so it was moved to `experimental` as a manual stress
case.

## Real Platform Results

Run command:

```powershell
python scripts\smoke\live_smoke.py --base-url http://154.40.36.22:8780 --tier platform --timeout-seconds 320
```

All platform cases completed with at least one artifact and `Range` playback
check returned `206 Partial Content`.

| Case | Source URL | Candidates | Output | Range Check |
| --- | --- | ---: | --- | --- |
| `youtube-bbb-external-video` | `https://www.youtube.com/watch?v=aqz-KE-bpKQ` | 23 | MP4 video | `206`, `video/mp4` |
| `soundcloud-public-audio` | `https://m.soundcloud.com/nasa/apollo-8-merry-christmas` | 4 | MP3 audio | `206`, `audio/mpeg` |
| `bilibili-bbb-browser-video` | `https://www.bilibili.com/video/BV1Fb4111732/` | 18 | MP4 video | `206`, `video/mp4` |
| `acfun-public-external-video` | `https://m.acfun.cn/v/?ac=17529896` | 5 | MP4 video | `206`, `video/mp4` |
| `youku-public-trailer-video` | `https://v.youku.com/v_show/id_XNDgwODM0NjYwNA%3D%3D.html` | 4 | MP4 video | `206`, `video/mp4` |
| `tiktok-public-external-video` | `https://vm.tiktok.com/ZMBNyCU7n/` | 5 | MP4 video | `206`, `video/mp4` |

## VRChat Raw URL Checks

The new AcFun and Youku artifacts were also checked with:

```powershell
python scripts\smoke\vrchat_raw_url_check.py --url <artifact-url>
```

Results:

| Platform | Result |
| --- | --- |
| AcFun | `HEAD 200`, `Range 206`, MP4 faststart, H.264, yuv420p, AAC 2ch |
| Youku | `HEAD 200`, `Range 206`, MP4 faststart, H.264, yuv420p, AAC 2ch |
| TikTok | `Range 206`, MP4 output generated through delegated yt-dlp download fallback |
| Douyin | `Range 206`, MP4 output for the public sample |
| iQIYI/iQ.com | `HEAD 200`, `Range 206`, H.264 MP4 output generated through delegated yt-dlp download |

Both still warn that the current public endpoint is HTTP. PC VRChat can use it
with untrusted URLs enabled, but Android/Quest production use needs HTTPS.

## Site Notes

### AcFun

AcFun is a good automatic platform smoke target. The public sample returns HLS
MP4 candidates through yt-dlp's AcFun extractor. Common future failure causes:
Referer requirements, page JSON changes, region/copyright restrictions, and
signed CDN URLs.

### Youku

Youku CN public trailer pages are usable as platform smoke. They expose signed
HLS URLs through yt-dlp and you-get. Common failure causes: `utid/cna` changes,
`ckey/ccode` changes, Referer/Cookie requirements, VIP/password/region limits,
or CDN segment denial.

### iQIYI

iQIYI/iQ.com is usable for the sampled public trailer after installing the
deprecated PhantomJS compatibility dependency on the VPS. yt-dlp returns
`data:application/x-mpegurl` inline HLS manifests; Reflection King now keeps
only safe inline HLS manifest candidates and delegates the final download back
to yt-dlp using the original page URL and selected `format_id`.

Keep this case in `experimental`, not `platform`, because PhantomJS is obsolete
and iQIYI signatures, regions, VIP pages, and anti-bot checks can drift quickly.
you-get still returned a tiny unknown file rather than a usable media candidate
on the tested page.

Current Reflection King experimental smoke result after the inline HLS change:

```text
case: iqiyi-public-trailer-probe
job: f65f962a-f0c4-43df-a9ef-69d96c823723
status: ready
candidates: 3
selected: 720P manifest, format_id 500
artifact: video/mp4, 11,704,420 bytes
range: 206 video/mp4, content-range bytes 0-1023/11704420
ffprobe: h264, 1280x712, duration about 94s
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 yuv420p, aac 2ch
```

### TikTok

TikTok public short videos can be extracted by yt-dlp on the VPS, but the raw
CDN URLs returned by `--dump-single-json` rejected direct ffmpeg access with
`403 Forbidden` even when replaying safe `User-Agent`, `Accept-Language`, and
`Referer` headers. Reflection King now keeps raw URL handling first, then falls
back to a constrained delegated yt-dlp download for yt-dlp candidates:

- uses the original source page URL, not arbitrary candidate hosts;
- uses `--no-playlist`, `--no-cache-dir`, and `--max-filesize` from
  `RK_MAX_DOWNLOAD_MB`;
- remuxes the downloaded file back into the same server-hosted `/media/...mp4`
  raw URL path.

Current smoke result:

```text
case: tiktok-public-external-video
job: bba78379-3219-495b-91f9-e5523e5477ba
status: ready
candidates: 5
range: 206 video/mp4
```

### Douyin

Douyin is kept in `experimental`, not `platform`, because public short-video
access changes by URL, region, freshness, and challenge-cookie state. The same
short-link can fail through yt-dlp while succeeding through browser probing.
Use browser probing for this sample when a server Profile is available.

```text
case: douyin-public-browser-video
job: c24634df-2850-47b3-8c8a-19545facebb7
status: ready
candidates: 4
artifact: video/mp4, 3,484,984 bytes
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 576x1024, yuv420p, aac 2ch
```

The yt-dlp external route for the same short-link currently fails before
candidates with a clear message:

```text
case: douyin-public-external-video
job: 89443fb6-470a-4f61-87d7-f20fe2123417
status: error
error: Fresh cookies (not necessarily logged in) are needed
```

Another public-looking full Douyin URL also currently fails through yt-dlp:

```text
case: douyin-fresh-cookies-required
job: ea9d45ae-cf7c-4cb1-bc39-654712c24568
status: error
error: Fresh cookies (not necessarily logged in) are needed
```

The server Profile had Douyin cookies during this run, but yt-dlp still
rejected those URLs as not fresh enough. That points to challenge/freshness
state rather than complete absence of cookies.

### Kuaishou

Kuaishou is usable through browser probing on the tested sample, but the CDN
URL is short-lived. A candidate selected several hours after discovery failed
in ffmpeg. Fresh discovery followed by immediate selection/remuxing succeeded.
yt-dlp is unsupported for this URL, you-get exits with its generic failure
message, and streamlink is unsupported for this short-video page.

Current fresh smoke results:

```text
case: kuaishou-public-auto-probe
job: e62b1a4f-eb4e-4a98-94a2-fafa60c51cde
status: ready
candidates: 1
artifact: video/mp4, 40,406,603 bytes
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 1280x720, yuv420p, aac 2ch

case: kuaishou-public-browser-video
job: 32bbb941-3891-4de6-bb72-ed4137501580
status: ready
candidates: 1
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 1280x720, yuv420p, aac 2ch
```

## User-Supplied URL Regression, 2026-06-13

These are real URLs supplied during live testing. The goal is to separate
working extraction from "candidate was visible but not actually reusable".

### Youku

```text
url: https://v.youku.com/v_show/id_XNTk0Njk3NDI2MA==.html
case: user-youku-series-vshow
job: a955116c-53de-473d-be43-f30eaa0a0b98
status: ready
candidates: 3 HLS manifests, top advertised 720p
artifact: video/mp4, 214,304,216 bytes
raw url: http://154.40.36.22:8780/media/a955116c-53de-473d-be43-f30eaa0a0b98/video-ee4b4698-cc55-4a49-beaf-0167bfa84402.mp4
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 960x540, yuv420p, aac 2ch
```

The output passed the raw URL checks, but the final file probed as 540p even
though a 720p manifest was listed first. This should be tracked as a yt-dlp
delegated format selection issue for long Youku HLS jobs.

### AcFun

```text
url: https://www.acfun.cn/v/ac48589257
case: user-acfun-ac48589257
job: bdd3c72b-5e79-4c95-a656-2a39c9258dfa
status: ready
candidates: 7 HLS manifests, top candidate 3840p
artifact: video/mp4, 99,652,086 bytes
raw url: http://154.40.36.22:8780/media/bdd3c72b-5e79-4c95-a656-2a39c9258dfa/video-aed718c2-72b1-49a9-abe0-99c563077dc7.mp4
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 2160x3840, yuv420p, aac 2ch
```

### iQIYI

```text
url: https://www.iqiyi.com/v_2dkhwocyjk4.html
case: user-iqiyi-v-2dkhwocyjk4
job: 7f57acb6-6e28-446a-b94d-4469c34bf22b
status: error
error: yt-dlp [iqiyi] Can't find any video
```

This is different from the earlier iQ.com trailer success. Treat this
`www.iqiyi.com/v_...` page as a separate adapter target rather than assuming
the iQ.com inline-HLS path covers it.

### Generic Episode Sites

The browser sidecar now scans inline script candidates before clicking generic
play controls. For episode-list sites this prevents a false navigation from
episode 1 to episode 2 when the page already exposes `player_aaaa.url`.

```text
url: https://www.dmttang.com/vodplay/872-14-1.html
sidecar finalUrl: https://www.dmttang.com/vodplay/872-14-1.html
title: episode 01
candidates: 2 inline m3u8 values from player_aaaa
job: 47488be3-8d85-4b64-ad49-a32b6c54daec
status: error
error: both m3u8 candidates returned 404 from the CDN, even with browser UA and Referer

url: https://www.83dm.com/yinghua_9334-2-1.html
sidecar finalUrl: https://www.83dm.com/yinghua_9334-2-1.html
title: episode 01
candidates: 2 inline m3u8 values from player_aaaa
job: 42ad9145-d294-4cb1-8307-d2a0145783b8
status: error
error: both m3u8 candidates returned 403 from the CDN, even with browser UA and Referer
```

Conclusion: these pages are now classified correctly as episode pages and no
longer drift to the wrong episode during probing. They are not yet successful
downloads because the surfaced CDN manifests are not reusable from the server.
The next implementation target is a dedicated generic episode adapter that
validates each route's manifest before selection and records the route/episode
metadata in the candidate explanation.

### Hanime1

```text
url: https://hanime1.me/watch?v=406643
sidecar observation before Cloudflare state changed: 1080p/720p/480p complete MP4 candidates from vdownload.hembed.com were visible
job: 2edf22ee-53f4-4814-b9d9-eedef67a674b
status: error
error: no media candidates from chain [browser_probe]

url: https://hanime1.me/watch?v=406627
job: 166f297d-dd06-445a-a138-964c0fd0f84f
status: error
error: no media candidates from chain [browser_probe]
```

The final sidecar runs landed on `Attention Required! | Cloudflare`, so Hanime1
must stay experimental. The sidecar has a Hanime1 filter that prefers complete
`hembed.com` MP4 files over HLS fragments when the page is reachable, but the
remaining blocker is Cloudflare challenge handling rather than candidate
scoring.

## Follow-Up Fix Verification, 2026-06-13

Changes verified in this round:

- Added `hanime1` dedicated mobile-HTML extractor ahead of browser probing.
- Added `mac_cms` extractor for `player_aaaa` episode pages.
- MacCMS candidates now validate HLS with GET master playlist, GET variant
  playlist, and Range GET of the first segment instead of trusting HEAD.
- iQIYI browser stream noise from `static-d.iqiyi.com/lequ` is filtered.
- iQIYI browser manifests that are visible but blocked on segment replay are
  marked as failed candidates with an explicit reason instead of being offered
  as normal usable resources.

### Hanime1

```text
url: https://hanime1.me/watch?v=406643
job: c82fae2f-9395-4468-912b-0255778e650d
status: ready
candidates: 1080p, 720p, 480p MP4 from vdownload.hembed.com
artifact: video/mp4, 115,076,887 bytes
raw url: http://154.40.36.22:8780/media/c82fae2f-9395-4468-912b-0255778e650d/video-1992fdc5-1ddb-4e37-a911-c9405b763830.mp4
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 1080x1440, yuv420p, aac 2ch

url: https://hanime1.me/watch?v=406627
job: a19545d5-bab4-41fe-8455-30d841a54be3
status: ready
candidates: 1080p, 720p, 480p MP4 from vdownload.hembed.com
artifact: video/mp4, 45,890,335 bytes
raw url: http://154.40.36.22:8780/media/a19545d5-bab4-41fe-8455-30d841a54be3/video-e2304ae4-750a-41f9-8db4-be60facb07ee.mp4
vrchat_raw_url_check: HEAD 200, Range 206, faststart OK, h264 1620x1080, yuv420p, aac 2ch
```

Hanime1 is now working for these two supplied pages when the mobile page is
reachable. It still remains experimental because Cloudflare can return
`Attention Required` / HTTP 403; the extractor now reports that state clearly
instead of surfacing a generic browser no-candidate failure.

### MacCMS Episode Sites

```text
url: https://www.dmttang.com/vodplay/872-14-1.html
job: 959085e8-7aef-48d8-9bfa-5ab90b2eb112
resolved extractor: mac_cms>yt_dlp>you_get>streamlink>browser_probe
candidates: 2
state: region_blocked
failure: cdn region blocked: HTTP 404 Not Found
route: mac_cms/lzm3u8/current and mac_cms/lzm3u8/next
```

`dmttang` parsing is now correct: `player_aaaa` is read directly and no generic
playback click changes the episode. The current route is not reusable from the
VPS because the CDN returns a regional 404 page.

```text
url: https://www.83dm.com/yinghua_9334-2-1.html
job: ebac21b2-4f19-40b6-9078-f206ff301069
resolved extractor: mac_cms>yt_dlp>you_get>streamlink>browser_probe
candidates: 2
state: region_blocked
failure: cdn region blocked: HTTP 403 Forbidden
route: mac_cms/dyttm3u8/current and mac_cms/dyttm3u8/next
```

From the local Windows network, `vip.dytt-cine.com` returned a playable master
m3u8. From the VPS, every tested Referer variant returned HTTP 403 with a region
block page. The application now marks these candidates as `region_blocked`, so
the UI does not present them as ordinary selectable resources.

### iQIYI Domestic Page

```text
url: https://www.iqiyi.com/v_2dkhwocyjk4.html
job: 77b29304-29de-4b99-8484-d4bc8f26073f
resolved extractor: yt_dlp>you_get>streamlink>browser_probe
candidates: 1
candidate: https://meta-cdn.video.iqiyi.com/...m3u8
state: failed
protection: needs_profile
failure: iQIYI browser manifest segment replay is blocked by QWS 403; needs dedicated iQIYI runtime signature adapter
```

The browser can observe a signed iQIYI manifest, but server-side ffmpeg cannot
replay the first TS segment: direct checks return QWS HTTP 403 even with browser
UA and Referer, and Profile header replay did not fix it. This is now classified
as a dedicated iQIYI runtime/signature adapter gap rather than a successful
media candidate. The earlier `static-d.iqiyi.com/lequ` 5-second MP4 false
positive is filtered out.
