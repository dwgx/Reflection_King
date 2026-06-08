# URL Fetch Policy

Current MVP policy:

- Allowed schemes: `http`, `https`.
- Block private and reserved IP targets.
- Validate each redirect target.
- Reject HTML responses.
- Enforce max download bytes.

Future additions:

- Host allowlists and denylists.
- Allowed ports.
- DNS pinning/re-resolution policy.
- Per-tenant trust tiers.
- Extractor output re-validation.
