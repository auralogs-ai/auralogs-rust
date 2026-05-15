use crate::entry::LogEntry;
use crate::error::{AuralogsError, Result};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(not(feature = "ureq-transport"))]
compile_error!(
    "auralogs requires a transport feature. Enable the default `ureq-transport` feature."
);

#[derive(Debug, Clone)]
pub(crate) struct TransportConfig {
    pub(crate) api_key: String,
    pub(crate) endpoint: String,
    pub(crate) flush_interval: Duration,
    pub(crate) max_batch_size: usize,
    pub(crate) max_queue_size: usize,
    pub(crate) max_retry_attempts: usize,
    pub(crate) retry_initial_delay: Duration,
    pub(crate) retry_max_delay: Duration,
    pub(crate) http_timeout: Duration,
}

#[derive(Debug)]
pub(crate) struct Transport {
    inner: Arc<Inner>,
    worker: Mutex<Option<JoinHandle<()>>>,
    worker_done: Mutex<Option<Receiver<()>>>,
}

#[derive(Debug)]
struct Inner {
    config: TransportConfig,
    #[cfg(feature = "ureq-transport")]
    agent: ureq::Agent,
    state: Mutex<State>,
    wake: Condvar,
    warned_failure: Mutex<bool>,
}

#[derive(Debug, Default)]
struct State {
    batch_queue: VecDeque<QueuedEntry>,
    single_queue: VecDeque<QueuedEntry>,
    stopped: bool,
}

#[derive(Debug, Clone)]
struct QueuedEntry {
    entry: LogEntry,
    attempts: usize,
}

impl Transport {
    pub(crate) fn new(config: TransportConfig) -> Result<Self> {
        if config.max_batch_size == 0 {
            return Err(AuralogsError::InvalidConfig(
                "max_batch_size must be greater than zero".to_string(),
            ));
        }
        if config.max_queue_size == 0 {
            return Err(AuralogsError::InvalidConfig(
                "max_queue_size must be greater than zero".to_string(),
            ));
        }

        let inner = Arc::new(Inner {
            #[cfg(feature = "ureq-transport")]
            // Disable redirect following: ureq 2.x defaults to up to 5
            // redirects. While 30x responses are downgraded to GET with no
            // body (so projectApiKey is not replayed), the redirected URL
            // still leaks via the Host header and a malicious server can
            // force redirects for SSRF reconnaissance. Refuse outright.
            agent: ureq::AgentBuilder::new()
                .redirects(0)
                .timeout_connect(config.http_timeout)
                .timeout_read(config.http_timeout)
                .build(),
            config,
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
            warned_failure: Mutex::new(false),
        });
        let (done_sender, done_receiver) = mpsc::channel();
        // The worker only owns `Inner` and never reaches back into `Auralogs`,
        // so starting it here is independent of the outer Arc<Auralogs>
        // construction order.
        let worker_inner = inner.clone();
        let worker = thread::Builder::new()
            .name("auralogs-flush".to_string())
            .spawn(move || {
                worker_inner.run();
                let _ = done_sender.send(());
            })?;

        Ok(Self {
            inner,
            worker: Mutex::new(Some(worker)),
            worker_done: Mutex::new(Some(done_receiver)),
        })
    }

    pub(crate) fn send(&self, entry: LogEntry) {
        let immediate = entry.level.is_error_or_above();
        let mut state = self.inner.state.lock().expect("auralogs state poisoned");
        trim_for_capacity(&mut state, self.inner.config.max_queue_size);
        if immediate {
            state
                .single_queue
                .push_back(QueuedEntry { entry, attempts: 0 });
            self.inner.wake.notify_one();
        } else {
            state
                .batch_queue
                .push_back(QueuedEntry { entry, attempts: 0 });
        }
    }

    pub(crate) fn flush(&self) {
        self.inner.flush_until_empty(None);
    }

    pub(crate) fn shutdown_with_timeout(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        {
            let mut state = self.inner.state.lock().expect("auralogs state poisoned");
            state.stopped = true;
            self.inner.wake.notify_all();
        }
        if let Some(done) = self
            .worker_done
            .lock()
            .expect("auralogs worker_done poisoned")
            .take()
        {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let _ = done.recv_timeout(remaining);
        }
        if let Some(worker) = self.worker.lock().expect("auralogs worker poisoned").take() {
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
        self.inner.flush_until_empty(Some(deadline));
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.shutdown_with_timeout(Duration::from_millis(250));
    }
}

impl Inner {
    fn run(&self) {
        let mut retry_delay = self.config.retry_initial_delay;
        loop {
            let stopped = {
                let state = self.state.lock().expect("auralogs state poisoned");
                let (state, _timeout) = self
                    .wake
                    .wait_timeout(state, self.config.flush_interval)
                    .expect("auralogs condvar poisoned");
                state.stopped
            };
            if stopped {
                return;
            }

            if self.flush_once() {
                retry_delay = self.config.retry_initial_delay;
            } else {
                thread::sleep(retry_delay);
                retry_delay = std::cmp::min(retry_delay * 2, self.config.retry_max_delay);
            }
        }
    }

