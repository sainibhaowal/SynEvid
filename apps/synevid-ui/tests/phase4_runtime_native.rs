//! Native-process Phase 4 runtime-authority contract.
//!
//! The daemon is a child process. CLI, MCP, and UI are represented by
//! authenticated HTTP clients with distinct client identities, so this test
//! exercises the wire contract rather than calling DashboardServer methods.

use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const TOKEN: &str = "phase4-native-runtime-token";

fn reserve_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("reserve local port");
    listener.local_addr().unwrap().port()
}

#[allow(clippy::too_many_arguments)]
fn request(
    port: u16,
    token: Option<&str>,
    client_id: &str,
    client_type: &str,
    lease_id: Option<&str>,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> (u16, Value) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("daemon accepts HTTP");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .unwrap();
    let body = body.unwrap_or("");
    let authorization = token
        .map(|value| format!("Authorization: Bearer {value}\r\n"))
        .unwrap_or_default();
    let lease = lease_id
        .map(|value| format!("X-Synevid-Lease-Id: {value}\r\n"))
        .unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nX-Synevid-Client-Id: {client_id}\r\nX-Synevid-Client-Type: {client_type}\r\n{lease}{authorization}Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    if status == 401 {
        assert!(
            response
                .lines()
                .any(|line| line.eq_ignore_ascii_case("Access-Control-Allow-Origin: *")),
            "authenticated browser clients must be able to read structured 401 responses"
        );
    }
    let payload = response
        .split_once("\r\n\r\n")
        .map(|(_, payload)| payload)
        .expect("HTTP response has headers");
    let value = serde_json::from_str(payload)
        .unwrap_or_else(|error| panic!("invalid JSON from {method} {path}: {error}: {payload}"));
    (status, value)
}

fn wait_for_health(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if health_probe(port) {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("daemon did not become healthy on port {port}");
}

fn health_probe(port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(500),
    ) else {
        return false;
    };
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok();
    let request = format!(
        "GET /api/v1/health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: 0\r\n\r\n"
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = String::new();
    if stream.read_to_string(&mut response).is_err() {
        return false;
    }
    response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        == Some(200)
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
        .env("SYNEVID_LOOPBACK_TOKEN", TOKEN)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn native synevid-ui daemon");
    wait_for_health(port);
    child
}

#[test]
fn phase4_native_runtime_clients_share_authority_and_replay_after_restart() {
    let root = std::env::temp_dir().join(format!(
        "synevid-phase4-native-runtime-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.ts"),
        "export function runtimeAuthority(value: number) { return value + 1; }\n",
    )
    .unwrap();
    let port = reserve_port();

    let mut daemon = start_daemon(&root, port);
    let (unauthorized_status, unauthorized) = request(
        port,
        Some("wrong-token"),
        "bad:test",
        "test",
        None,
        "GET",
        "/api/v1/status",
        None,
    );
    assert_eq!(unauthorized_status, 401);
    assert_eq!(unauthorized["error_code"], "ERR-AUTH-401");

    let canonical_root = root
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let (attach_status, attached) = request(
        port,
        Some(TOKEN),
        "cli:test",
        "cli",
        None,
        "POST",
        "/api/v1/repos/connect",
        Some(
            &serde_json::json!({
                "root_path": canonical_root,
                "display_name": "phase4-runtime-test"
            })
            .to_string(),
        ),
    );
    assert_eq!(attach_status, 200);
    let repo_id = attached["repo_id"].as_str().expect("attached repo id");
    let (connect_status, connected) = request(
        port,
        Some(TOKEN),
        "cli:test",
        "cli",
        None,
        "POST",
        "/api/v1/runtime/connect",
        Some(
            &serde_json::json!({
                "client_id": "cli:test",
                "client_type": "cli",
                "repository_id": repo_id,
                "last_event_seq": 0
            })
            .to_string(),
        ),
    );
    assert_eq!(connect_status, 200);
    let lease_id = connected["lease_id"]
        .as_str()
        .expect("runtime lease")
        .to_string();
    let daemon_id = connected["runtime"]["daemon_id"]
        .as_str()
        .expect("daemon identity")
        .to_string();

    let (ui_status_code, ui_status) = request(
        port,
        Some(TOKEN),
        "ui:test",
        "ui",
        None,
        "GET",
        "/api/v1/status",
        None,
    );
    assert_eq!(ui_status_code, 200);
    assert_eq!(ui_status["daemon_id"], daemon_id);
    assert_eq!(ui_status["active_repo_id"], repo_id);
    assert_eq!(ui_status["runtime"]["repository_id"], repo_id);

    let (_heartbeat_status, heartbeat) = request(
        port,
        Some(TOKEN),
        "cli:test",
        "cli",
        Some(&lease_id),
        "POST",
        "/api/v1/runtime/heartbeat",
        Some(&serde_json::json!({ "last_event_seq": 0 }).to_string()),
    );
    assert_eq!(heartbeat["status"], "heartbeat");

    let before = ui_status["last_event_seq"].as_i64().unwrap_or(0);
    let (_event_status, event) = request(
        port,
        Some(TOKEN),
        "mcp:test",
        "mcp",
        None,
        "POST",
        "/api/v1/live/broadcast",
        Some(
            &serde_json::json!({
                "event": "phase4.native_mcp_action",
                "data": { "repository_id": repo_id, "evidence": "wire" }
            })
            .to_string(),
        ),
    );
    let event_seq = event["seq_id"].as_i64().expect("event sequence");
    assert!(event_seq > before);
    let (_replay_status, replay) = request(
        port,
        Some(TOKEN),
        "ui:test",
        "ui",
        None,
        "GET",
        &format!("/api/v1/live/poll?since_seq={before}"),
        None,
    );
    assert_eq!(replay["gap"], false);
    assert!(replay["events"].as_array().unwrap().iter().any(|item| {
        item["seq_id"] == event_seq && item["data"]["source_client_type"] == "mcp"
    }));

    daemon.kill().unwrap();
    let _ = daemon.wait();
    let mut restarted = start_daemon(&root, port);
    let (_restart_status, restart_state) = request(
        port,
        Some(TOKEN),
        "ui:test",
        "ui",
        None,
        "GET",
        "/api/v1/status",
        None,
    );
    assert_eq!(restart_state["daemon_id"], daemon_id);
    assert_eq!(restart_state["active_repo_id"], repo_id);
    assert!(restart_state["last_event_seq"].as_i64().unwrap() >= event_seq);
    let (stale_lease_status, stale_lease) = request(
        port,
        Some(TOKEN),
        "cli:test",
        "cli",
        Some(&lease_id),
        "POST",
        "/api/v1/runtime/heartbeat",
        Some(&serde_json::json!({ "last_event_seq": event_seq }).to_string()),
    );
    assert_eq!(stale_lease_status, 200);
    assert_eq!(stale_lease["error_code"], "ERR-RUNTIME-403");
    let (_restart_replay_status, restart_replay) = request(
        port,
        Some(TOKEN),
        "ui:test",
        "ui",
        None,
        "GET",
        &format!("/api/v1/live/poll?since_seq={before}"),
        None,
    );
    assert!(
        restart_replay["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| {
                item["seq_id"] == event_seq && item["data"]["evidence_digest"].is_string()
            })
    );
    restarted.kill().unwrap();
    let _ = restarted.wait();
    let _ = fs::remove_dir_all(root);
}
