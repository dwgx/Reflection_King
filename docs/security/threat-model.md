# Threat Model

Primary threats:

- SSRF into local network or cloud metadata services.
- Resource exhaustion through huge downloads or expensive transcodes.
- Abuse of public media hosting.
- Secret leakage through logs or job payloads.
- Processing or sharing unauthorized media.
- Native codec vulnerabilities in FFmpeg.

Mitigations should combine application checks, worker sandboxing, network egress policy, quotas, and observability.
