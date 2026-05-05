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
const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

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
    pub(crate) http_timeout: Duration,
    pub(crate) shutdown_timeout: Duration,
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
            http_timeout: self.http_timeout,
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
    http_timeout: Option<Duration>,
    shutdown_timeout: Option<Duration>,
    trace_id: Option<String>,
    global_metadata: Option<GlobalMetadata>,
    allow_insecure_endpoint: Option<bool>,
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

    pub fn http_timeout(mut self, value: Duration) -> Self {
        self.http_timeout = Some(value);
        self
    }

    pub fn shutdown_timeout(mut self, value: Duration) -> Self {
        self.shutdown_timeout = Some(value);
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

    /// Allow plaintext (non-HTTPS) endpoints. Defaults to `false`.
    ///
    /// By default, [`AuralogConfigBuilder::build`] rejects endpoints whose
    /// scheme is not `https://` so that a misconfigured endpoint cannot
    /// silently downgrade every POST to plaintext. Set this to `true`
    /// explicitly when you need to talk to a local development server or
    /// an internal HTTP-only ingest.
    pub fn allow_insecure_endpoint(mut self, value: bool) -> Self {
        self.allow_insecure_endpoint = Some(value);
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

        let allow_insecure_endpoint = self.allow_insecure_endpoint.unwrap_or(false);
        // Per RFC 3986 §3.1, URI scheme comparison is case-insensitive.
        // `HTTPS://example.com` and `https://example.com` denote the same
        // scheme, so we lowercase before checking. Only the scheme prefix
        // needs lowercasing, but Rust's `&str::to_lowercase` is allocation-
        // happy and the endpoint string is short, so we just lowercase the
        // whole leading slice.
        let trimmed_endpoint = endpoint.trim_start();
        let scheme_is_https = trimmed_endpoint
            .split_once("://")
            .map(|(scheme, _rest)| scheme.eq_ignore_ascii_case("https"))
            .unwrap_or(false);
        if !allow_insecure_endpoint && !scheme_is_https {
            return Err(AuralogError::InvalidConfig(
                "endpoint must use https://; pass allow_insecure_endpoint(true) to opt in to \
                 plaintext"
                    .to_string(),
            ));
        }

        let flush_interval = self.flush_interval.unwrap_or(DEFAULT_FLUSH_INTERVAL);
        let max_batch_size = self.max_batch_size.unwrap_or(DEFAULT_MAX_BATCH_SIZE);
        let max_queue_size = self.max_queue_size.unwrap_or(DEFAULT_MAX_QUEUE_SIZE);
        let max_retry_attempts = self
            .max_retry_attempts
            .unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS);
        let retry_initial_delay = self
            .retry_initial_delay
            .unwrap_or(DEFAULT_RETRY_INITIAL_DELAY);
        let retry_max_delay = self.retry_max_delay.unwrap_or(DEFAULT_RETRY_MAX_DELAY);
        let http_timeout = self.http_timeout.unwrap_or(DEFAULT_HTTP_TIMEOUT);
        let shutdown_timeout = self.shutdown_timeout.unwrap_or(DEFAULT_SHUTDOWN_TIMEOUT);

        validate_duration("flush_interval", flush_interval)?;
        validate_duration("retry_initial_delay", retry_initial_delay)?;
        validate_duration("retry_max_delay", retry_max_delay)?;
        validate_duration("http_timeout", http_timeout)?;
        validate_duration("shutdown_timeout", shutdown_timeout)?;
        if max_batch_size == 0 {
            return Err(AuralogError::InvalidConfig(
                "max_batch_size must be greater than zero".to_string(),
            ));
        }
        if max_queue_size == 0 {
            return Err(AuralogError::InvalidConfig(
                "max_queue_size must be greater than zero".to_string(),
            ));
        }
        if max_retry_attempts == 0 {
            return Err(AuralogError::InvalidConfig(
                "max_retry_attempts must be greater than zero".to_string(),
            ));
        }
        if retry_max_delay < retry_initial_delay {
            return Err(AuralogError::InvalidConfig(
                "retry_max_delay must be greater than or equal to retry_initial_delay".to_string(),
            ));
        }

        Ok(AuralogConfig {
            api_key,
            environment,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            flush_interval,
            max_batch_size,
            max_queue_size,
            max_retry_attempts,
            retry_initial_delay,
            retry_max_delay,
            http_timeout,
            shutdown_timeout,
            trace_id: self.trace_id,
            global_metadata: self.global_metadata,
            #[cfg(feature = "panic-capture")]
            capture_panics: self.capture_panics.unwrap_or(false),
        })
    }
}

fn validate_duration(name: &str, value: Duration) -> Result<()> {
    if value.is_zero() {
        return Err(AuralogError::InvalidConfig(format!(
            "{name} must be greater than zero"
        )));
    }
    Ok(())
}
