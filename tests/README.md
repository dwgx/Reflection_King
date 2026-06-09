# Tests

Planned integration coverage:

- Health endpoint.
- Job creation with and without API key.
- Direct `.m4a` to `.mp3` transcode.
- HTML source rejection.
- Private network URL rejection.
- Oversized download rejection.
- Static `/media` response headers.

Tests should use tiny checked-in fixtures or local test servers, not third-party media URLs.
