# VRChat Playback

VRChat players need a public media URL. Localhost URLs and private LAN URLs are not reachable by other clients.

Official references:

- VRChat video players: https://creators.vrchat.com/worlds/udon/video-players/
- VRChat video player allowlist: https://creators.vrchat.com/worlds/udon/video-players/www-whitelist/

## Raw Media URLs

The API serves generated media as unauthenticated raw URLs:

```text
/media/<job-id>/<artifact-filename>
```

with:

- `Content-Type: audio/mpeg`
- `Content-Type: video/mp4`
- `Accept-Ranges: bytes`
- `Content-Range` on satisfiable Range requests
- `Content-Length`
- CORS enabled for simple browser use
- `HEAD` support for player and proxy probes

The API supports one byte range per request, which covers common browser and
VRChat player probes for large generated audio/video.

## Video Player Compatibility Target

For a raw URL that is meant for a VRChat video player, prefer:

- Direct `.mp4` URL, not an HTML page.
- MP4 with `-movflags +faststart`.
- H.264 video, `yuv420p` pixel format.
- AAC audio, or MP3 for audio-only artifacts.
- Public `http` or `https` URL with no API key, cookie, or redirect requirement.
- `https` if the URL is meant to work on Android/Quest.

The current public IP endpoint is useful for PC testing:

```text
http://154.40.36.22:8780
```

Because it is not a VRChat allowlisted host, users need to enable `Allow
Untrusted URLs` in VRChat settings. VRChat on Android requires HTTPS for
non-allowlisted hosts, so production VRChat use should put the service behind a
real domain and TLS.

## Self Check

Run the raw URL check against one or more generated artifacts:

```powershell
python scripts\smoke\vrchat_raw_url_check.py `
  --url "http://154.40.36.22:8780/media/<job-id>/<artifact>.mp4"
```

Or check every artifact on a job:

```powershell
$env:RK_API_KEY = "<user-or-admin-key>"
python scripts\smoke\vrchat_raw_url_check.py `
  --base-url "http://154.40.36.22:8780" `
  --job-id "<job-id>"
```

The check verifies:

- `HEAD` returns `200`.
- `GET` with `Range: bytes=0-511` returns `206`.
- `Accept-Ranges`, `Content-Length`, `Content-Range`, and MIME type are present.
- MP4 has a `moov` atom before media data.
- MP4 video is H.264 and audio is AAC/MP3.
- MP3 audio-only artifacts contain an MP3 audio stream.

## Important Constraints

- Non-allowlisted domains require users to enable untrusted URLs.
- Android/Quest requires HTTPS for non-allowlisted hosts.
- Public worlds can apply stricter URL and sync rules.
- Some VRChat video players reject audio-only files; generate MP4 with a still image and audio track when a world requires video-player playback.
- Long audio should be tested in the actual target world/player.
- Do not trigger multiple video URL loads faster than VRChat's rate limit.
