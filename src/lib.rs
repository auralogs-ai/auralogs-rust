//! Auralog Rust SDK beta.
//!
//! The core API is synchronous and runtime-agnostic. Network delivery happens
//! on a dedicated background thread so callers can use the SDK from CLI apps,
//! Tokio services, framework code, or libraries without coupling to an async
//! runtime.

mod config;
mod entry;
mod error;
mod global;
#[cfg(feature = "log")]
mod log_bridge;
#[cfg(feature = "panic-capture")]
mod panic_capture;
#[cfg(feature = "tracing")]
mod tracing_layer;
mod transport;

pub use config::{AuralogConfig, AuralogConfigBuilder};
pub use entry::{LogEntry, LogLevel};
pub use error::{AuralogError, Result};
pub use global::{GlobalMetadata, MetadataMap};
#[cfg(feature = "log")]
pub use log_bridge::{install_log_logger, AuralogLogLogger};
#[cfg(feature = "tracing")]
pub use tracing_layer::AuralogLayer;

use once_cell::sync::OnceCell;
use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::Arc;
use transport::Transport;
use uuid::Uuid;

static GLOBAL: OnceCell<Arc<Auralog>> = OnceCell::new();

/// Thread-safe Auralog client.
#[derive(Debug)]
pub struct Auralog {
    environment: String,
    trace_id: parking_trace_id::TraceId,
    global_metadata: Option<GlobalMetadata>,
    transport: Transport,
}

impl Auralog {
    /// Create a client without installing it as the global singleton.
    pub fn new(config: AuralogConfig) -> Result<Arc<Self>> {
        let trace_id = config
            .trace_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let transport = Transport::new(config.transport_config())?;
        let client = Arc::new(Self {
            environment: config.environment,
            trace_id: parking_trace_id::TraceId(trace_id),
            global_metadata: config.global_metadata,
            transport,
        });

        #[cfg(feature = "panic-capture")]
        if config.capture_panics {
            panic_capture::install(client.clone());
        }

        Ok(client)
    }

    /// Create and install the global client used by module-level logging calls.
    pub fn init(config: AuralogConfig) -> Result<Arc<Self>> {
        let client = Self::new(config)?;
        let _ = GLOBAL.set(client.clone());
        Ok(client)
    }

    /// Return the global client if one has been installed.
    pub fn global() -> Option<Arc<Self>> {
        GLOBAL.get().cloned()
    }

    pub fn trace_id(&self) -> &str {
        &self.trace_id.0
    }

    pub fn debug<M>(&self, message: impl Into<String>, metadata: M)
    where
        M: Serialize,
    {
        self.log(LogLevel::Debug, message.into(), metadata, None);
    }

    pub fn info<M>(&self, message: impl Into<String>, metadata: M)
    where
        M: Serialize,
    {
        self.log(LogLevel::Info, message.into(), metadata, None);
    }

    pub fn warn<M>(&self, message: impl Into<String>, metadata: M)
    where
        M: Serialize,
    {
        self.log(LogLevel::Warn, message.into(), metadata, None);
    }

    pub fn error<M>(&self, message: impl Into<String>, metadata: M)
    where
        M: Serialize,
    {
        self.log(LogLevel::Error, message.into(), metadata, None);
    }

    pub fn error_with_stack<M>(
        &self,
        message: impl Into<String>,
        metadata: M,
        stack_trace: impl Into<String>,
    ) where
        M: Serialize,
    {
        self.log(
            LogLevel::Error,
            message.into(),
            metadata,
            Some(stack_trace.into()),
        );
    }

    pub fn fatal<M>(&self, message: impl Into<String>, metadata: M)
    where
        M: Serialize,
    {
        self.log(LogLevel::Fatal, message.into(), metadata, None);
    }

    pub fn log<M>(&self, level: LogLevel, message: String, metadata: M, stack_trace: Option<String>)
    where
        M: Serialize,
    {
        self.log_inner(level, message, metadata, stack_trace, true);
    }

    pub(crate) fn log_without_global_metadata<M>(
        &self,
        level: LogLevel,
        message: String,
        metadata: M,
        stack_trace: Option<String>,
    ) where
        M: Serialize,
    {
        self.log_inner(level, message, metadata, stack_trace, false);
    }

    fn log_inner<M>(
        &self,
        level: LogLevel,
        message: String,
        metadata: M,
        stack_trace: Option<String>,
        include_global_metadata: bool,
    ) where
        M: Serialize,
    {
        let metadata = self.merge_metadata(metadata, include_global_metadata);
        let entry = LogEntry::new(
            level,
            message,
            self.environment.clone(),
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            metadata,
            stack_trace,
            self.trace_id.0.clone(),
        );
        self.transport.send(entry);
    }

    pub fn flush(&self) {
        self.transport.flush();
    }

    pub fn shutdown(&self) {
        self.transport.shutdown();
    }

    fn merge_metadata<M>(&self, per_call: M, include_global_metadata: bool) -> Option<Value>
    where
        M: Serialize,
    {
        let mut out = match include_global_metadata
            .then(|| self.global_metadata.as_ref().and_then(GlobalMetadata::read))
            .flatten()
        {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };

        if let Ok(Value::Object(map)) = serde_json::to_value(per_call) {
            for (key, value) in map {
                out.insert(key, value);
            }
        }

        if out.is_empty() {
            None
        } else {
            Some(Value::Object(out))
        }
    }
}

pub fn init(config: AuralogConfig) -> Result<Arc<Auralog>> {
    Auralog::init(config)
}

pub fn global() -> Option<Arc<Auralog>> {
    Auralog::global()
}

pub fn shutdown() {
    if let Some(client) = Auralog::global() {
        client.shutdown();
    }
}

pub fn info<M>(message: impl Into<String>, metadata: M)
where
    M: Serialize,
{
    if let Some(client) = Auralog::global() {
        client.info(message, metadata);
    }
}

pub fn error<M>(message: impl Into<String>, metadata: M)
where
    M: Serialize,
{
    if let Some(client) = Auralog::global() {
        client.error(message, metadata);
    }
}

mod parking_trace_id {
    #[derive(Debug)]
    pub(crate) struct TraceId(pub(crate) String);
}
