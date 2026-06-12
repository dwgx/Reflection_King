#!/usr/bin/env python3
"""Check Reflection King raw media URLs for VRChat-style playback.

This script intentionally checks public media URLs without sending an API key.
It validates the HTTP behavior VRChat video players commonly need: direct file
URLs, byte ranges, streamable MP4 layout, and conservative codecs.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any


RANGE_SAMPLE_SIZE = 512
FASTSTART_SAMPLE_SIZE = 1024 * 1024


@dataclass
class Finding:
    level: str
    url: str
    check: str
    detail: str


def read_secret(path: str | None) -> str | None:
    if not path:
        return None
    with open(path, "r", encoding="utf-8") as handle:
        return handle.read().strip()


def request(
    method: str,
    url: str,
    *,
    headers: dict[str, str] | None = None,
    timeout: int = 30,
    read_bytes: int | None = None,
) -> tuple[int, dict[str, str], bytes]:
    request_headers = dict(headers or {})
    req = urllib.request.Request(url, method=method, headers=request_headers)
    with urllib.request.urlopen(req, timeout=timeout) as response:
        body = response.read(read_bytes) if read_bytes is not None else response.read()
        return (
            response.status,
            {key.lower(): value for key, value in response.headers.items()},
            body,
        )


def fetch_artifact_urls(base_url: str, api_key: str, job_id: str) -> list[str]:
    url = f"{base_url.rstrip('/')}/api/jobs/{job_id}/artifacts"
    status, _, body = request("GET", url, headers={"x-api-key": api_key})
    if status != 200:
        raise RuntimeError(f"artifact lookup returned HTTP {status}: {url}")
    artifacts = json.loads(body.decode("utf-8"))
    return [artifact["media_url"] for artifact in artifacts if artifact.get("media_url")]


def add(finding: list[Finding], level: str, url: str, check: str, detail: str) -> None:
    finding.append(Finding(level=level, url=url, check=check, detail=detail))


def is_probably_html(sample: bytes) -> bool:
    stripped = sample[:128].strip().lower()
    return stripped.startswith(b"<!doctype") or stripped.startswith(b"<html")


def check_http(url: str, findings: list[Finding]) -> dict[str, str]:
    parsed = urllib.parse.urlparse(url)
    path = parsed.path.lower()
    if parsed.scheme not in {"http", "https"}:
        add(findings, "FAIL", url, "scheme", "URL must use http or https")
    if not path.endswith((".mp4", ".webm", ".mp3")):
        add(findings, "WARN", url, "extension", "VRChat works best with direct .mp4/.webm video URLs")
    if parsed.scheme != "https":
        add(
            findings,
            "WARN",
            url,
            "https",
            "PC can use untrusted HTTP URLs, but VRChat on Android/Quest requires HTTPS hosts",
        )

    headers: dict[str, str] = {}
    try:
        status, headers, _ = request("HEAD", url, read_bytes=0)
        if status != 200:
            add(findings, "FAIL", url, "head", f"HEAD returned HTTP {status}")
        else:
            add(findings, "OK", url, "head", "HEAD returned HTTP 200")
    except urllib.error.HTTPError as error:
        add(findings, "FAIL", url, "head", f"HEAD returned HTTP {error.code}")
    except Exception as error:  # noqa: BLE001 - smoke script should report all probe failures.
        add(findings, "FAIL", url, "head", f"HEAD failed: {error}")

    content_type = headers.get("content-type", "")
    if path.endswith(".mp4") and not content_type.startswith("video/mp4"):
        add(findings, "FAIL", url, "content-type", f"expected video/mp4, got {content_type or '-'}")
    elif path.endswith(".mp3") and not content_type.startswith("audio/mpeg"):
        add(findings, "FAIL", url, "content-type", f"expected audio/mpeg, got {content_type or '-'}")
    elif content_type:
        add(findings, "OK", url, "content-type", content_type)

    if headers.get("accept-ranges", "").lower() != "bytes":
        add(findings, "FAIL", url, "accept-ranges", "missing Accept-Ranges: bytes")
    else:
        add(findings, "OK", url, "accept-ranges", "bytes")

    content_length = headers.get("content-length")
    if not content_length or not content_length.isdigit() or int(content_length) <= 0:
        add(findings, "FAIL", url, "content-length", f"invalid Content-Length: {content_length or '-'}")
    else:
        add(findings, "OK", url, "content-length", content_length)

    try:
        status, range_headers, sample = request(
            "GET",
            url,
            headers={"Range": f"bytes=0-{RANGE_SAMPLE_SIZE - 1}"},
            read_bytes=RANGE_SAMPLE_SIZE,
        )
        if status != 206:
            add(findings, "FAIL", url, "range", f"Range GET returned HTTP {status}, expected 206")
        elif "content-range" not in range_headers:
            add(findings, "FAIL", url, "range", "Range GET returned 206 without Content-Range")
        elif is_probably_html(sample):
            add(findings, "FAIL", url, "range", "Range GET returned HTML instead of media bytes")
        else:
            add(findings, "OK", url, "range", range_headers.get("content-range", "-"))
    except urllib.error.HTTPError as error:
        add(findings, "FAIL", url, "range", f"Range GET returned HTTP {error.code}")
    except Exception as error:  # noqa: BLE001
        add(findings, "FAIL", url, "range", f"Range GET failed: {error}")

    if path.endswith(".mp4"):
        check_faststart(url, findings)

    return headers


def check_faststart(url: str, findings: list[Finding]) -> None:
    try:
        status, _, sample = request(
            "GET",
            url,
            headers={"Range": f"bytes=0-{FASTSTART_SAMPLE_SIZE - 1}"},
            read_bytes=FASTSTART_SAMPLE_SIZE,
        )
        if status not in {200, 206}:
            add(findings, "FAIL", url, "faststart", f"sample GET returned HTTP {status}")
            return
    except Exception as error:  # noqa: BLE001
        add(findings, "FAIL", url, "faststart", f"could not read MP4 header sample: {error}")
        return

    moov = sample.find(b"moov")
    mdat = sample.find(b"mdat")
    if moov < 0:
        add(findings, "FAIL", url, "faststart", "moov atom not found in first 1 MiB")
    elif 0 <= mdat < moov:
        add(findings, "FAIL", url, "faststart", "mdat appears before moov; MP4 is not web optimized")
    else:
        add(findings, "OK", url, "faststart", "moov atom is before media data")


def run_ffprobe(ffprobe: str, url: str) -> dict[str, Any]:
    output = subprocess.check_output(
        [
            ffprobe,
            "-v",
            "error",
            "-show_entries",
            "format=format_name,duration,size",
            "-show_entries",
            "stream=index,codec_type,codec_name,profile,width,height,pix_fmt,sample_rate,channels",
            "-of",
            "json",
            url,
        ],
        stderr=subprocess.STDOUT,
        text=True,
        timeout=60,
    )
    return json.loads(output)


def check_codecs(url: str, findings: list[Finding], ffprobe: str | None) -> None:
    if not ffprobe:
        add(findings, "WARN", url, "ffprobe", "ffprobe not found; codec checks skipped")
        return

    try:
        probe = run_ffprobe(ffprobe, url)
    except subprocess.CalledProcessError as error:
        add(findings, "FAIL", url, "ffprobe", error.output.strip()[:400] or "ffprobe failed")
        return
    except Exception as error:  # noqa: BLE001
        add(findings, "FAIL", url, "ffprobe", f"ffprobe failed: {error}")
        return

    streams = probe.get("streams") or []
    video_streams = [stream for stream in streams if stream.get("codec_type") == "video"]
    audio_streams = [stream for stream in streams if stream.get("codec_type") == "audio"]
    path = urllib.parse.urlparse(url).path.lower()

    if path.endswith(".mp4"):
        if not video_streams:
            add(findings, "FAIL", url, "video-codec", "MP4 has no video stream")
        else:
            video = video_streams[0]
            codec = str(video.get("codec_name") or "")
            pix_fmt = str(video.get("pix_fmt") or "")
            resolution = f"{video.get('width', '?')}x{video.get('height', '?')}"
            if codec != "h264":
                add(findings, "FAIL", url, "video-codec", f"expected h264 for best VRChat compatibility, got {codec or '-'}")
            else:
                add(findings, "OK", url, "video-codec", f"h264 {resolution}")
            if pix_fmt and pix_fmt != "yuv420p":
                add(findings, "WARN", url, "pixel-format", f"expected yuv420p for broad compatibility, got {pix_fmt}")
            elif pix_fmt:
                add(findings, "OK", url, "pixel-format", pix_fmt)

        if not audio_streams:
            add(findings, "FAIL", url, "audio-codec", "video URL has no audio stream")
        else:
            audio = audio_streams[0]
            codec = str(audio.get("codec_name") or "")
            channels = audio.get("channels", "?")
            rate = audio.get("sample_rate", "?")
            if codec not in {"aac", "mp3"}:
                add(findings, "FAIL", url, "audio-codec", f"expected aac/mp3, got {codec or '-'}")
            else:
                add(findings, "OK", url, "audio-codec", f"{codec} {channels}ch {rate}Hz")

    elif path.endswith(".mp3"):
        if not audio_streams:
            add(findings, "FAIL", url, "audio-codec", "MP3 has no audio stream")
        else:
            codec = str(audio_streams[0].get("codec_name") or "")
            if codec != "mp3":
                add(findings, "FAIL", url, "audio-codec", f"expected mp3, got {codec or '-'}")
            else:
                add(findings, "OK", url, "audio-codec", "mp3")


def print_findings(findings: list[Finding], *, as_json: bool) -> None:
    if as_json:
        print(json.dumps([finding.__dict__ for finding in findings], indent=2, ensure_ascii=False))
        return
    for finding in findings:
        print(f"[{finding.level}] {finding.check}: {finding.detail}")
        print(f"      {finding.url}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", action="append", default=[], help="Public media URL to check. Can be repeated.")
    parser.add_argument("--job-id", action="append", default=[], help="Fetch artifact URLs from this job id. Can be repeated.")
    parser.add_argument("--base-url", default=os.environ.get("RK_PUBLIC_BASE_URL", "http://127.0.0.1:8780"))
    parser.add_argument("--api-key", default=os.environ.get("RK_API_KEY"))
    parser.add_argument("--api-key-file", default=os.environ.get("RK_API_KEY_FILE"))
    parser.add_argument("--ffprobe", default=shutil.which("ffprobe"))
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    api_key = args.api_key or read_secret(args.api_key_file)
    urls = list(args.url)
    for job_id in args.job_id:
        if not api_key:
            parser.error("--job-id requires --api-key, --api-key-file, or RK_API_KEY")
        urls.extend(fetch_artifact_urls(args.base_url, api_key, job_id))

    if not urls:
        parser.error("provide at least one --url or --job-id")

    findings: list[Finding] = []
    for url in urls:
        check_http(url, findings)
        check_codecs(url, findings, args.ffprobe)

    print_findings(findings, as_json=args.json)
    return 1 if any(finding.level == "FAIL" for finding in findings) else 0


if __name__ == "__main__":
    raise SystemExit(main())
