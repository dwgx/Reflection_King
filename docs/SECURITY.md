# Security

Media backends are high-risk because they fetch arbitrary remote URLs and run heavy native codecs.

## Implemented Baseline

- Only `http` and `https` sources are accepted.
- Hostnames are resolved before fetch.
- Private, loopback, link-local, multicast, documentation, and reserved network targets are blocked.
- Redirects are followed manually and each redirect target is validated.
- Download size is capped by `RK_MAX_DOWNLOAD_MB`.
- `text/html` responses are rejected to catch webpage URLs instead of direct media URLs.
- `POST /api/jobs` can require `x-api-key` via `RK_API_KEY`.
- Browser Profile login is controlled through authenticated API routes. The
  dashboard receives screenshots and sends click/key events; Cookie values stay
  inside the server-side Playwright Profile.

## Required Before Public Use

- Put the service behind HTTPS.
- Set `RK_API_KEY` or real auth.
- Add per-IP and per-user rate limits.
- Add domain policy: allowlist, denylist, or user trust tiers.
- Run workers in a constrained account/container.
- Put `storage/` on a disk with quota.
- Add cleanup for old jobs and failed temp files.
- Log job IDs, not secrets or private URLs.
- Keep any browser CDP/VNC/debug ports bound to localhost or an internal
  network. Do not expose them directly to the public internet.

## Copyright And Authorization

The backend must not be positioned as a bypass tool. Users should only process media they own, created, licensed, or otherwise have permission to use. Public sharing to VRChat or other platforms may require additional rights.

Profile cookies are an authorization aid for content the operator can already
access. They must not be used to bypass DRM, paywalls, captcha, 2FA, regional
access controls, or legal age gates.

## SSRF Notes

SSRF checks must be repeated after every redirect and after every future extractor step. Platform extractors can return new media URLs; those URLs need the same validation as the original input.
