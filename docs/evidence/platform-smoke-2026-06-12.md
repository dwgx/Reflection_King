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
