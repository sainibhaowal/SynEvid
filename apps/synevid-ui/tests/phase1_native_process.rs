//! Real-process Phase 1 contract test.
//!
//! This test deliberately starts the `synevid-ui` binary instead of calling
//! `DashboardServer` directly. It proves attach/status, authenticated-ready
//! health, live publication, cursor replay, and restart persistence against the
//! same local SQLite database.

use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn reserve_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve local port");
    listener.local_addr().unwrap().port()
}

fn http_json(port: u16, method: &str, path: &str, body: Option<&str>) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("daemon accepts HTTP");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let payload = response
        .split_once("\r\n\r\n")
        .map(|(_, payload)| payload)
        .expect("HTTP response has headers");
    serde_json::from_str(payload).unwrap_or_else(|error| {
        panic!("{method} {path} returned invalid JSON: {error}; payload={payload:?}")
    })
}

fn wait_for_health(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            let request =
                b"GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
            if stream.write_all(request).is_ok() {
                let mut response = String::new();
                if stream.read_to_string(&mut response).is_ok()
                    && response.starts_with("HTTP/1.1 200")
                {
                    return;
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("daemon did not become healthy on port {port}");
}

fn start_daemon(root: &Path, port: u16) -> Child {
    let child = Command::new(env!("CARGO_BIN_EXE_synevid-ui"))
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--repo-root",
            root.to_str().expect("UTF-8 test path"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn native synevid-ui daemon");
    wait_for_health(port);
    child
}

#[test]
fn phase1_native_process_attach_status_replay_and_restart() {
    let root =
        std::env::temp_dir().join(format!("synevid-native-process-e2e-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.ts"),
        "export function nativeProcessProof(value: number) { return value + 1; }\n",
    )
    .unwrap();
    let port = reserve_port();

    let mut daemon = start_daemon(&root, port);
    let canonical_root = root
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let canonical_root = canonical_root
        .strip_prefix("//?/")
        .unwrap_or(&canonical_root)
        .to_string();
    let attached = http_json(
        port,
        "POST",
        "/api/v1/repos/connect",
        Some(
            &serde_json::json!({
                "root_path": canonical_root,
                "display_name": "native-process-test"
            })
            .to_string(),
        ),
    );
    assert!(attached["repo_id"].as_str().is_some());
    assert!(attached["canonical_path"].as_str().is_some());
    let status = http_json(port, "GET", "/api/v1/status", None);
    assert_eq!(status["daemon_connection"], "connected");
    assert_eq!(status["canonical_path"], canonical_root);
    let before = status["last_event_seq"].as_i64().unwrap_or(0);

    let broadcast = http_json(
        port,
        "POST",
        "/api/v1/live/broadcast",
        Some(
            &serde_json::json!({
                "event": "native_process.proof",
                "data": { "source": "real-process-test" }
            })
            .to_string(),
        ),
    );
    let published = broadcast["seq_id"].as_i64().expect("published sequence");
    assert!(published > before);
    let replay = http_json(
        port,
        "GET",
        &format!("/api/v1/live/poll?since_seq={before}"),
        None,
    );
    assert!(
        replay["events"].as_array().unwrap().iter().any(|event| {
            event["seq_id"] == published && event["event"] == "native_process.proof"
        })
    );
    daemon.kill().unwrap();
    let _ = daemon.wait();

    let mut restarted = start_daemon(&root, port);
    let restarted_status = http_json(port, "GET", "/api/v1/status", None);
    assert_eq!(restarted_status["canonical_path"], canonical_root);
    assert!(restarted_status["last_event_seq"].as_i64().unwrap() >= published);
    let restarted_replay = http_json(
        port,
        "GET",
        &format!("/api/v1/live/poll?since_seq={before}"),
        None,
    );
    assert!(
        restarted_replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| {
                event["seq_id"] == published && event["event"] == "native_process.proof"
            })
    );
    restarted.kill().unwrap();
    let _ = restarted.wait();
    let _ = fs::remove_dir_all(root);
}
