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
| iQIYI/iQ.com | Failed without PhantomJS | Returned tiny unknown file | Unsupported | Keep as experimental/manual only. |

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

iQIYI is not ready for automatic success smoke. Current VPS probe shows yt-dlp
needs PhantomJS for the sampled iQ.com pages, and you-get returned a tiny
unknown file rather than a usable media candidate. Keep it in `experimental`
until extractor dependencies and failure classification are improved.

Current Reflection King experimental smoke result:

```text
case: iqiyi-public-trailer-probe
job: 2bf3da84-7197-4a22-b10b-fc42cfa06049
status: error
error: remote source error: no media candidates from chain [yt_dlp]
```
