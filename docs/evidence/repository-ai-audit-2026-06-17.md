# Repository And Agent-Work Audit 2026-06-17

## Scope

This audit records the repository state after commit `04f4882` on `master`.
It focuses on what Reflection King is for, how strong the current engineering
baseline is, and where repeated Agent work has left misleading or risky
maintenance artifacts.

Verified repository facts:

- `git status --short --untracked-files=all`: clean.
- `git log --oneline -10`: latest commit is `04f4882 Verify archive cache Rust
  checks`.
- `git ls-files`: 122 tracked files.
- GitHub Actions latest `master` run: `27593628977`, `success`, head SHA
  `04f4882f610b52abf47ef30846fb05bba5054439`.

This is not a live VPS audit. The VPS state was not revalidated in this pass.

## Project Purpose

Reflection King is a Rust media acquisition, transcoding, and raw URL output
backend. Its target workflow is to turn public or operator-authorized media
pages and direct media URLs into server-owned artifacts that external players,
such as VRChat video players, can fetch without cookies or API keys.

Current implementation shape:

- Rust API and core crates handle tasks, URL policy, candidate storage,
  artifact serving, downloads, remux/transcode, API keys, cache inventory, and
  page archive APIs.
- Playwright sidecar handles browser probing, Profile/Cookie reuse, CDP network
  capture, screenshots, MHTML/HAR generation, and page resource metadata.
- React dashboard handles job creation, candidate selection, Profile login,
  admin key rotation, archive browsing, runtime settings, and cache cleanup.
- Documentation is intentionally evidence-heavy: workflow, deployment, security,
  API docs, platform smoke evidence, page archive evidence, and next-Agent
  handoff state are all tracked.

Explicit non-goals remain important: no DRM removal, captcha solving, paywall
or login-wall bypass, age-gate bypass, region-limit bypass, private token
guessing, or unsupported platform success claims.

## Self Score

| Area | Score | Rationale |
| --- | ---: | --- |
| Product fit | 8/10 | The README and workflow define a concrete problem: authorized media to reusable raw URLs. The API, dashboard, browser sidecar, and VRChat smoke scripts all align with that goal. |
| Architecture | 7/10 | The service split is clear, but large files such as `crates/reflection-api/src/state.rs`, `apps/reflection-dashboard/src/main.tsx`, and `services/reflection-browser/src/probe.ts` carry too many responsibilities after repeated feature additions. |
| Security | 7/10 | Strong positive signs: URL policy, API keys, job access checks, sensitive header filtering, private Profile storage, and documented `/media` boundary. Remaining concerns are operational: default public HTTP, raw URL sharing risk, and root installer deletion boundary. |
| Test and evidence | 7/10 | Current CI covers Rust fmt/clippy/test, sidecar check, dashboard build, Docker build, Compose config, and Compose health. Evidence docs are unusually strong. Gaps remain in local check parity, VPS revalidation, and page archive/browser UI regression fixtures. |
| Operations | 6/10 | Docker and VPS paths exist, and CI covers Docker health. The install path still has a P1 deletion hazard, and key retrieval docs contradict script behavior. |
| Documentation | 6/10 | The docs are broad and useful, but the handoff file mixes durable facts with stale conversational state. Some docs now contradict current scripts and CI state. |
| AI maintainability | 5/10 | The repo is maintainable only if future Agents treat handoff and evidence as review inputs, not truth. Symptoms include stale baselines, repeated caveats, large append-only feature files, and verification claims that drift from scripts. |

Overall score: **6.6/10**. The core engineering direction is solid, but
operational safety and documentation consistency must be tightened before this
can be treated as a production-grade service.

## Findings

### P1 - `install.sh` can delete the wrong root-owned directory

`install.sh` exposes `--app-dir PATH` as a root installer option (`install.sh`
lines 18-23). It only validates that `APP_DIR` is absolute and has no whitespace
(`install.sh` lines 79-82). If the target is not already a Git repository, it
executes `rm -rf "${APP_DIR}"` before cloning (`install.sh` lines 101-106).

Impact: a typo such as `--app-dir /opt`, a non-empty restore directory, or any
absolute path that is not this repo can be recursively deleted by a root
installer.

Follow-up status: fixed in this session. The installer now rejects protected
parent/system directories, symlink targets, non-directory targets, and non-empty
non-Git directories instead of deleting them.

Preferred fix:

- Reject `/`, `/opt`, `/home`, `/root`, `/usr`, `/var`, and other broad parent
  paths explicitly.
- If a non-empty non-Git directory exists, fail with a clear error instead of
  deleting it.
