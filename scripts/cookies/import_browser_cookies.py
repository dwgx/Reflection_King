#!/usr/bin/env python3
"""Import local browser cookies into a Reflection King browser profile.

This intentionally runs as an explicit local Python command. It does not
register URL protocols, does not open helper shells, and does not print cookie
values. It uses yt-dlp's browser cookie extractor, converts the Netscape cookie
file to Playwright JSON, and submits only allowed site domains to the API.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
from http.cookiejar import Cookie
from dataclasses import dataclass
from pathlib import Path
from typing import Any


PLATFORM_DOMAINS = {
    "bilibili": (
        ".bilibili.com",
        ".bilivideo.com",
        ".hdslb.com",
    ),
    "youtube": (
        ".youtube.com",
        ".google.com",
        ".googlevideo.com",
        ".ytimg.com",
    ),
    "soundcloud": (
        ".soundcloud.com",
        ".sndcdn.com",
        ".media-streaming.soundcloud.cloud",
    ),
    "douyin": (
        ".douyin.com",
        ".iesdouyin.com",
        ".douyinvod.com",
        ".byteimg.com",
        ".bytedance.com",
    ),
    "kuaishou": (
        ".kuaishou.com",
        ".gifshow.com",
        ".ksapisrv.com",
    ),
    "acfun": (
        ".acfun.cn",
        ".aixifan.com",
    ),
    "iqiyi": (
        ".iqiyi.com",
        ".qiyi.com",
    ),
    "youku": (
        ".youku.com",
        ".ykimg.com",
    ),
}


@dataclass(frozen=True)
class NetscapeCookie:
    domain: str
    path: str
    secure: bool
    expires: int
    name: str
    value: str
    http_only: bool


def eprint(message: str) -> None:
    print(message, file=sys.stderr, flush=True)


def normalized_domain(value: str) -> str:
    value = value.strip().lower()
    if value.startswith("#httponly_"):
        value = value[len("#httponly_") :]
    return value


def domain_matches(domain: str, allowed: tuple[str, ...]) -> bool:
    host = normalized_domain(domain).lstrip(".")
    for suffix in allowed:
        suffix_host = suffix.lower().lstrip(".")
        if host == suffix_host or host.endswith("." + suffix_host):
            return True
    return False


def parse_netscape_cookie_line(line: str) -> NetscapeCookie | None:
    line = line.rstrip("\n")
    if not line or line.startswith("# ") or line == "# Netscape HTTP Cookie File":
        return None

    http_only = line.startswith("#HttpOnly_")
    if line.startswith("#") and not http_only:
        return None
    if http_only:
        line = line[len("#HttpOnly_") :]

    parts = line.split("\t")
    if len(parts) != 7:
        return None

    domain, _include_subdomains, path, secure, expires, name, value = parts
    try:
        expires_value = int(expires)
    except ValueError:
        expires_value = 0

    return NetscapeCookie(
        domain=domain,
        path=path or "/",
        secure=secure.upper() == "TRUE",
        expires=expires_value,
        name=name,
        value=value,
        http_only=http_only,
    )


def read_netscape_cookies(path: Path, allowed_domains: tuple[str, ...]) -> list[dict[str, Any]]:
    cookies: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    with path.open("r", encoding="utf-8", errors="replace") as file:
        for line in file:
            cookie = parse_netscape_cookie_line(line)
            if cookie is None or not domain_matches(cookie.domain, allowed_domains):
                continue
            key = (normalized_domain(cookie.domain), cookie.path, cookie.name)
            if key in seen:
                continue
            seen.add(key)
            playwright_cookie: dict[str, Any] = {
                "name": cookie.name,
                "value": cookie.value,
                "domain": normalized_domain(cookie.domain),
                "path": cookie.path,
                "httpOnly": cookie.http_only,
                "secure": cookie.secure,
                "sameSite": "Lax",
            }
            if cookie.expires > 0:
                playwright_cookie["expires"] = cookie.expires
            cookies.append(playwright_cookie)
    return cookies


def browser_cookie3_loader(browser: str):
    try:
        import browser_cookie3  # type: ignore[import-not-found]
    except ImportError as error:
        raise SystemExit(
            "browser-cookie3 engine requires `python -m pip install --user browser-cookie3`"
        ) from error

    loaders = {
        "edge": browser_cookie3.edge,
        "chrome": browser_cookie3.chrome,
        "firefox": browser_cookie3.firefox,
        "brave": browser_cookie3.brave,
        "chromium": browser_cookie3.chromium,
    }
    try:
        return loaders[browser]
    except KeyError as error:
        known = ", ".join(sorted(loaders))
        raise SystemExit(f"browser-cookie3 does not support browser {browser!r}; known: {known}") from error


def cookiejar_to_playwright(
    cookies: list[Cookie],
    allowed_domains: tuple[str, ...],
) -> list[dict[str, Any]]:
    output: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    for cookie in cookies:
        domain = normalized_domain(cookie.domain)
        if not domain_matches(domain, allowed_domains):
            continue
        key = (domain, cookie.path, cookie.name)
        if key in seen:
            continue
        seen.add(key)
        value: dict[str, Any] = {
            "name": cookie.name,
            "value": cookie.value,
            "domain": domain,
            "path": cookie.path or "/",
            "httpOnly": bool(cookie.has_nonstandard_attr("HttpOnly")),
            "secure": bool(cookie.secure),
            "sameSite": "Lax",
        }
        if cookie.expires:
            value["expires"] = int(cookie.expires)
        output.append(value)
    return output


def export_with_browser_cookie3(args: argparse.Namespace, allowed_domains: tuple[str, ...]) -> list[dict[str, Any]]:
    loader = browser_cookie3_loader(args.browser)
    collected: list[Cookie] = []
    failures: list[str] = []
    for domain in allowed_domains:
        try:
            jar = loader(domain_name=domain.lstrip("."))
            collected.extend(list(jar))
        except Exception as error:  # noqa: BLE001 - report per-domain browser access failures.
            failures.append(f"{domain}: {type(error).__name__}: {str(error)[:220]}")
    cookies = cookiejar_to_playwright(collected, allowed_domains)
    if not cookies and failures:
        raise SystemExit(
            "browser-cookie3 did not return matching cookies.\n"
            + "\n".join(failures)
            + "\nTry closing the browser and using --engine yt-dlp, or run this command as administrator."
        )
    return cookies


def yt_dlp_command(args: argparse.Namespace) -> list[str]:
    if args.yt_dlp:
        return [args.yt_dlp]
    executable = shutil.which("yt-dlp")
    if executable:
        return [executable]
    return [sys.executable, "-m", "yt_dlp"]


def export_browser_cookies(args: argparse.Namespace, output_path: Path) -> None:
    command = yt_dlp_command(args)
    browser = args.browser
    if args.browser_profile:
        browser = f"{browser}:{args.browser_profile}"

    # yt-dlp writes the extracted browser cookies to --cookies. The URL is only
    # used as a harmless target that lets yt-dlp initialize cookie extraction.
    full_command = [
        *command,
        "--cookies-from-browser",
        browser,
        "--cookies",
        str(output_path),
        "--skip-download",
        "--simulate",
        "--no-warnings",
        "https://example.com/",
    ]
    eprint(f"exporting cookies with browser={browser}")
    result = subprocess.run(
        full_command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            "yt-dlp cookie export failed. Install/update yt-dlp or try another browser.\n"
            + result.stderr[-1200:]
        )


def import_to_reflection(base_url: str, api_key: str, profile_id: str, cookies: list[dict[str, Any]]) -> Any:
    endpoint = (
        base_url.rstrip("/")
        + "/api/admin/browser-profiles/"
        + urllib.parse.quote(profile_id, safe="")
        + "/cookies/import"
    )
    payload = json.dumps({"cookies": cookies}).encode("utf-8")
    request = urllib.request.Request(
        endpoint,
        data=payload,
        method="POST",
        headers={
            "content-type": "application/json",
            "x-api-key": api_key,
        },
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            text = response.read().decode("utf-8")
            return json.loads(text) if text else {}
    except urllib.error.HTTPError as error:
        text = error.read().decode("utf-8", "replace")
        raise SystemExit(f"cookie import failed: HTTP {error.code}: {text[:1000]}") from error


def resolve_allowed_domains(args: argparse.Namespace) -> tuple[str, ...]:
    domains: list[str] = []
    for platform in args.platform:
        try:
            domains.extend(PLATFORM_DOMAINS[platform])
        except KeyError as error:
            known = ", ".join(sorted(PLATFORM_DOMAINS))
            raise SystemExit(f"unknown platform {platform!r}; known: {known}") from error
    for domain in args.domain:
        value = domain.strip()
        if value:
            domains.append(value if value.startswith(".") else "." + value)
    if not domains:
        raise SystemExit("select at least one --platform or --domain")
    return tuple(dict.fromkeys(domains))


def load_api_key(args: argparse.Namespace) -> str:
    if args.api_key:
        return args.api_key.strip()
    if args.api_key_file:
        return Path(args.api_key_file).read_text(encoding="utf-8").strip()
    env_key = os.environ.get("RK_API_KEY") or os.environ.get("API_KEY")
    if env_key:
        return env_key.strip()
    raise SystemExit("provide --api-key, --api-key-file, RK_API_KEY, or API_KEY")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-url", default=os.environ.get("RK_BASE_URL", "http://154.40.36.22:8780"))
    parser.add_argument("--api-key")
    parser.add_argument("--api-key-file")
    parser.add_argument("--profile-id", default="admin_default")
    parser.add_argument(
        "--browser",
        default="edge",
        help="yt-dlp browser name, for example edge, chrome, firefox, brave",
    )
    parser.add_argument("--browser-profile", help="optional yt-dlp browser profile name/path")
    parser.add_argument("--yt-dlp", help="optional path to yt-dlp executable")
    parser.add_argument(
        "--engine",
        choices=["yt-dlp", "browser-cookie3"],
        default="yt-dlp",
        help="cookie extraction engine. yt-dlp works without admin when the browser DB is not locked; browser-cookie3 can use Windows shadow copy when run as admin.",
    )
    parser.add_argument(
        "--platform",
        action="append",
        default=[],
        choices=sorted(PLATFORM_DOMAINS),
        help="platform domain set to import; can be repeated",
    )
    parser.add_argument(
        "--domain",
        action="append",
        default=[],
        help="extra cookie domain suffix, for example .example.com; can be repeated",
    )
    parser.add_argument("--dry-run", action="store_true", help="export and count cookies without uploading")
    parser.add_argument("--keep-cookies-file", help="optional debug path for the Netscape cookies file")
    args = parser.parse_args()

    api_key = load_api_key(args)
    allowed_domains = resolve_allowed_domains(args)
    eprint("allowed domains: " + ", ".join(allowed_domains))

    if args.engine == "browser-cookie3":
        cookies = export_with_browser_cookie3(args, allowed_domains)
    else:
        with tempfile.TemporaryDirectory(prefix="rk-cookies-") as temp_dir:
            cookies_path = Path(args.keep_cookies_file) if args.keep_cookies_file else Path(temp_dir) / "cookies.txt"
            export_browser_cookies(args, cookies_path)
            cookies = read_netscape_cookies(cookies_path, allowed_domains)

    if not cookies:
        raise SystemExit(
            "no matching cookies found. Confirm that this Windows user is logged into the browser "
            "and try --browser chrome/firefox or add --domain."
        )

    domains = sorted({cookie["domain"] for cookie in cookies})
    print(
        json.dumps(
            {
                "profile_id": args.profile_id,
                "browser": args.browser,
                "engine": args.engine,
                "cookie_count": len(cookies),
                "domains": domains,
                "dry_run": args.dry_run,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    if args.dry_run:
        return 0

    response = import_to_reflection(args.base_url, api_key, args.profile_id, cookies)
    print(json.dumps({"import_response": response}, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