    fn flush_until_empty(&self, deadline: Option<Instant>) {
        loop {
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                self.warn_once("auralogs: shutdown/flush timed out with pending logs");
                break;
            }
            let has_entries = {
                let state = self.state.lock().expect("auralogs state poisoned");
                !state.single_queue.is_empty() || !state.batch_queue.is_empty()
            };
            if !has_entries {
                break;
            }
            let success = self.flush_once();
            if !success {
                let delay = self.config.retry_initial_delay;
                if deadline.is_some_and(|deadline| Instant::now() + delay > deadline) {
                    break;
                }
                thread::sleep(delay);
            }
        }
    }

    fn flush_once(&self) -> bool {
        let (entries, single) = {
            let mut state = self.state.lock().expect("auralogs state poisoned");
            if let Some(entry) = state.single_queue.pop_front() {
                (vec![entry], true)
            } else if !state.batch_queue.is_empty() {
                let mut entries = Vec::new();
                for _ in 0..self.config.max_batch_size {
                    if let Some(entry) = state.batch_queue.pop_front() {
                        entries.push(entry);
                    } else {
                        break;
                    }
                }
                (entries, false)
            } else {
                return true;
            }
        };

        let outcome = self.send_http(&entries, single);
        match outcome {
            SendOutcome::Success | SendOutcome::PermanentFailure => true,
            SendOutcome::RetryableFailure => {
                self.requeue(entries, single);
                false
            }
        }
    }

    #[cfg(feature = "ureq-transport")]
    fn send_http(&self, entries: &[QueuedEntry], single: bool) -> SendOutcome {
        let url = if single {
            format!("{}/v1/logs/single", self.config.endpoint)
        } else {
            format!("{}/v1/logs", self.config.endpoint)
        };
        let body = if single {
            json!({"projectApiKey": self.config.api_key, "log": entries[0].entry})
        } else {
            let logs: Vec<_> = entries.iter().map(|entry| &entry.entry).collect();
            json!({"projectApiKey": self.config.api_key, "logs": logs})
        };
        match self
            .agent
            .post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)
        {
            Ok(response) if (200..300).contains(&response.status()) => SendOutcome::Success,
            Ok(response) if (400..500).contains(&response.status()) => {
                self.warn_once(&format!(
                    "auralogs: dropping logs after non-retryable HTTP {} from ingest",
                    response.status()
                ));
                SendOutcome::PermanentFailure
            }
            Ok(response) => {
                self.warn_once(&format!(
                    "auralogs: retrying logs after HTTP {} from ingest",
                    response.status()
                ));
                SendOutcome::RetryableFailure
            }
            Err(ureq::Error::Status(status, _)) if (400..500).contains(&status) => {
                self.warn_once(&format!(
                    "auralogs: dropping logs after non-retryable HTTP {status} from ingest"
                ));
                SendOutcome::PermanentFailure
            }
            Err(err) => {
                self.warn_once(&format!(
                    "auralogs: retrying logs after delivery failure: {err}"
                ));
                SendOutcome::RetryableFailure
            }
        }
    }

    fn requeue(&self, entries: Vec<QueuedEntry>, single: bool) {
        let mut retryable = Vec::new();
        for mut entry in entries {
            entry.attempts += 1;
            if entry.attempts < self.config.max_retry_attempts {
                retryable.push(entry);
            } else {
                self.warn_once("auralogs: dropping logs after retry attempts exhausted");
            }
        }
        if retryable.is_empty() {
            return;
        }

        let mut state = self.state.lock().expect("auralogs state poisoned");
        for entry in retryable.into_iter().rev() {
            if single {
                state.single_queue.push_front(entry);
            } else {
                state.batch_queue.push_front(entry);
            }
        }
    }

    fn warn_once(&self, message: &str) {
        let mut warned = self.warned_failure.lock().expect("auralogs warned poisoned");
        if !*warned {
            eprintln!("{message}");
            *warned = true;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendOutcome {
    Success,
    RetryableFailure,
    PermanentFailure,
}

fn trim_for_capacity(state: &mut State, max_queue_size: usize) {
    while state.batch_queue.len() + state.single_queue.len() >= max_queue_size {
        if state.batch_queue.pop_front().is_none() {
            let _ = state.single_queue.pop_front();
        }
    }
}
