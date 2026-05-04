# auralog-rust (Beta)

Rust SDK for [Auralog](https://auralog.ai) — agentic logging and application awareness.

Auralog uses Claude as an on-call engineer: it monitors your logs and errors, alerts you when something's wrong, and opens fix PRs automatically.

[![license](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

## Install

```toml
[dependencies]
auralog = "0.1.0-beta.1"
serde_json = "1"
```

The beta targets Rust 1.86+.

## Quick Start

```rust
use auralog::prelude::*;
use serde_json::json;

let client = Auralog::init(
    AuralogConfig::builder()
        .api_key(std::env::var("AURALOG_API_KEY")?)
        .environment("production")
        .build()?,
)?;

client.info("user signed in", json!({ "user_id": "123" }));
client.error("payment failed", json!({ "order_id": "abc" }));
client.shutdown();
# Ok::<(), Box<dyn std::error::Error>>(())
```

The `prelude` exports `Auralog`, `AuralogConfig`, `AuralogConfigBuilder`, `GlobalMetadata`, and `LogLevel`.

## Configuration

| Option | Type | Default | Description |
|---|---|---|---|
| `api_key` | `String` | _required_ | Your Auralog project API key |
| `environment` | `String` | `"production"` | e.g. `"production"`, `"staging"`, `"dev"` |
| `endpoint` | `String` | `https://ingest.auralog.ai` | Ingest endpoint override |
| `flush_interval` | `Duration` | `5s` | Time between batched flushes |
| `max_batch_size` | `usize` | `50` | Maximum logs per batch request |
| `max_queue_size` | `usize` | `1000` | Maximum in-memory logs retained before dropping oldest entries |
| `max_retry_attempts` | `usize` | `5` | Drop a failed log after this many attempts |
| `retry_initial_delay` | `Duration` | `1s` | First retry delay |
| `retry_max_delay` | `Duration` | `30s` | Maximum retry delay |
| `http_timeout` | `Duration` | `30s` | Connect/read timeout for ingest requests |
| `shutdown_timeout` | `Duration` | `2s` | Maximum time `shutdown()` spends waiting for the worker and synchronous drain |
| `trace_id` | `String` | _auto-generated_ | Custom trace ID for distributed tracing |
| `global_metadata` | `GlobalMetadata` | _none_ | Static metadata or a sync supplier merged into every entry |
| `capture_panics` | `bool` | `false` | Install a panic hook and emit fatal logs before chaining to the previous hook |

## Global Metadata

Use `GlobalMetadata` to attach session-scoped fields to every log:

```rust
use auralog::GlobalMetadata;
use serde_json::json;

let config = AuralogConfig::builder()
    .api_key("aura_your_key")
    .global_metadata(GlobalMetadata::supplier(|| {
        json!({ "service": "checkout" })
    }))
    .build()?;
```

The supplier runs on every emit, so keep it cheap and side-effect-free. The SDK catches panics from the supplier and ships the log without global metadata rather than crashing the host.

Non-object per-call metadata is wrapped as `{ "value": ... }`, so scalar values and arrays are not silently discarded.

## Runtime Updates

```rust
client.set_trace_id("trace-123");
client.set_global_metadata(Some(GlobalMetadata::static_map(json!({
    "tenant": "acme"
}))));
```

## Panic Capture

Panic capture is opt-in during beta:

```rust
let config = AuralogConfig::builder()
    .api_key("aura_your_key")
    .capture_panics(true)
    .build()?;
```

Rust panic hooks run even for panics that are later caught with `catch_unwind`, so enabling panic capture can report caught panics as well as process-ending panics. The hook chains to the previous hook after enqueueing a fatal Auralog event.

Panic capture intentionally bypasses `global_metadata` suppliers. Panic hooks should avoid running user closures while the process may already be unwinding; the emitted event still includes panic metadata such as source, thread, and location.

## Transport Semantics

- Non-error logs are queued and flushed every `flush_interval`.
- Calling `client.flush()` synchronously drains all pending single and batch queues.
- Errors and fatals are prioritized onto `/v1/logs/single`.
- 4xx ingest responses are treated as permanent failures and are not retried.
- 5xx ingest responses and network failures are retried up to `max_retry_attempts`.
- Delivery failures are self-logged once with `eprintln!`.
- `Drop` is bounded and best-effort. Call `client.shutdown()` or `client.shutdown_with_timeout(...)` for deterministic flushing.

The default crate features include `ureq-transport`. Building without a transport feature is a compile-time error so the SDK cannot become a silent black-hole logger.

## `log` Integration

Enable the `log` feature:

```toml
auralog = { version = "0.1.0-beta.1", features = ["log"] }
```

```rust
let client = Auralog::init(config)?;
auralog::install_log_logger(client, log::LevelFilter::Info)?;
log::info!("payment processed");
```

Rust allows only one global `log` logger. If your app already installs `env_logger`, `tracing_log`, or another logger, `install_log_logger` will return `SetLoggerError`.

`log::Level::Trace` is sent to Auralog as `debug` because Auralog's cross-SDK wire levels are `debug`, `info`, `warn`, `error`, and `fatal`. The original Rust level is preserved in metadata as `rust_log_level`.

## `tracing` Integration

Enable the `tracing` feature:

```toml
auralog = { version = "0.1.0-beta.1", features = ["tracing"] }
```

Use `AuralogLayer` with your subscriber stack:

```rust
use tracing_subscriber::prelude::*;

let client = Auralog::init(config)?;
let subscriber = tracing_subscriber::registry().with(auralog::AuralogLayer::new(client));
tracing::subscriber::set_global_default(subscriber)?;
```

The SDK provides a layer rather than installing a subscriber directly so it composes with existing formatting, filtering, and OpenTelemetry layers.

Events include the active tracing span chain in `metadata.spans`, including span names, targets, source locations, and recorded span fields.

## Graceful Shutdown

The transport runs on a named background thread (`auralog-flush`). `Arc<Auralog>` values may be retained by the global singleton or panic hook, so dropping a handle is not a deterministic flush mechanism. Call `shutdown()` for deterministic flush in CLIs, tests, and serverless handlers:

```rust
client.shutdown();
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The release workflow expects `secrets.CARGO_REGISTRY_TOKEN` until crates.io trusted publishing is configured for this repository.

## Documentation

Full docs at [docs.auralog.ai](https://docs.auralog.ai).

## Security

Found a vulnerability? See [SECURITY.md](./SECURITY.md) for how to report it.

## License

[MIT](./LICENSE) © James Thomas
