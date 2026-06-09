# Contributing

## Expectations

- Keep URL fetching, extractor, transcoding, storage, and auth changes small enough to review.
- Update docs when changing public API, runtime config, storage layout, or security policy.
- Add tests for URL policy and media handling changes.

## Local Checks

```powershell
.\scripts\check.ps1
```

## Security Review Triggers

Request an explicit security review for changes touching:

- URL parsing, DNS resolution, redirects, allowlists, or deny rules.
- Downloader and extractor integrations.
- FFmpeg arguments or sandboxing.
- API auth, API keys, secrets, logs, or queue payloads.
- Storage cleanup, retention, or public media serving.
