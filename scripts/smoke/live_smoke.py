#!/usr/bin/env python3
"""Live smoke test for Reflection King media acquisition.

The script creates real jobs against a running API, watches state transitions,
selects top-ranked candidates when needed, and verifies generated media URLs
with a byte-range read.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any


TERMINAL_STATUSES = {"ready", "error"}


@dataclass(frozen=True)
class SmokeCase:
    name: str
    url: str
    discovery: str
    platform_hint: str
    outputs: list[str]
    tier: str = "core"
    notes: str = ""
    bitrate: str = "auto"
    auth_mode: str = "auto"
    profile_id: str = "admin_default"
    max_selected: int = 8
    expect_success: bool = True


DEFAULT_CASES = [
    SmokeCase(
        name="mdn-page-browser-video",
        url="https://interactive-examples.mdn.mozilla.net/pages/tabbed/source.html",
        discovery="browser",
        platform_hint="generic",
        outputs=["video"],
    ),
    SmokeCase(
        name="mdn-direct-audio",
        url="https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.mp4",
        discovery="direct",
        platform_hint="generic",
        outputs=["audio"],
        bitrate="128k",
    ),
    SmokeCase(
        name="blender-bbb-auto-video",
        url="https://download.blender.org/peach/bigbuckbunny_movies/BigBuckBunny_320x180.mp4",
        discovery="auto",
        platform_hint="generic",
        outputs=["video"],
    ),
    SmokeCase(
        name="archive-bbb-auto-audio",
        url="https://archive.org/download/BigBuckBunny_328/BigBuckBunny_512kb.mp4",
        discovery="auto",
        platform_hint="generic",
        outputs=["audio"],
        bitrate="128k",
    ),
    SmokeCase(
        name="mux-hls-low-video",
        url="https://test-streams.mux.dev/x36xhzz/url_2/193039199_mp4_h264_aac_ld_7.m3u8",
        discovery="auto",
        platform_hint="live",
        outputs=["video"],
        notes="Low-resolution public HLS video stream; keeps default smoke below large-artifact pressure size.",
    ),
    SmokeCase(
        name="mux-hls-auto-audio",
        url="https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8",
        discovery="auto",
        platform_hint="live",
        outputs=["audio"],
        bitrate="128k",
    ),
    SmokeCase(
        name="youtube-bbb-external-video",
        url="https://www.youtube.com/watch?v=aqz-KE-bpKQ",
        discovery="external",
        platform_hint="youtube",
        outputs=["video"],
        tier="platform",
        notes="yt-dlp supported public Blender Foundation video; may fail if YouTube changes tokens or bot checks.",
    ),
    SmokeCase(
        name="soundcloud-public-audio",
        url="https://m.soundcloud.com/nasa/apollo-8-merry-christmas",
        discovery="external",
        platform_hint="soundcloud",
        outputs=["audio"],
        tier="platform",
        notes="yt-dlp supported public NASA audio sample.",
        bitrate="128k",
    ),
    SmokeCase(
        name="bilibili-bbb-browser-video",
        url="https://www.bilibili.com/video/BV1Fb4111732/",
        discovery="browser",
        platform_hint="bilibili",
        outputs=["video"],
        tier="platform",
        notes="Browser/API probe public Bilibili video; quality may require profile login.",
    ),
    SmokeCase(
        name="acfun-public-external-video",
        url="https://m.acfun.cn/v/?ac=17529896",
        discovery="external",
        platform_hint="acfun",
        outputs=["video"],
        tier="platform",
        notes="yt-dlp AcFunVideo extractor; public short video, HLS MP4 variants.",
    ),
    SmokeCase(
        name="youku-public-trailer-video",
        url="https://v.youku.com/v_show/id_XNDgwODM0NjYwNA%3D%3D.html",
        discovery="external",
        platform_hint="youku",
        outputs=["video"],
        tier="platform",
        notes="yt-dlp/you-get public Youku trailer; signed HLS URLs may expire quickly.",
    ),
    SmokeCase(
        name="tiktok-public-external-video",
        url="https://vm.tiktok.com/ZMBNyCU7n/",
        discovery="external",
        platform_hint="tiktok",
        outputs=["video"],
        tier="platform",
        notes="yt-dlp TikTok public short-video sample; quality and availability can vary by region or bot checks.",
    ),
    SmokeCase(
        name="douyin-public-external-video",
        url="https://v.douyin.com/BgPNNVe/",
        discovery="external",
        platform_hint="douyin",
        outputs=["video"],
        tier="experimental",
        notes="yt-dlp Douyin public short-video sample; currently works without login on the VPS but some Douyin URLs require fresh cookies.",
    ),
    SmokeCase(
        name="douyin-fresh-cookies-required",
        url="https://www.douyin.com/video/7519330213189127439",
        discovery="external",
        platform_hint="douyin",
        outputs=["video"],
        tier="experimental",
        notes="Expected failure sample: yt-dlp reports fresh cookies are needed, not necessarily a logged-in account.",
        expect_success=False,
    ),
    SmokeCase(
        name="kuaishou-public-auto-probe",
        url="https://www.kuaishou.com/short-video/3x2wdee2f2ud7ac",
        discovery="auto",
        platform_hint="kuaishou",
        outputs=["video"],
        tier="experimental",
        notes="Expected failure sample on current VPS: yt-dlp does not support the URL and you-get fails; retained to track future adapter work.",
        expect_success=False,
    ),
    SmokeCase(
        name="apple-bipbop-large-hls-video",
        url="https://devstreaming-cdn.apple.com/videos/streaming/examples/bipbop_16x9/bipbop_16x9_variant.m3u8",
        discovery="auto",
        platform_hint="live",
        outputs=["video"],
        tier="experimental",
        notes="Apple public HLS sample is about 30 minutes and can generate artifacts above 300 MB; use for manual stress checks only.",
    ),
    SmokeCase(
        name="iqiyi-public-trailer-probe",
        url="https://www.iq.com/play/invincible-call-to-power-trailer-19ruu27qdk?lang=en_us",
        discovery="external",
        platform_hint="iqiyi",
        outputs=["video"],
        tier="experimental",
        notes="Currently blocked in yt-dlp without PhantomJS on VPS; retained for manual extractor research only.",
    ),
]


def eprint(message: str) -> None:
    print(message, flush=True)


class Client:
    def __init__(self, base_url: str, api_key: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key

    def request(
        self,
        method: str,
        path: str,
        body: Any | None = None,
        *,
        timeout: int = 60,
        raw: bool = False,
        extra_headers: dict[str, str] | None = None,
    ) -> Any:
        data = None
        headers = {"x-api-key": self.api_key}
        if body is not None:
            data = json.dumps(body).encode("utf-8")
            headers["content-type"] = "application/json"
        if extra_headers:
            headers.update(extra_headers)
        request = urllib.request.Request(
            self.base_url + path,
            data=data,
            method=method,
            headers=headers,
        )
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                if raw:
                    return {
                        "status": response.status,
                        "headers": {key.lower(): value for key, value in response.headers.items()},
                        "sample": response.read(512),
                    }
                text = response.read().decode("utf-8")
                return json.loads(text) if text else None
        except urllib.error.HTTPError as error:
            payload = error.read().decode("utf-8", "replace")
            raise RuntimeError(f"{method} {path} HTTP {error.code}: {payload[:500]}") from error


def parse_height(candidate: dict[str, Any]) -> int:
    quality = candidate.get("quality_label") or ""
    if isinstance(quality, str) and quality.endswith("p"):
        try:
            return int(quality[:-1])
        except ValueError:
            return 0
    metadata = candidate.get("metadata_json")
    if isinstance(metadata, dict):
        candidate_metadata = metadata.get("candidate")
        if isinstance(candidate_metadata, dict):
            height = candidate_metadata.get("height")
            if isinstance(height, (int, float)):
                return int(height)
    return 0


def candidate_rank(candidate: dict[str, Any], outputs: list[str]) -> int:
    kind = candidate.get("kind")
    if "video" in outputs:
        kind_score = {"video": 4000, "manifest": 3600, "audio": -1000, "image": -2000}.get(kind, -3000)
    elif "audio" in outputs:
        kind_score = {"audio": 4000, "manifest": 3300, "video": 2800}.get(kind, -3000)
    else:
        kind_score = {"image": 2500, "video": 2000, "manifest": 1800, "audio": 1500}.get(kind, 0)

    penalty = 0
    if candidate.get("ad_risk"):
        penalty += 10000
    if candidate.get("failure_reason"):
        penalty += 5000
    if candidate.get("protection") in {"drm", "region_blocked"}:
        penalty += 10000
    if candidate.get("requires_authorization"):
        penalty += 200
    compat = mp4_compatibility_rank(candidate) if "video" in outputs else 0
    return kind_score + int(candidate.get("score") or 0) + parse_height(candidate) + compat - penalty


def metadata_text(candidate: dict[str, Any], key: str) -> str:
    metadata = candidate.get("metadata_json")
    if isinstance(metadata, dict):
        value = metadata.get(key)
        if isinstance(value, str):
            return value
        nested = metadata.get("candidate")
        if isinstance(nested, dict):
            value = nested.get(key)
            if isinstance(value, str):
                return value
    return ""


def mp4_compatibility_rank(candidate: dict[str, Any]) -> int:
    value = " ".join(
        str(part or "").lower()
        for part in (
            candidate.get("content_type"),
            candidate.get("url"),
            metadata_text(candidate, "ext"),
            metadata_text(candidate, "vcodec"),
            metadata_text(candidate, "acodec"),
        )
    )
    rank = 0
    if "video/mp4" in value or ".mp4" in value or " mp4 " in value:
        rank += 300
    if "avc1" in value or "h264" in value:
        rank += 300
    if "mp4a" in value or "aac" in value:
        rank += 120
    if "video/webm" in value or ".webm" in value or "vp9" in value or "vp09" in value or "av01" in value:
        rank -= 700
    if "opus" in value or "vorbis" in value:
        rank -= 180
    return rank


def select_candidate_ids(candidates: list[dict[str, Any]], case: SmokeCase) -> list[str]:
    ranked = sorted(candidates, key=lambda candidate: candidate_rank(candidate, case.outputs), reverse=True)
    if "video" in case.outputs:
        primary = next(
            (
                candidate
                for candidate in ranked
                if candidate.get("kind") in {"video", "manifest"}
                and candidate.get("protection") != "drm"
                and not candidate.get("ad_risk")
            ),
            None,
        )
        if not primary:
            return []
        selected = [primary["id"]]
        if primary.get("kind") == "video" and candidate_needs_audio_companion(primary):
            audio = best_audio_companion(primary, ranked)
            if audio:
                selected.append(audio["id"])
        return selected

    selected: list[str] = []
    for candidate in ranked:
        if len(selected) >= case.max_selected:
            break
        if candidate.get("protection") == "drm" or candidate.get("ad_risk"):
            continue
        selected.append(candidate["id"])
    return selected


def candidate_needs_audio_companion(candidate: dict[str, Any]) -> bool:
    if candidate.get("kind") != "video":
        return False
    acodec = metadata_text(candidate, "acodec").strip().lower()
    vcodec = metadata_text(candidate, "vcodec").strip().lower()
    if codec_present(vcodec) and not codec_present(acodec):
        return True
    value = " ".join(
        str(candidate.get(key) or "").lower()
        for key in ("url", "resource_type", "quality_label")
    )
    return "bilibili" in value or ".m4s" in value or "dash" in value or "video-only" in value


def codec_present(value: str) -> bool:
    return bool(value and value not in {"none", "null", "unknown"})


def candidate_family(candidate: dict[str, Any]) -> str:
    resource_type = candidate.get("resource_type")
    if resource_type in {"bilibili_playinfo", "bilibili_api"}:
        return f"{candidate.get('extractor')}:bilibili"
    return f"{candidate.get('extractor')}:{candidate.get('initiator_url') or candidate.get('platform') or 'unknown'}"


def best_audio_companion(
    video_candidate: dict[str, Any],
    ranked_candidates: list[dict[str, Any]],
) -> dict[str, Any] | None:
    video_family = candidate_family(video_candidate)
    for candidate in ranked_candidates:
        if candidate.get("kind") != "audio":
            continue
        if candidate.get("protection") == "drm" or candidate.get("ad_risk"):
            continue
        if candidate_family(candidate) == video_family or is_bilibili_pair(video_candidate, candidate):
            return candidate
    return None


def is_bilibili_pair(left: dict[str, Any], right: dict[str, Any]) -> bool:
    left_value = f"{left.get('url') or ''} {left.get('resource_type') or ''}".lower()
    right_value = f"{right.get('url') or ''} {right.get('resource_type') or ''}".lower()
    return (
        "bilibili" in left_value
        and "bilibili" in right_value
        and left.get("extractor") == right.get("extractor")
    )


def external_to_api_path(media_url: str, base_url: str) -> str:
    for prefix in (base_url.rstrip("/"), "http://127.0.0.1:8787", "http://154.40.36.22:8780"):
        if media_url.startswith(prefix):
            return media_url[len(prefix) :]
    parsed = urllib.parse.urlparse(media_url)
    return parsed.path + (f"?{parsed.query}" if parsed.query else "")


def run_case(client: Client, case: SmokeCase, timeout_seconds: int) -> dict[str, Any]:
    eprint(f"\n=== {case.name} ===")
    payload = {
        "url": case.url,
        "discovery": case.discovery,
        "platform_hint": case.platform_hint,
        "outputs": case.outputs,
        "bitrate": case.bitrate,
        "profile_id": case.profile_id,
        "auth_mode": case.auth_mode,
    }
    if case.notes:
        eprint(f"notes {case.notes}")
    job = client.request("POST", "/api/jobs", payload)
    job_id = job["id"]
    eprint(f"job {job_id}")

    started_at = time.monotonic()
    last_status = ""
    candidates: list[dict[str, Any]] = []
    selected_once = False

    while time.monotonic() - started_at < timeout_seconds:
        time.sleep(2)
        job = client.request("GET", f"/api/jobs/{job_id}")
        status = job["status"]
        if status != last_status:
            error = job.get("error") or ""
            eprint(f"status {status}{' | ' + error[:180] if error else ''}")
            last_status = status

        if status == "candidates_ready" and not selected_once:
            candidates = client.request("GET", f"/api/jobs/{job_id}/candidates")
            eprint(f"candidates {len(candidates)}")
            for candidate in sorted(candidates, key=lambda item: candidate_rank(item, case.outputs), reverse=True)[:6]:
                eprint(
                    "  cand "
                    f"{candidate.get('kind')} {candidate.get('quality_label') or '-'} "
                    f"score={candidate.get('score')} "
                    f"route={candidate.get('route') or candidate.get('extractor')} "
                    f"{str(candidate.get('url'))[:120]}"
                )
            selected = select_candidate_ids(candidates, case)
            if not selected:
                raise RuntimeError(f"{case.name}: no selectable candidates")
            eprint(f"select {len(selected)} candidates")
            client.request("POST", f"/api/jobs/{job_id}/select-candidates", {"candidate_ids": selected})
            selected_once = True

        if status in TERMINAL_STATUSES:
            break
    else:
        raise TimeoutError(f"{case.name}: timed out waiting for job {job_id}")

    final_job = client.request("GET", f"/api/jobs/{job_id}")
    artifacts = client.request("GET", f"/api/jobs/{job_id}/artifacts")
    media_checks = []
    for artifact in artifacts:
        media_url = artifact["media_url"]
        path = external_to_api_path(media_url, client.base_url)
        try:
            result = client.request("GET", path, raw=True, extra_headers={"range": "bytes=0-511"})
            check = {
                "kind": artifact["kind"],
                "status": result["status"],
                "content_type": result["headers"].get("content-type"),
                "content_range": result["headers"].get("content-range"),
                "bytes": len(result["sample"]),
                "url": media_url,
            }
        except Exception as error:  # noqa: BLE001 - smoke script records failures.
            check = {"kind": artifact.get("kind"), "error": str(error), "url": media_url}
        media_checks.append(check)
        eprint("artifact " + json.dumps(check, ensure_ascii=False))

    trace = client.request("GET", f"/api/jobs/{job_id}/trace")
    return {
        "case": case.name,
        "tier": case.tier,
        "notes": case.notes,
        "job_id": job_id,
        "status": final_job["status"],
        "error": final_job.get("error"),
        "candidate_count": len(candidates),
        "artifact_count": len(artifacts),
        "media_checks": media_checks,
        "trace_events": len(trace),
        "expect_success": case.expect_success,
    }


def load_api_key(args: argparse.Namespace) -> str:
    if args.api_key:
        return args.api_key.strip()
    if args.api_key_file:
        with open(args.api_key_file, "r", encoding="utf-8") as file:
            return file.read().strip()
    env_key = os.environ.get("RK_API_KEY") or os.environ.get("API_KEY")
    if env_key:
        return env_key.strip()
    raise SystemExit("provide --api-key, --api-key-file, RK_API_KEY, or API_KEY")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default=os.environ.get("RK_BASE_URL", "http://127.0.0.1:8787"))
    parser.add_argument("--api-key")
    parser.add_argument("--api-key-file")
    parser.add_argument("--timeout-seconds", type=int, default=240)
    parser.add_argument("--case", action="append", help="Run only matching case name. Can be repeated.")
    parser.add_argument(
        "--tier",
        action="append",
        choices=["core", "platform", "experimental"],
        help="Run a smoke tier. Defaults to core. Can be repeated.",
    )
    parser.add_argument("--all-tiers", action="store_true", help="Run core, platform, and experimental cases.")
    parser.add_argument("--list", action="store_true", help="List case names and exit.")
    args = parser.parse_args()

    if args.list:
        for case in DEFAULT_CASES:
            print(f"{case.name}\t{case.tier}\t{case.discovery}/{case.platform_hint}")
        return 0

    api_key = load_api_key(args)
    selected_names = set(args.case or [])
    selected_tiers = {"core", "platform", "experimental"} if args.all_tiers else set(args.tier or ["core"])
    cases = [
        case
        for case in DEFAULT_CASES
        if (not selected_names or case.name in selected_names)
        and (selected_names or case.tier in selected_tiers)
    ]
    if selected_names and len(cases) != len(selected_names):
        known = {case.name for case in DEFAULT_CASES}
        missing = sorted(selected_names - known)
        raise SystemExit(f"unknown smoke case(s): {', '.join(missing)}")

    client = Client(args.base_url, api_key)
    results = []
    failed = False
    for case in cases:
        try:
            result = run_case(client, case, args.timeout_seconds)
            success = result["status"] == "ready" and result["artifact_count"]
            if case.expect_success and not success:
                failed = True
            results.append(result)
        except Exception as error:  # noqa: BLE001 - smoke script records failures.
            if case.expect_success:
                failed = True
            eprint(f"case failed: {case.name}: {error}")
            results.append(
                {
                    "case": case.name,
                    "status": "smoke_error",
                    "error": str(error),
                    "expect_success": case.expect_success,
                }
            )

    print("\nSUMMARY")
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
