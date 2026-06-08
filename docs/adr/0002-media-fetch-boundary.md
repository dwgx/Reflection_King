# ADR 0002: Media Fetch Boundary

## Status

Accepted for MVP

## Decision

All remote media fetching must go through `reflection-core` URL policy and downloader code.

## Rationale

Future extractors and live stream resolvers can produce secondary URLs. Every produced URL needs the same validation as user input to keep SSRF controls consistent.
