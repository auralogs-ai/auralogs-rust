use auralog::{Auralog, AuralogConfig, GlobalMetadata, LogLevel};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::panic;
use std::sync::{mpsc, Mutex};
use std::thread;
use std::time::Duration;
#[cfg(feature = "tracing")]
use tracing_subscriber::prelude::*;

static PANIC_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn config_requires_api_key() {
    assert!(AuralogConfig::builder().build().is_err());
}

#[test]
fn config_rejects_zero_durations_and_bad_retry_order() {
    assert!(AuralogConfig::builder()
        .api_key("aura_test")
        .flush_interval(Duration::ZERO)
        .build()
        .is_err());
    assert!(AuralogConfig::builder()
        .api_key("aura_test")
        .retry_initial_delay(Duration::from_secs(2))
        .retry_max_delay(Duration::from_secs(1))
        .build()
        .is_err());
}

#[test]
fn manual_logs_send_expected_wire_payloads() {
    let server = TestServer::start(2);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .environment("test")
            .endpoint(server.endpoint())
            .flush_interval(Duration::from_millis(10))
            .global_metadata(GlobalMetadata::static_map(json!({"service": "checkout"})))
            .build()
            .unwrap(),
    )
    .unwrap();

    client.info("started", json!({"order_id": "ord_1"}));
    client.error("failed", json!({"reason": "declined"}));
    client.shutdown();

    let requests = server.requests();
    assert!(requests.iter().any(|request| request.path == "/v1/logs"));
    assert!(requests
        .iter()
        .any(|request| request.path == "/v1/logs/single"));
    let batch = requests
        .iter()
        .find(|request| request.path == "/v1/logs")
        .unwrap();
    assert_eq!(batch.body["projectApiKey"], "aura_test");
    assert_eq!(batch.body["logs"][0]["level"], "info");
    assert_eq!(batch.body["logs"][0]["environment"], "test");
    assert_eq!(batch.body["logs"][0]["metadata"]["service"], "checkout");
    assert_eq!(batch.body["logs"][0]["metadata"]["order_id"], "ord_1");
    assert!(batch.body["logs"][0]["timestamp"]
        .as_str()
        .unwrap()
        .ends_with('Z'));
}

#[test]
fn global_metadata_supplier_panic_does_not_crash_logging() {
    let _guard = PANIC_TEST_LOCK.lock().unwrap();
    let server = TestServer::start(1);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .global_metadata(GlobalMetadata::supplier(|| panic!("bad supplier")))
            .build()
            .unwrap(),
    )
    .unwrap();

    client.error("still ships", json!({"ok": true}));
    client.shutdown();

    let requests = server.requests();
    assert_eq!(requests[0].body["log"]["message"], "still ships");
    assert_eq!(requests[0].body["log"]["metadata"]["ok"], true);
}

#[test]
fn flush_drains_all_batches() {
    let server = TestServer::start(3);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .max_batch_size(50)
            .build()
            .unwrap(),
    )
    .unwrap();

    for index in 0..120 {
        client.info("bulk", json!({"index": index}));
    }
    client.flush();

    let requests = server.requests();
    let total: usize = requests
        .iter()
        .map(|request| request.body["logs"].as_array().unwrap().len())
        .sum();
    assert_eq!(total, 120);
}

#[test]
fn four_xx_failures_are_not_retried() {
    let server = TestServer::with_statuses(vec![401]);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("bad_key")
            .endpoint(server.endpoint())
            .max_retry_attempts(3)
            .retry_initial_delay(Duration::from_millis(1))
            .retry_max_delay(Duration::from_millis(1))
            .build()
            .unwrap(),
    )
    .unwrap();

    client.info("bad auth", json!({}));
    client.flush();

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/v1/logs");
}

#[test]
fn retryable_failures_stop_after_attempt_limit() {
    let server = TestServer::with_statuses(vec![500, 500]);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .max_retry_attempts(2)
            .retry_initial_delay(Duration::from_millis(1))
            .retry_max_delay(Duration::from_millis(1))
            .build()
            .unwrap(),
    )
    .unwrap();

    client.info("server failing", json!({}));
    client.flush();

    assert_eq!(server.requests().len(), 2);
}

#[test]
fn queue_trims_oldest_entries_under_pressure() {
    let server = TestServer::start(1);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .max_queue_size(2)
            .max_batch_size(10)
            .build()
            .unwrap(),
    )
    .unwrap();

    for index in 0..5 {
        client.info("trim", json!({"index": index}));
    }
    client.flush();

    let requests = server.requests();
    let logs = requests[0].body["logs"].as_array().unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0]["metadata"]["index"], 3);
    assert_eq!(logs[1]["metadata"]["index"], 4);
}

