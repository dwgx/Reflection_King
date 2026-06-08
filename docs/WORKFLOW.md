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
- Persist jobs, candidates, and artifacts in SQLite.

Intended near-term targets:

- Bilibili public video candidate discovery and selected audio/video artifacts.
- SoundCloud public audio candidate discovery and MP3 artifacts.
- YouTube public video candidate discovery, with explicit failure when access is
  gated or no candidate is found.

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

1. Add a public GitHub remote and push a sanitized history.
2. Extend CI to include the browser sidecar.
3. Run a real Bilibili public video browser-probe test and record the evidence.
4. Add artifact selection UI or a small CLI helper for candidate selection.
5. Add noVNC/Xvfb deployment for headed Linux profile login.
