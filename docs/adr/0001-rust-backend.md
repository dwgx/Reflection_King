# ADR 0001: Rust Backend

## Status

Accepted

## Decision

Use Rust for the backend and split the project into a Cargo workspace.

## Rationale

The project needs strong control over concurrency, process execution, file paths, and URL safety. Rust gives a good fit for a media backend that will eventually run heavy asynchronous IO and worker tasks.
