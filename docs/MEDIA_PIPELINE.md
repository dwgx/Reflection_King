# Media Pipeline

## MVP

The current MVP accepts a direct media URL and emits MP3:

```text
source URL -> SQLite job -> download temp/input.media -> ffmpeg -> public/audio.mp3
```

Supported in practice depends on the local `ffmpeg` build. Common sources include `.m4a`, `.mp3`, `.aac`, `.wav`, `.mp4`, and `.webm`.

Generated MP3 files are served with single-range HTTP byte support for browser,
VRChat, and proxy playback probes.

## Planned Source Types

- Direct file URL.
- HLS live or VOD (`.m3u8`).
- DASH (`.mpd`).
- Platform extractor output from tools such as `yt-dlp`.
- User-uploaded file.

## Planned Output Profiles

- `audio_mp3_vrc`: MP3, 44.1 kHz, stereo, 128-320k.
- `audio_aac`: AAC for browser/player compatibility.
- `video_mp4_faststart`: H.264/AAC MP4.
- `hls_vod`: segmented output for long videos.
- `thumbnail`: still image extraction.

## Do Not Skip

- `ffprobe` metadata before processing.
- Duration limits.
- Segment and live capture timeouts.
- Queue-level concurrency limits.
- Retention policy.
- User-visible error mapping.
