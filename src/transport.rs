use crate::entry::LogEntry;
use crate::error::{AuralogError, Result};
use serde_json::json;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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
}

#[derive(Debug)]
pub(crate) struct Transport {
    inner: Arc<Inner>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

#[derive(Debug)]
struct Inner {
    config: TransportConfig,
    state: Mutex<State>,
    wake: Condvar,
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
            return Err(AuralogError::InvalidConfig(
                "max_batch_size must be greater than zero".to_string(),
            ));
        }
        if config.max_queue_size == 0 {
            return Err(AuralogError::InvalidConfig(
                "max_queue_size must be greater than zero".to_string(),
            ));
        }

        let inner = Arc::new(Inner {
            config,
            state: Mutex::new(State::default()),
            wake: Condvar::new(),
        });
        let worker_inner = inner.clone();
        let worker = thread::Builder::new()
            .name("auralog-flush".to_string())
            .spawn(move || worker_inner.run())
            .map_err(|err| AuralogError::InvalidConfig(err.to_string()))?;

        Ok(Self {
            inner,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub(crate) fn send(&self, entry: LogEntry) {
        let immediate = entry.level.is_error_or_above();
        let mut state = self.inner.state.lock().expect("auralog state poisoned");
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
        self.inner.flush_once();
    }

    pub(crate) fn shutdown(&self) {
        {
            let mut state = self.inner.state.lock().expect("auralog state poisoned");
            state.stopped = true;
            self.inner.wake.notify_all();
        }
        if let Some(worker) = self.worker.lock().expect("auralog worker poisoned").take() {
            let _ = worker.join();
        }
        self.inner.flush_until_empty();
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn run(&self) {
        let mut retry_delay = self.config.retry_initial_delay;
        loop {
            let stopped = {
                let state = self.state.lock().expect("auralog state poisoned");
                let (state, _timeout) = self
                    .wake
                    .wait_timeout(state, self.config.flush_interval)
                    .expect("auralog condvar poisoned");
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

    fn flush_until_empty(&self) {
        loop {
            let has_entries = {
                let state = self.state.lock().expect("auralog state poisoned");
                !state.single_queue.is_empty() || !state.batch_queue.is_empty()
            };
            if !has_entries {
                break;
            }
            self.flush_once();
        }
    }

    fn flush_once(&self) -> bool {
        let (entries, single) = {
            let mut state = self.state.lock().expect("auralog state poisoned");
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

        let success = self.send_http(&entries, single);
        if !success {
            self.requeue(entries, single);
        }
        success
    }

    #[cfg(feature = "ureq-transport")]
    fn send_http(&self, entries: &[QueuedEntry], single: bool) -> bool {
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
        ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map(|response| (200..300).contains(&response.status()))
            .unwrap_or(false)
    }

    #[cfg(not(feature = "ureq-transport"))]
    fn send_http(&self, _entries: &[QueuedEntry], _single: bool) -> bool {
        true
    }

    fn requeue(&self, entries: Vec<QueuedEntry>, single: bool) {
        let mut retryable = Vec::new();
        for mut entry in entries {
            entry.attempts += 1;
            if entry.attempts < self.config.max_retry_attempts {
                retryable.push(entry);
            }
        }
        if retryable.is_empty() {
            return;
        }

        let mut state = self.state.lock().expect("auralog state poisoned");
        for entry in retryable.into_iter().rev() {
            if single {
                state.single_queue.push_front(entry);
            } else {
                state.batch_queue.push_front(entry);
            }
        }
    }
}

fn trim_for_capacity(state: &mut State, max_queue_size: usize) {
    while state.batch_queue.len() + state.single_queue.len() >= max_queue_size {
        if state.batch_queue.pop_front().is_none() {
            let _ = state.single_queue.pop_front();
        }
    }
}
