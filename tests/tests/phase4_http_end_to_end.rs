//! Phase 4 wire-level daemon closure.
//!
//! Exercises the persisted session/event/cache contracts over HTTP, then
//! restarts the daemon and verifies that the live sequence cursor replays the
//! same events. The test does not touch the canvas renderer.

use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant};
use synevid_ui::server::DashboardServer;
use tiny_http::Server;

fn http_request(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> String {
    let body = body.unwrap_or("");
    let mut stream = TcpStream::connect(addr).unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).unwrap();
    let marker = b"\r\n\r\n";
    let body_start = response
        .windows(marker.len())
        .position(|window| window == marker)
        .unwrap()
        + marker.len();
    let raw = &response[body_start..];
    let decoded = if let Some(first_line_end) = raw.windows(2).position(|window| window == b"\r\n")
        && usize::from_str_radix(
            std::str::from_utf8(&raw[..first_line_end]).unwrap_or_default(),
            16,
        )
        .is_ok()
    {
        let mut result = Vec::new();
        let mut cursor = 0;
        loop {
            let line_end = raw[cursor..]
                .windows(2)
                .position(|window| window == b"\r\n")
                .map(|offset| cursor + offset)
                .unwrap();
            let size =
                usize::from_str_radix(std::str::from_utf8(&raw[cursor..line_end]).unwrap(), 16)
                    .unwrap();
            cursor = line_end + 2;
            if size == 0 {
                break;
            }
            result.extend_from_slice(&raw[cursor..cursor + size]);
            cursor += size + 2;
        }
        result
    } else {
        raw.to_vec()
    };
    String::from_utf8(decoded).unwrap()
}

