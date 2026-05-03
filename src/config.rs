use crate::error::{AuralogError, Result};
use crate::global::GlobalMetadata;
use crate::transport::TransportConfig;
use std::time::Duration;

const DEFAULT_ENDPOINT: &str = "https://ingest.auralog.ai";
const DEFAULT_ENVIRONMENT: &str = "production";
const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const DEFAULT_MAX_BATCH_SIZE: usize = 50;
const DEFAULT_MAX_QUEUE_SIZE: usize = 1000;
const DEFAULT_MAX_RETRY_ATTEMPTS: usize = 5;
const DEFAULT_RETRY_INITIAL_DELAY: Duration = Duration::from_secs(1);
const DEFAULT_RETRY_MAX_DELAY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct AuralogConfig {
    pub(crate) api_key: String,
    pub(crate) environment: String,
    pub(crate) endpoint: String,
    pub(crate) flush_interval: Duration,
    pub(crate) max_batch_size: usize,
    pub(crate) max_queue_size: usize,
    pub(crate) max_retry_attempts: usize,
    pub(crate) retry_initial_delay: Duration,
    pub(crate) retry_max_delay: Duration,
    pub(crate) trace_id: Option<String>,
    pub(crate) global_metadata: Option<GlobalMetadata>,
    #[cfg(feature = "panic-capture")]
    pub(crate) capture_panics: bool,
}

impl AuralogConfig {
    pub fn builder() -> AuralogConfigBuilder {
        AuralogConfigBuilder::default()
    }

    pub(crate) fn transport_config(&self) -> TransportConfig {
        TransportConfig {
            api_key: self.api_key.clone(),
            endpoint: self.endpoint.clone(),
            flush_interval: self.flush_interval,
            max_batch_size: self.max_batch_size,
            max_queue_size: self.max_queue_size,
            max_retry_attempts: self.max_retry_attempts,
            retry_initial_delay: self.retry_initial_delay,
            retry_max_delay: self.retry_max_delay,
        }
    }
}

#[derive(Debug, Default)]
pub struct AuralogConfigBuilder {
    api_key: Option<String>,
    environment: Option<String>,
    endpoint: Option<String>,
    flush_interval: Option<Duration>,
    max_batch_size: Option<usize>,
    max_queue_size: Option<usize>,
    max_retry_attempts: Option<usize>,
    retry_initial_delay: Option<Duration>,
    retry_max_delay: Option<Duration>,
    trace_id: Option<String>,
    global_metadata: Option<GlobalMetadata>,
    #[cfg(feature = "panic-capture")]
    capture_panics: Option<bool>,
}

impl AuralogConfigBuilder {
    pub fn api_key(mut self, value: impl Into<String>) -> Self {
        self.api_key = Some(value.into());
        self
    }

    pub fn environment(mut self, value: impl Into<String>) -> Self {
        self.environment = Some(value.into());
        self
    }

    pub fn endpoint(mut self, value: impl Into<String>) -> Self {
        self.endpoint = Some(value.into());
        self
    }

    pub fn flush_interval(mut self, value: Duration) -> Self {
        self.flush_interval = Some(value);
        self
    }

    pub fn max_batch_size(mut self, value: usize) -> Self {
        self.max_batch_size = Some(value);
        self
    }

    pub fn max_queue_size(mut self, value: usize) -> Self {
        self.max_queue_size = Some(value);
        self
    }

    pub fn max_retry_attempts(mut self, value: usize) -> Self {
        self.max_retry_attempts = Some(value);
        self
    }

    pub fn retry_initial_delay(mut self, value: Duration) -> Self {
        self.retry_initial_delay = Some(value);
        self
    }

    pub fn retry_max_delay(mut self, value: Duration) -> Self {
        self.retry_max_delay = Some(value);
        self
    }

    pub fn trace_id(mut self, value: impl Into<String>) -> Self {
        self.trace_id = Some(value.into());
        self
    }

    pub fn global_metadata(mut self, value: GlobalMetadata) -> Self {
        self.global_metadata = Some(value);
        self
    }

    #[cfg(feature = "panic-capture")]
    pub fn capture_panics(mut self, value: bool) -> Self {
        self.capture_panics = Some(value);
        self
    }

    pub fn build(self) -> Result<AuralogConfig> {
        let api_key = self.api_key.ok_or(AuralogError::MissingApiKey)?;
        if api_key.trim().is_empty() {
            return Err(AuralogError::MissingApiKey);
        }

        let environment = self
            .environment
            .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_string());
        if environment.trim().is_empty() {
            return Err(AuralogError::MissingEnvironment);
        }

        let endpoint = self
            .endpoint
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
        if endpoint.trim().is_empty() {
            return Err(AuralogError::MissingEndpoint);
        }

        Ok(AuralogConfig {
            api_key,
            environment,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            flush_interval: self.flush_interval.unwrap_or(DEFAULT_FLUSH_INTERVAL),
            max_batch_size: self.max_batch_size.unwrap_or(DEFAULT_MAX_BATCH_SIZE),
            max_queue_size: self.max_queue_size.unwrap_or(DEFAULT_MAX_QUEUE_SIZE),
            max_retry_attempts: self
                .max_retry_attempts
                .unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS),
            retry_initial_delay: self
                .retry_initial_delay
                .unwrap_or(DEFAULT_RETRY_INITIAL_DELAY),
            retry_max_delay: self.retry_max_delay.unwrap_or(DEFAULT_RETRY_MAX_DELAY),
            trace_id: self.trace_id,
            global_metadata: self.global_metadata,
            #[cfg(feature = "panic-capture")]
            capture_panics: self.capture_panics.unwrap_or(false),
        })
    }
}