- If deletion is ever allowed, require a repo-specific marker file or an
  explicit force flag whose help text says it deletes that directory.

### P2 - Deployment docs say bootstrap keys are printed, but scripts hide them

README says Docker first startup prints the admin key (`README.md` lines 66-67).
`docs/DEPLOYMENT.md` says VPS install prints `Admin key: <initial key>` (lines
41-48) and later says quick Docker startup prints the key (lines 125-131).
`docs/NEXT_AGENT_HANDOFF.md` repeats that VPS and Docker startup display the
initial admin key (lines 243-247).

The scripts do the opposite by default:

- `scripts/deploy/docker-entrypoint.sh` prints `Admin key: [hidden; ...]` unless
  `RK_PRINT_BOOTSTRAP_KEY=1` is set.
- `scripts/deploy/linux-install-services.sh` prints the same hidden-key message
  unless `RK_PRINT_BOOTSTRAP_KEY=1` is set.

Impact: a new operator following the docs will wait for a key that never appears
in logs. This is especially confusing on a fresh VPS or Docker deployment.

Follow-up status: fixed in this session. README, deployment docs, workflow, and
handoff now describe bootstrap keys as hidden by default and point operators to
the server/container key file.

Preferred fix:

- Make README, deployment docs, operations docs, and handoff all say: default
  logs hide the admin key.
- Document the retrieval paths: `/data/admin-key.txt` for Docker and
  `/root/reflection-king-admin-key.txt` for VPS.
- Keep `RK_PRINT_BOOTSTRAP_KEY=1` documented only for one-time controlled
  environments.

### P2 - `scripts/check.ps1` is not the full handoff revalidation command

The handoff lists the latest local validation as Rust fmt/clippy/test, dashboard
build, sidecar check/build, and `git diff --check` (`docs/NEXT_AGENT_HANDOFF.md`
lines 98-107). It then says a future Agent can revalidate this round by running
only `.\scripts\check.ps1` (lines 127-130).

The script currently runs:

- Rust fmt, clippy, and test (`scripts/check.ps1` lines 48-63).
- Sidecar `npm run check` (`scripts/check.ps1` lines 66-71).
- PowerShell parser checks for three scripts (`scripts/check.ps1` lines 77-96).

It does not run dashboard build, sidecar build, `git diff --check`, or shell
syntax checks. CI does cover these broader areas: shell syntax and Rust
(`.github/workflows/ci.yml` lines 15-18), sidecar npm check (lines 32-33),
dashboard build (lines 47-48), and Docker/Compose health (lines 55-69).

Impact: future local validation can miss dashboard or sidecar build regressions
while the handoff implies one command is enough.

Follow-up status: fixed in this session. `scripts/check.ps1` now covers sidecar
check/build, dashboard build, PowerShell syntax, optional Bash syntax, and
`git diff --check`.

Preferred fix:

- Either expand `scripts/check.ps1` into a local CI-parity command, or rename it
  to a partial check and add a full command list to the handoff.
- Include dashboard build, sidecar build, `git diff --check`, and shell syntax
  where local tooling exists.

### P3 - Local check uses `npm install` while CI uses `npm ci`

`scripts/check.ps1` runs `npm install` when sidecar `node_modules` is missing
(`scripts/check.ps1` lines 66-71). CI uses `npm ci` for sidecar and dashboard
(`.github/workflows/ci.yml` lines 32 and 47).

Impact: local validation can mutate dependency state or follow a different
dependency resolution path than CI.

Follow-up status: fixed in this session. `scripts/check.ps1` now uses `npm ci`
when it needs to install missing Node dependencies.

Preferred fix:

- Use `npm ci` in validation scripts.
- Keep `npm install` in bootstrap-only scripts if needed.

### P3 - Handoff CI/VPS baseline is stale

`docs/NEXT_AGENT_HANDOFF.md` still records `b3e4533` and GitHub Actions run
`27457817046` as the most recently confirmed functional baseline (lines
265-270). Current `HEAD` and `origin/master` are
`04f4882f610b52abf47ef30846fb05bba5054439`, and latest `master` CI is run
`27593628977`, success.

The same handoff says the VPS is confirmed active and healthy with baseline
`b3e4533` (`docs/NEXT_AGENT_HANDOFF.md` lines 291-301). This audit did not
revalidate VPS access or health, so that statement should not be repeated as a
current fact.

Impact: the next Agent can waste time reconciling stale baselines or assume VPS
state is verified when it is not.

Follow-up status: fixed in this session. The handoff now records current local
CI baseline `04f4882` / run `27593628977` and marks VPS state as not revalidated
in this pass.

Preferred fix:

