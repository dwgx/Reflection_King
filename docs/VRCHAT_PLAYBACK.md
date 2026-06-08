# VRChat Playback

VRChat players need a public media URL. Localhost URLs and private LAN URLs are not reachable by other clients.

## Current MP3 Output

The API serves:

```text
/media/<job-id>/audio.mp3
```

with:

- `Content-Type: audio/mpeg`
- `Accept-Ranges: bytes`
- `Content-Range` on satisfiable Range requests
- CORS enabled for simple browser use

The API supports one byte range per request, which covers common browser and
VRChat player probes for large generated audio.

## Important Constraints

- Non-whitelisted domains may require users to enable untrusted URLs.
- Public worlds can apply stricter URL rules.
- Some players handle direct MP3 better than direct M4A.
- Long audio should be tested in the actual target world/player.
- If a VRC player rejects audio-only files, generate a simple MP4 with a still image and audio track.
- For MP4 outputs, use `-movflags +faststart` so playback can begin before the full file downloads.