fn json_request(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> Value {
    let response = http_request(addr, method, path, body);
    serde_json::from_str(&response)
        .unwrap_or_else(|error| panic!("invalid JSON from {method} {path}: {error}; {response}"))
}

fn write_masked_text_frame(stream: &mut TcpStream, value: &Value) {
    let payload = value.to_string();
    assert!(payload.len() < 126);
    let mask = [19_u8, 41, 83, 107];
    let mut frame = vec![0x81_u8, 0x80 | payload.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .as_bytes()
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    stream.write_all(&frame).unwrap();
}

fn read_websocket_json(stream: &mut TcpStream) -> Value {
    let mut header = [0_u8; 2];
    stream.read_exact(&mut header).unwrap();
    assert_eq!(header[0] & 0x0F, 0x1);
    assert_eq!(header[1] & 0x80, 0);
    let payload_len = match header[1] & 0x7F {
        length @ 0..=125 => length as usize,
        126 => {
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes).unwrap();
            u16::from_be_bytes(bytes) as usize
        }
        _ => panic!("test WebSocket event is unexpectedly large"),
    };
    let mut payload = vec![0_u8; payload_len];
    stream.read_exact(&mut payload).unwrap();
    serde_json::from_slice(&payload).unwrap()
}

#[test]
fn phase4_connected_websocket_receives_external_daemon_event() {
    let root = std::env::temp_dir().join(format!(
        "synevid_phase4_websocket_e2e_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.ts"),
        "export function main() { return 1; }\n",
    )
    .unwrap();

    let dashboard = DashboardServer::new(root.clone(), "127.0.0.1".into(), 0);
    let mut cursor = dashboard
        .get_storage_for_active_repo()
        .unwrap()
        .latest_live_event_seq()
        .unwrap();
    let listener = Server::http(("127.0.0.1", 0)).unwrap();
    let addr = listener.server_addr().to_ip().unwrap();
    let dispatch_dashboard = dashboard.clone();
    const LIVE_SAMPLES: usize = 8;
    let dispatcher = thread::spawn(move || {
        let mut workers = Vec::new();
        for request in listener.incoming_requests().take(LIVE_SAMPLES + 1) {
            let worker_dashboard = dispatch_dashboard.clone();
            workers.push(thread::spawn(move || worker_dashboard.dispatch(request)));
        }
        for worker in workers {
            worker.join().unwrap();
        }
    });

    let mut websocket = TcpStream::connect(addr).unwrap();
    websocket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    websocket
        .write_all(
            format!(
                "GET /api/v1/live?since={cursor} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut handshake = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        websocket.read_exact(&mut byte).unwrap();
        handshake.push(byte[0]);
        if handshake.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    assert!(
        String::from_utf8(handshake)
            .unwrap()
            .starts_with("HTTP/1.1 101")
    );

    let mut delivery_latencies = Vec::with_capacity(LIVE_SAMPLES);
    let mut ai_activity_latency = None;
    for index in 0..LIVE_SAMPLES {
        let delivery_started = Instant::now();
        let event_name = if index == 0 {
            "mcp.live_activity"
        } else {
            "phase4.external_mcp_activity"
        };
        let external = json_request(
            addr,
            "POST",
            "/api/v1/live/broadcast",
            Some(
                &serde_json::json!({
                    "event": event_name,
                    "data": { "source": "mcp", "target": "src/main.ts", "status": "active", "sample": index }
                })
                .to_string(),
            ),
        );
        assert_eq!(external["success"], true);

        write_masked_text_frame(
            &mut websocket,
            &serde_json::json!({
                "event": "live.poll",
                "data": { "since_seq": cursor }
            }),
        );
        let received = read_websocket_json(&mut websocket);
        let delivery_latency = delivery_started.elapsed();
        assert_eq!(received["event"], event_name);
        assert_eq!(received["data"]["source"], "mcp");
        let received_seq = received["seq_id"].as_i64().expect("event sequence");
        assert!(received_seq > cursor);
        cursor = received_seq;
        if index == 0 {
            assert_eq!(received["data"]["status"], "active");
            ai_activity_latency = Some(delivery_latency);
        }
        delivery_latencies.push(delivery_latency);
    }
    delivery_latencies.sort_unstable();
    let p95_index = ((delivery_latencies.len() * 95).div_ceil(100)).saturating_sub(1);
    let delivery_p95 = delivery_latencies[p95_index];
    eprintln!("Phase 4 live WebSocket delivery p95: {delivery_p95:.2?}");
    assert!(
        delivery_p95 <= Duration::from_millis(20),
        "Phase 4 live timeline delivery p95 exceeded 20 ms: {delivery_p95:?}; samples={delivery_latencies:?}"
    );
    let ai_activity_latency = ai_activity_latency.expect("AI activity sample");
    assert!(
        ai_activity_latency <= Duration::from_millis(25),
        "Phase 4 AI Active delivery exceeded 25 ms: {ai_activity_latency:?}"
    );

    websocket
        .write_all(&[0x88_u8, 0x80, 1, 2, 3, 4, 0, 0])
        .unwrap();
    dispatcher.join().unwrap();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn phase4_daemon_http_session_cache_event_and_restart_replay() {
    let root = std::env::temp_dir().join(format!("synevid_phase4_http_e2e_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/service.ts"),
        "export function target(value: number) { return value + 1; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/caller.ts"),
        "import { target } from './service';\nexport function caller() { return target(1); }\n",
    )
    .unwrap();

    let dashboard = DashboardServer::new(root.clone(), "127.0.0.1".into(), 0);
    let active_id = dashboard.active_repo_id.lock().unwrap().clone();
    let repo = dashboard
        .connected_repos
        .lock()
        .unwrap()
        .get(&active_id)
        .unwrap()
        .clone();
    let storage = dashboard.get_storage_for_active_repo().unwrap();
    let (nodes, _) = storage
        .graph_rows_for_snapshot(repo.latest_snap_id.as_deref().unwrap())
        .unwrap();
    let target = nodes
        .iter()
        .find(|node| node.node_kind == "function")
        .unwrap()
        .qualified_name
        .clone();

    let listener = Server::http(("127.0.0.1", 0)).unwrap();
    let addr = listener.server_addr().to_ip().unwrap();
    let thread_dashboard = dashboard.clone();
    let server_thread = thread::spawn(move || {
        for request in listener.incoming_requests().take(6) {
            thread_dashboard.dispatch(request);
        }
    });

    let start = json_request(
        addr,
        "POST",
        "/api/v1/forensics/sessions/start",
        Some(
            &serde_json::json!({
                "session_id": "sess_phase4_http",
                "actor_kind": "AI_AGENT",
                "actor_name": "http-test"
            })
            .to_string(),
        ),
    );
    assert_eq!(start["status"], "ACTIVE");

    let simulated = json_request(
        addr,
        "POST",
        "/api/v1/simulate/edit",
        Some(
            &serde_json::json!({
                "repo_root": root.to_string_lossy(),
                "file_path": "src/service.ts",
                "unsaved_buffer": "export function target(value: number) { return value + 2; }\n",
                "session_id": "sess_phase4_http"
            })
            .to_string(),
        ),
    );
    assert!(simulated["verdict"].is_string());
    assert!(
        simulated["computation_latency_ms"]
            .as_u64()
            .is_some_and(|latency| latency <= 50),
        "HTTP simulation exceeded the 50 ms Phase 4 budget: {}",
        simulated["computation_latency_ms"]
    );

    let recorded = json_request(
        addr,
        "POST",
        "/api/v1/forensics/event",
        Some(
            &serde_json::json!({
                "repo_root": root.to_string_lossy(),
                "session_id": "sess_phase4_http",
                "event_type": "canvas_focus",
                "summary": "HTTP canvas focus",
                "target_symbol": target.clone(),
                "evidence": { "source": "http-test" }
            })
            .to_string(),
        ),
    );
    assert_eq!(recorded["status"], "RECORDED");

    let query = serde_json::json!({
        "repo_root": root.to_string_lossy(),
        "session_id": "sess_phase4_http",
        "symbol": target.clone(),
        "query_kind": "callers"
    })
    .to_string();
    let first_cache = json_request(addr, "POST", "/api/v1/ai/cache/query", Some(&query));
    assert_eq!(first_cache["status"], "CACHED_NEW");
    assert!(first_cache["query"]["target_node"].is_object());
    let second_cache = json_request(addr, "POST", "/api/v1/ai/cache/query", Some(&query));
    assert_eq!(second_cache["status"], "UNCHANGED_SINCE_PREVIOUS_STEP");
    assert!(second_cache["tokens_saved"].as_i64().unwrap_or(0) > 0);

    let live = json_request(addr, "GET", "/api/v1/live/poll?since_seq=0", None);
    assert!(live["events"].as_array().is_some_and(|events| {
        events
            .iter()
            .any(|event| event["event"] == "trajectory.step_recorded")
    }));
    server_thread.join().unwrap();

    // Reopen the same repository/SQLite journal and replay the persisted live
    // sequence through the restarted daemon's HTTP endpoint.
    let restarted = DashboardServer::new(root.clone(), "127.0.0.1".into(), 0);
    let listener = Server::http(("127.0.0.1", 0)).unwrap();
    let addr = listener.server_addr().to_ip().unwrap();
    let thread_dashboard = restarted.clone();
    let replay_thread = thread::spawn(move || {
        for request in listener.incoming_requests().take(1) {
            thread_dashboard.dispatch(request);
        }
    });
    let replay = json_request(addr, "GET", "/api/v1/live/poll?since_seq=0", None);
    assert!(replay["events"].as_array().is_some_and(|events| {
        events
            .iter()
            .any(|event| event["event"] == "trajectory.step_recorded")
    }));
    replay_thread.join().unwrap();
    fs::remove_dir_all(root).ok();
}
