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

### Hardened

- `flush()` now drains all pending entries, not just one batch.
- HTTP transport uses bounded connect/read timeouts.
- 4xx ingest responses are dropped without retry; 5xx/network failures retry with caps.
- `Drop` is bounded and best-effort; deterministic flushing is documented through `shutdown()`.
- `init()` now returns `AlreadyInitialized` on double initialization.
- Runtime `set_trace_id` and `set_global_metadata` helpers.
- Non-object metadata is wrapped instead of silently dropped.
- `tracing` integration includes active span context.