#[test]
fn runtime_trace_and_global_metadata_can_change() {
    let server = TestServer::start(2);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .global_metadata(GlobalMetadata::static_map(json!({"service": "one"})))
            .trace_id("trace-one")
            .max_batch_size(1)
            .build()
            .unwrap(),
    )
    .unwrap();

    client.info("first", json!({}));
    client.set_trace_id("trace-two");
    client.set_global_metadata(Some(GlobalMetadata::static_map(json!({"service": "two"}))));
    client.info("second", json!({}));
    client.flush();

    let requests = server.requests();
    assert_eq!(requests[0].body["logs"][0]["traceId"], "trace-one");
    assert_eq!(requests[0].body["logs"][0]["metadata"]["service"], "one");
    assert_eq!(requests[1].body["logs"][0]["traceId"], "trace-two");
    assert_eq!(requests[1].body["logs"][0]["metadata"]["service"], "two");
}

#[test]
fn non_object_metadata_is_wrapped() {
    let server = TestServer::start(1);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .build()
            .unwrap(),
    )
    .unwrap();

    client.info("scalar", "hello");
    client.flush();

    let requests = server.requests();
    assert_eq!(requests[0].body["logs"][0]["metadata"]["value"], "hello");
}

#[test]
fn panic_hook_emits_fatal_entry() {
    let _guard = PANIC_TEST_LOCK.lock().unwrap();
    let server = TestServer::start(1);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .capture_panics(true)
            .shutdown_timeout(Duration::from_secs(1))
            .build()
            .unwrap(),
    )
    .unwrap();

    let _ = panic::catch_unwind(|| panic!("boom"));
    client.shutdown();

    let requests = server.requests();
    assert_eq!(requests[0].body["log"]["level"], "fatal");
    assert_eq!(requests[0].body["log"]["metadata"]["source"], "rust_panic");
    panic::set_hook(Box::new(|_| {}));
}

#[cfg(feature = "tracing")]
#[test]
fn tracing_layer_includes_span_context() {
    let server = TestServer::start(1);
    let client = Auralog::new(
        AuralogConfig::builder()
            .api_key("aura_test")
            .endpoint(server.endpoint())
            .build()
            .unwrap(),
    )
    .unwrap();
    let subscriber =
        tracing_subscriber::registry().with(auralog::AuralogLayer::new(client.clone()));

    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", request_id = "req_1");
        let _guard = span.enter();
        tracing::info!(user_id = "u_1", "handled");
    });
    client.shutdown();

    let requests = server.requests();
    let metadata = &requests[0].body["logs"][0]["metadata"];
    assert_eq!(metadata["source"], "rust_tracing");
    assert_eq!(metadata["user_id"], "u_1");
    assert_eq!(metadata["spans"][0]["name"], "request");
    assert_eq!(metadata["spans"][0]["fields"]["request_id"], "req_1");
}

#[test]
fn log_level_serializes_lowercase() {
    assert_eq!(serde_json::to_value(LogLevel::Fatal).unwrap(), "fatal");
}

struct TestServer {
    endpoint: String,
    receiver: mpsc::Receiver<Request>,
    handle: thread::JoinHandle<()>,
    expected: usize,
}

impl TestServer {
    fn start(expected: usize) -> Self {
        Self::with_statuses(vec![204; expected])
    }

    fn with_statuses(statuses: Vec<u16>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = mpsc::channel();
        let expected = statuses.len();
        let handle = thread::spawn(move || {
            for status in statuses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0_u8; 1024];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..read]);
                    if request_complete(&bytes) {
                        break;
                    }
                }
                let raw = String::from_utf8(bytes).unwrap();
                let request = parse_request(&raw);
                sender.send(request).unwrap();
                let reason = if status == 204 {
                    "No Content"
                } else if status == 401 {
                    "Unauthorized"
                } else {
                    "Internal Server Error"
                };
                let response = format!("HTTP/1.1 {status} {reason}\r\nContent-Length: 0\r\n\r\n");
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        Self {
            endpoint,
            receiver,
            handle,
            expected,
        }
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn requests(self) -> Vec<Request> {
        let mut out = Vec::new();
        for _ in 0..self.expected {
            out.push(self.receiver.recv_timeout(Duration::from_secs(2)).unwrap());
        }
        self.handle.join().unwrap();
        out
    }
}

#[derive(Debug)]
struct Request {
    path: String,
    body: serde_json::Value,
}

fn request_complete(bytes: &[u8]) -> bool {
    let raw = String::from_utf8_lossy(bytes);
    let Some((headers, body)) = raw.split_once("\r\n\r\n") else {
        return false;
    };
    let len = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("Content-Length: ")
                .and_then(|value| value.parse::<usize>().ok())
        })
        .unwrap_or(0);
    body.len() >= len
}

fn parse_request(raw: &str) -> Request {
    let (headers, body) = raw.split_once("\r\n\r\n").unwrap();
    let path = headers
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .to_string();
    Request {
        path,
        body: serde_json::from_str(body).unwrap(),
    }
}
