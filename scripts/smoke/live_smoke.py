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
    bitrate: str = "auto"
    auth_mode: str = "auto"
    profile_id: str = "admin_default"
    max_selected: int = 8


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
        name="apple-hls-auto-video",
        url="https://devstreaming-cdn.apple.com/videos/streaming/examples/bipbop_16x9/bipbop_16x9_variant.m3u8",
        discovery="auto",
        platform_hint="live",
        outputs=["video"],
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
    ),
    SmokeCase(
        name="soundcloud-public-audio",
        url="https://m.soundcloud.com/nasa/apollo-8-merry-christmas",
        discovery="external",
        platform_hint="soundcloud",
        outputs=["audio"],
        bitrate="128k",
    ),
    SmokeCase(
        name="bilibili-bbb-browser-video",
        url="https://www.bilibili.com/video/BV1Fb4111732/",
        discovery="browser",
        platform_hint="bilibili",
        outputs=["video"],
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
    return kind_score + int(candidate.get("score") or 0) + parse_height(candidate) - penalty


def select_candidate_ids(candidates: list[dict[str, Any]], case: SmokeCase) -> list[str]:
    ranked = sorted(candidates, key=lambda candidate: candidate_rank(candidate, case.outputs), reverse=True)
    selected: list[str] = []
    for candidate in ranked:
        if len(selected) >= case.max_selected:
            break
        if candidate.get("protection") == "drm" or candidate.get("ad_risk"):
            continue
        selected.append(candidate["id"])
    return selected


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
        "job_id": job_id,
        "status": final_job["status"],
        "error": final_job.get("error"),
        "candidate_count": len(candidates),
        "artifact_count": len(artifacts),
        "media_checks": media_checks,
        "trace_events": len(trace),
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
    parser.add_argument("--list", action="store_true", help="List case names and exit.")
    args = parser.parse_args()

    if args.list:
        for case in DEFAULT_CASES:
            print(case.name)
        return 0

    api_key = load_api_key(args)
    selected_names = set(args.case or [])
    cases = [case for case in DEFAULT_CASES if not selected_names or case.name in selected_names]
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
            if result["status"] != "ready" or not result["artifact_count"]:
                failed = True
            results.append(result)
        except Exception as error:  # noqa: BLE001 - smoke script records failures.
            failed = True
            eprint(f"case failed: {case.name}: {error}")
            results.append({"case": case.name, "status": "smoke_error", "error": str(error)})

    print("\nSUMMARY")
    print(json.dumps(results, ensure_ascii=False, indent=2))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
