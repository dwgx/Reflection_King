# SSRF

SSRF controls must apply to:

- User submitted URLs.
- Redirect targets.
- Extractor-produced media URLs.
- Playlist segment URLs.
- Webhook or callback URLs.

Application checks are not enough for production. Fetch workers should also run with network egress restrictions that deny private networks and metadata endpoints.
