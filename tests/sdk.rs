use auralog::{Auralog, AuralogConfig, GlobalMetadata, LogLevel};
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[test]
fn config_requires_api_key() {
    assert!(AuralogConfig::builder().build().is_err());
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
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (sender, receiver) = mpsc::channel();
        let handle = thread::spawn(move || {
            for _ in 0..expected {
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
                stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
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