- Update CI baseline to `04f4882` / run `27593628977`.
- Change VPS wording to "last recorded" unless the current session can actually
  SSH/curl the deployed host.

## Positive Controls

The audit also confirmed several important controls that should be preserved:

- Archive tree/file endpoints use `authorize` and `ensure_job_access` before
  serving files (`crates/reflection-api/src/main.rs` lines 520-536).
- API auth reads `x-api-key` or `Authorization: Bearer` and maps user keys to
  role and permissions (`crates/reflection-api/src/main.rs` lines 1307-1337).
- Non-admin job access checks job ownership before serving protected job data
  (`crates/reflection-api/src/main.rs` lines 1356-1370).
- Dashboard archive opening uses authenticated `fetch(file.media_url,
  { headers })`, then a Blob URL; it does not put API keys in query strings
  (`apps/reflection-dashboard/src/main.tsx` lines 579-600 and 3005-3007).
- Public API docs state archive file access must use job authorization and
  `x-api-key` when API keys are enabled (`docs/api/public-api.md` lines
  115-127).
- Cache cleanup preview and execution share `cache_cleanup_inner`, and the API
  requires admin auth for cleanup preview/execute.
- Sidecar archive headers are allowlisted, HAR cookies are written as empty
  arrays, and CDP header capture uses a whitelist.
- Tracked-file scans did not find real private keys, GitHub tokens, Cookie
  dumps, SQLite databases, `storage/`, `target/`, node modules, HAR/MHTML/WARC,
  archive ZIPs, or screenshots committed to Git.

## Agent-Work Risk Signals

These are not defects by themselves; they are signs that repeated Agent work can
mislead future maintenance if not controlled.

1. **Handoff is mixing durable facts with session notes.**  
   `docs/NEXT_AGENT_HANDOFF.md` is useful, but it includes stale CI/VPS facts,
   current-machine Rust path notes, old baseline references, and a large
   copy-paste prompt. Treat it as a handoff log, not as the source of truth.

2. **Documentation has duplicate operational claims.**  
   Key-printing behavior is described in README, DEPLOYMENT, OPERATIONS, and
   handoff. One script change left several docs inconsistent.

3. **Large files carry many unrelated responsibilities.**  
   `state.rs`, `main.tsx`, and `probe.ts` grew through feature accretion. This
   makes Agent patches more likely to touch unrelated behavior and harder to
   review.

4. **Evidence is strong but can become stale.**  
   Platform smoke evidence is valuable, especially where it marks experimental
   or failed cases. It should be dated and treated as historical unless a fresh
   run confirms the platform still works.

5. **Validation claims drift from validation commands.**  
   The handoff says one command revalidates a broad feature set, while the
   script is narrower. Every future "verified" claim should name exact commands
   and note missing dependencies.

6. **AI-style broad append behavior is visible.**  
   Recent commits added many features and docs at once. That is acceptable only
   when the next pass does tight review, line-level findings, and focused
   follow-up commits.

## Recommended Repair Order

Status after follow-up maintenance in the same session:

- `install.sh` now refuses protected parent/system directories, symlink targets,
  non-directory targets, and non-empty non-Git application directories instead
  of deleting them. Existing Git checkouts now fail on local changes and use
  fast-forward merge instead of `reset --hard`.
- README, deployment docs, workflow, and handoff now describe bootstrap key logs
  as hidden by default.
- `scripts/check.ps1` now uses `npm ci` for missing Node dependencies and covers
  sidecar check/build, dashboard build, PowerShell syntax, optional Bash syntax,
  and `git diff --check`.
- `docs/NEXT_AGENT_HANDOFF.md` now records the current local CI baseline
  `04f4882` / run `27593628977` and marks VPS state as not revalidated in this
  pass.

Remaining repair order:

1. Split or refactor the largest files only after the operational risks above
   are fixed and covered by tests.
2. Add targeted regression tests for archive file auth, dashboard archive open,
   cache cleanup candidates, and Docker/bootstrap key behavior.

## Next-Agent Rules

- Do not delete or "clean up" evidence and handoff files unless a separate
  review has classified them as obsolete and the user approves removal.
- Do not claim VPS health, platform support, VRChat playback, Docker health, or
  live smoke success without current commands and output.
- Keep `experimental`, `failed`, `region_blocked`, and `needs_profile` states
  explicit.
- Never put API keys, Cookies, Profile data, SSH keys, `.env`, SQLite, or
  `storage/` contents into committed docs.
- Treat generated local folders (`target/`, `storage/`, `node_modules/`,
  sidecar `dist/`, dashboard build output) as ignored operational state, not as
  source.
