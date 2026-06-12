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
access changes by URL, region, freshness, and challenge-cookie state. The tested
public short-link sample currently works without login on the VPS:

```text
case: douyin-public-external-video
job: 826f3fc5-ac19-46e4-9953-272057dc3b5e
status: ready
candidates: 13
range: 206 video/mp4
```

Another public-looking Douyin URL currently fails before candidates with a
clear yt-dlp message:

```text
case: douyin-fresh-cookies-required
job: 4832662e-132f-458f-acf9-420bb5e9fc5c
status: error
error: Fresh cookies (not necessarily logged in) are needed
```

This means browser/Profile Cookie import is the next path to test for that URL;
it does not prove that a full logged-in account is always required.

### Kuaishou

Kuaishou remains unsupported on the current VPS samples. yt-dlp either treats
the URL as generic and gets connection reset or unsupported, you-get exits with
its generic failure message, streamlink is unsupported, and the browser probe did
not recover a usable media candidate.

Current smoke result:

```text
case: kuaishou-public-auto-probe
job: fdd12ecb-79f5-4109-b2f6-a618339a8a75
status: error
chain: yt_dlp, you_get, streamlink, browser_probe
```
