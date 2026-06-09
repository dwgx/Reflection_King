# Project Workflow

This document is the operating contract for Reflection King development.

## Project Purpose

Reflection King is a policy-bound media acquisition backend.

Current implemented capabilities:

- Accept direct media URLs, download them with SSRF and size checks, transcode
  audio to MP3, and serve byte-range media URLs.
- Use a Playwright sidecar to load authorized public pages, observe browser
  network activity, save media candidates, let the caller select candidates,
  and generate server-owned artifacts.
- Discover media on unknown pages through generic DOM, performance-resource,
  anchor, metadata, network, and inline-script URL scanning.
- Parse Bilibili public-page `__playinfo__` DASH candidates and generate MP3
  audio artifacts from verified public video samples.
- Serve a Vite React dashboard for job creation, recent job inspection,
  candidate selection, and artifact playback.
- Persist jobs, candidates, and artifacts in SQLite.

Intended near-term targets:

- SoundCloud public audio candidate discovery and MP3 artifacts. Current
  evidence shows browser probing alone does not find audio candidates.
- YouTube public video candidate discovery. Current evidence shows browser
  probing alone only found page UI audio before filtering, not real media.

Non-goals:

- DRM removal.
- Captcha solving.
- Paywall, login-wall, or access-control bypass.
- Guessing private tokens or evading rate limits.
- Claims about platform support that have not been verified with evidence.

## Evidence Rules

- Do not state that a feature works unless it has been built and verified.
- If a claim comes from docs or code inspection, name the source.
- If a claim comes from a live test, record the command or scenario.
- If a behavior is inferred, label it as inference and identify the missing
  proof.
- If a review finds a possible issue, ground it in a file path, function, API
  contract, test result, or documented requirement.
- Do not invent future failures during review. Separate confirmed defects from
  risks and open questions.

## Coding Rules

- Keep the direct URL path working while adding browser acquisition.
- Discovery must only produce candidates; downloading and transcoding stay in
  the Rust backend.
- Every candidate URL must pass the same URL policy as user-provided URLs.
- Browser sidecar must not return Cookie/Auth headers through public job or
  candidate APIs.
- Persistent browser profiles, `node_modules`, build outputs, storage, and logs
  must never be committed.
- Prefer small, verifiable milestones over broad rewrites.
- Update docs and API examples in the same change when public behavior changes.

## Required Checks

Run before every commit:

```powershell
.\scripts\check.ps1
```

The check script must cover:

- Rust formatting, clippy, and tests.
- Browser sidecar TypeScript type checking.
- PowerShell script syntax for project scripts.
- Git ignore safety for browser profiles and dependency folders.

If a check cannot run, document the exact missing dependency or error in the
handoff or final response.

## Review Workflow

When asked for a review:

1. Read the changed code and relevant docs first.
2. Report confirmed findings first, ordered by severity.
3. Cite file paths and lines when available.
4. Include missing tests only when they are tied to a concrete risk.
5. End with a short residual-risk note if no issues are found.

Review output must not speculate about unsupported platforms or hidden bugs
without evidence. Use "risk" or "open question" labels when proof is missing.

## Git And GitHub Workflow

- Keep `master` clean and buildable.
- Commit coherent slices with short imperative messages.
- Do not commit secrets, browser profiles, `node_modules`, `target`, storage,
  generated media, or local logs.
- Before publishing a public repo, ensure no sensitive files exist in current
  HEAD. If sensitive local state entered history, squash or rewrite the local
  branch before the first public push.
- Once a GitHub remote exists, set it explicitly:

```powershell
git remote add origin https://github.com/<owner>/<repo>.git
git push -u origin master
```

- Public GitHub repos can be pulled anonymously on the server with HTTPS:

```bash
git clone https://github.com/<owner>/<repo>.git /opt/reflection-king
git pull --ff-only
```

- Private repos require a deploy key, token, or SSH key. Do not use password
  prompts or commit credentials.

## Deployment Workflow

Preferred server update path after a public GitHub repo exists:

```bash
cd /opt/reflection-king
git pull --ff-only
sudo APP_DIR=/opt/reflection-king bash scripts/deploy/linux-install-services.sh
```

If no remote exists yet:

1. Upload the working tree with `scp` or `rsync`.
2. Run `scripts/deploy/linux-bootstrap.sh` once.
3. Run `scripts/deploy/linux-install-services.sh`.

Use SSH keys for automation. If a root password was shared during setup, rotate
it after deployment and move to key-based login.

## Next Milestones

1. Add automated fixtures for generic unknown-page discovery: DOM media,
   metadata, link preload, performance resources, inline JSON, script URL
   extraction, and manifest URLs.
2. Add CDP-backed network capture for redirect chains, initiators, and bounded
   small JSON/manifest body inspection.
3. Add HLS and DASH manifest parsers with child URL SSRF validation, variant
   metadata, protection markers, and hard segment/duration limits.
4. Add artifact selection UI or a small CLI helper for candidate selection.
5. Add noVNC/Xvfb deployment for headed Linux profile login.
6. Build site-specific SoundCloud and YouTube extractors only after the generic
   discovery evidence shows which platform-specific layer is still missing.
7. Add automated integration tests around candidate selection and Range media
   responses.
