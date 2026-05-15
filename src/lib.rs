//! Auralogs Rust SDK beta.
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

pub use config::{AuralogsConfig, AuralogsConfigBuilder};
pub use entry::{LogEntry, LogLevel};
pub use error::{AuralogsError, Result};
pub use global::GlobalMetadata;
#[cfg(feature = "log")]
pub use log_bridge::{install_log_logger, AuralogsLogLogger};
#[cfg(feature = "tracing")]
pub use tracing_layer::AuralogsLayer;

use once_cell::sync::OnceCell;
use serde::Serialize;
use serde_json::{Map, Value};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use transport::Transport;
use uuid::Uuid;

static GLOBAL: OnceCell<Arc<Auralogs>> = OnceCell::new();

/// Thread-safe Auralogs client.
#[derive(Debug)]
pub struct Auralogs {
    environment: String,
    trace_id: RwLock<String>,
    global_metadata: RwLock<Option<GlobalMetadata>>,
    warned_metadata: Mutex<bool>,
    shutdown_timeout: Duration,
    transport: Transport,
}

impl Auralogs {
    /// Create a client without installing it as the global singleton.
    pub fn new(config: AuralogsConfig) -> Result<Arc<Self>> {
        let trace_id = config
            .trace_id
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let transport = Transport::new(config.transport_config())?;
        let shutdown_timeout = config.shutdown_timeout;
        let client = Arc::new(Self {
            environment: config.environment,
            trace_id: RwLock::new(trace_id),
            global_metadata: RwLock::new(config.global_metadata),
            warned_metadata: Mutex::new(false),
            shutdown_timeout,
            transport,
        });

        #[cfg(feature = "panic-capture")]
        if config.capture_panics {
            panic_capture::install(client.clone());
        }

        Ok(client)
    }

    /// Create and install the global client used by module-level logging calls.
    pub fn init(config: AuralogsConfig) -> Result<Arc<Self>> {
        if GLOBAL.get().is_some() {
            return Err(AuralogsError::AlreadyInitialized);
        }
        let client = Self::new(config)?;
        if GLOBAL.set(client.clone()).is_err() {
            client.shutdown();
            return Err(AuralogsError::AlreadyInitialized);
        }
        Ok(client)
    }

    /// Return the global client if one has been installed.
    pub fn global() -> Option<Arc<Self>> {
        GLOBAL.get().cloned()
    }

    pub fn trace_id(&self) -> String {
        self.trace_id
            .read()
            .expect("auralogs trace_id poisoned")
            .clone()
    }

    pub fn set_trace_id(&self, trace_id: impl Into<String>) {
        *self.trace_id.write().expect("auralogs trace_id poisoned") = trace_id.into();
    }

    pub fn set_global_metadata(&self, global_metadata: Option<GlobalMetadata>) {
        *self
            .global_metadata
            .write()
            .expect("auralogs global_metadata poisoned") = global_metadata;
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
            self.trace_id(),
        );
        self.transport.send(entry);
    }

    pub fn flush(&self) {
        self.transport.flush();
    }

    pub fn shutdown(&self) {
        self.transport.shutdown_with_timeout(self.shutdown_timeout);
    }

    pub fn shutdown_with_timeout(&self, timeout: Duration) {
        self.transport.shutdown_with_timeout(timeout);
    }

    fn merge_metadata<M>(&self, per_call: M, include_global_metadata: bool) -> Option<Value>
    where
        M: Serialize,
    {
        let global_metadata = self
            .global_metadata
            .read()
            .expect("auralogs global_metadata poisoned")
            .clone();
        let mut out = match include_global_metadata
            .then(|| global_metadata.as_ref().and_then(GlobalMetadata::read))
            .flatten()
        {
            Some(Value::Object(map)) => map,
            _ => Map::new(),
        };

        match serde_json::to_value(per_call) {
            Ok(Value::Object(map)) => {
                for (key, value) in map {
                    out.insert(key, value);
                }
            }
            Ok(Value::Null) => {}
            Ok(value) => {
                out.insert("value".to_string(), value);
            }
            Err(err) => {
                self.warn_metadata_once(&format!("auralogs: failed to serialize metadata: {err}"));
            }
        }

        if out.is_empty() {
            None
        } else {
            Some(Value::Object(out))
        }
    }

    fn warn_metadata_once(&self, message: &str) {
        let mut warned = self
            .warned_metadata
            .lock()
            .expect("auralogs warned_metadata poisoned");
        if !*warned {
            eprintln!("{message}");
            *warned = true;
        }
    }
}

pub fn init(config: AuralogsConfig) -> Result<Arc<Auralogs>> {
    Auralogs::init(config)
}

pub fn global() -> Option<Arc<Auralogs>> {
    Auralogs::global()
}

pub fn shutdown() {
    if let Some(client) = Auralogs::global() {
        client.shutdown();
    }
}

pub fn info<M>(message: impl Into<String>, metadata: M)
where
    M: Serialize,
{
    if let Some(client) = Auralogs::global() {
        client.info(message, metadata);
    }
}

pub fn error<M>(message: impl Into<String>, metadata: M)
where
    M: Serialize,
{
    if let Some(client) = Auralogs::global() {
        client.error(message, metadata);
    }
}

pub mod prelude {
    pub use crate::{Auralogs, AuralogsConfig, AuralogsConfigBuilder, GlobalMetadata, LogLevel};
}
