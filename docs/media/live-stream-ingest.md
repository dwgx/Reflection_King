# Live Stream Ingest

Live ingest needs stricter controls than direct file downloads:

- Maximum capture duration.
- Segment count and total byte limits.
- Retry and reconnect policy.
- Clock drift and discontinuity handling.
- Dead-letter handling for unstable streams.

Live support should not be added until persistent queueing and storage cleanup exist.
