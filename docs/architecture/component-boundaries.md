# Component Boundaries

`reflection-core` owns shared policy and media mechanics.

`reflection-api` owns HTTP routing and in-process job orchestration.

`reflection-worker` will own standalone queue consumption once the job store has
a lease or claim protocol for multiple consumers.

Future extractor integrations must sit behind a narrow interface and return candidate media URLs that are re-validated before fetching.
