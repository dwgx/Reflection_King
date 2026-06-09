# Risk Register

| Risk | Severity | Owner | Mitigation | Status |
| --- | --- | --- | --- | --- |
| Unauthorized media processing | High | Product/Security | Authorization policy and takedown workflow | Open |
| Access-control bypass pressure | High | Product/Security | Explicit non-goals for DRM, captcha, paywall, login-wall, and enforcement bypass | Open |
| SSRF | High | Backend/Security | URL policy, redirect checks, network isolation | Started |
| Resource exhaustion | High | Backend/Infra | Quotas, queue limits, worker sandbox | Open |
| Browser probe overreach | High | Backend/Security | Separate queue, request budgets, no captcha solving, no sensitive body capture by default | Open |
| Public URL abuse | Medium | Backend | API key, rate limits, retention | Started |
| VRChat playback incompatibility | Medium | Media | Output contract tests and target player testing | Open |
