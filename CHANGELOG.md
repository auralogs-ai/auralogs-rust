# Changelog

All notable changes to `auralog` are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0-beta.1] - 2026-05-03

### Added

- Initial beta Rust SDK.
- Runtime-agnostic manual logging API with `debug`, `info`, `warn`, `error`, and `fatal`.
- Background-thread HTTP transport with batching, immediate error sending, bounded queues, retry attempts, and shutdown flush.
- Static and supplier-based global metadata.
- Panic capture hook that emits fatal logs and chains to the previous hook.
- Optional `log` integration via `AuralogLogLogger`.
- Optional `tracing` integration via `AuralogLayer`.
