//! Phase 1 HTTP contract tests.
//!
//! This deliberately drives the daemon through its wire surface instead of
//! calling `handle_simulate_edit` directly.  Each randomized edit is compared
//! with an independent cold scan, and the same test exercises the path/query
//! ego compatibility route plus the bidirectional `/api/v1/live` SSE/POST
//! surface.

use autopsy_cli::{
    build_snapshot_and_graph_with_overlays_and_registry, build_snapshot_and_graph_with_registry,
    builtin_plugin_registry, load_or_default_config,
};
use autopsy_storage::{StorageEngine, StorageOptions};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use synevid_ui::server::DashboardServer;
use tiny_http::Server;

fn http_request(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> String {
    let mut stream = TcpStream::connect(addr).expect("HTTP test server must accept connections");
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .expect("HTTP request must be writable");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .expect("HTTP response must be readable");
    let marker = b"\r\n\r\n";
    let body_start = response
        .windows(marker.len())
        .position(|window| window == marker)
        .map(|position| position + marker.len())
        .expect("HTTP response must have headers");
    let raw_body = &response[body_start..];
    let body = if let Some(first_line_end) =
        raw_body.windows(2).position(|window| window == b"\r\n")
        && usize::from_str_radix(
            std::str::from_utf8(&raw_body[..first_line_end]).unwrap_or_default(),
            16,
        )
        .is_ok()
    {
        // tiny_http uses chunked transfer for bodies without a fixed length.
        let mut decoded = Vec::new();
        let mut cursor = 0usize;
        loop {
            let line_end = raw_body[cursor..]
                .windows(2)
                .position(|window| window == b"\r\n")
                .map(|offset| cursor + offset)
                .expect("chunked body must have a size line");
            let size = usize::from_str_radix(
                std::str::from_utf8(&raw_body[cursor..line_end]).unwrap(),
                16,
            )
            .expect("chunk size must be hexadecimal");
            cursor = line_end + 2;
            if size == 0 {
                break;
            }
            decoded.extend_from_slice(&raw_body[cursor..cursor + size]);
            cursor += size + 2;
        }
        decoded
    } else {
        raw_body.to_vec()
    };
    String::from_utf8(body).expect("HTTP response must be UTF-8")
}

fn json_request(addr: SocketAddr, method: &str, path: &str, body: Option<&str>) -> Value {
    let response = http_request(addr, method, path, body);
    serde_json::from_str(&response).unwrap_or_else(|error| {
        panic!("expected JSON response for {method} {path}: {error}; body={response:?}")
    })
}

fn websocket_request(addr: SocketAddr) -> Value {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .write_all(
            b"GET /api/v1/live?since=999999 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
        )
        .unwrap();
    let mut handshake = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).unwrap();
        handshake.push(byte[0]);
        if handshake.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let handshake = String::from_utf8(handshake).unwrap();
    assert!(handshake.starts_with("HTTP/1.1 101"));
    assert!(handshake.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="));

    let command = serde_json::json!({
        "event": "phase1.websocket_contract",
        "data": { "direction": "client-to-server" }
    })
    .to_string();
    let payload = command.as_bytes();
    assert!(payload.len() < 126);
    let mask = [7_u8, 19, 41, 83];
    let mut frame = vec![0x81_u8, 0x80 | payload.len() as u8];
    frame.extend_from_slice(&mask);
    frame.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index % 4]),
    );
    stream.write_all(&frame).unwrap();

    let mut response_header = [0_u8; 2];
    stream.read_exact(&mut response_header).unwrap();
    assert_eq!(response_header[0] & 0x0F, 0x1);
    assert_eq!(response_header[1] & 0x80, 0);
    let response_len = match response_header[1] & 0x7F {
        length @ 0..=125 => length as usize,
        126 => {
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes).unwrap();
            u16::from_be_bytes(bytes) as usize
        }
        _ => panic!("test response is unexpectedly large"),
    };
    let mut response_payload = vec![0_u8; response_len];
    stream.read_exact(&mut response_payload).unwrap();
    let response: Value = serde_json::from_slice(&response_payload).unwrap();
    assert_eq!(response["event"], "phase1.websocket_contract");
    assert!(response["seq_id"].as_i64().is_some());

    let close = [0x88_u8, 0x80, 1, 2, 3, 4, 0, 0];
    stream.write_all(&close).unwrap();
    response
}

fn encode_path_component(value: &str) -> String {
    value
        .bytes()
        .flat_map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                vec![byte as char]
            } else {
                format!("%{byte:02X}").chars().collect()
            }
        })
        .collect()
}

#[test]
fn phase1_http_randomized_edits_match_cold_scan_and_live_contracts() {
    let root = std::env::temp_dir().join(format!("synevid_phase1_http_e2e_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("src")).unwrap();
    let baseline_source = "export function target(value: number) { return value + 1; }\nexport function helper() { return 2; }\nexport function caller() { return target(1); }\n";
    fs::write(root.join("src/service.ts"), baseline_source).unwrap();
    fs::write(
        root.join("src/caller.ts"),
        "import { target } from './service';\nexport function externalCaller() { return target(1); }\n",
    )
    .unwrap();

    let config = load_or_default_config(&root).unwrap();
    let registry = builtin_plugin_registry(&root);
    let storage = StorageEngine::open(
        root.join(".autopsy/storage.db"),
        StorageOptions {
            use_wal: false,
            cache_dir: Some(root.join(".autopsy/cache")),
        },
    )
    .unwrap();
    let baseline =
        build_snapshot_and_graph_with_registry(&root, &config, None, Some(&storage), &registry)
            .unwrap();
    let baseline_id = baseline.snapshot.snapshot_id.to_string();

    let dashboard = DashboardServer::new(root.clone(), "127.0.0.1".into(), 0);
    let listener = Server::http(("127.0.0.1", 0)).unwrap();
    let addr = listener.server_addr().to_ip().unwrap();
    let server_thread = thread::spawn(move || {
        for request in listener.incoming_requests().take(106) {
            dashboard.dispatch(request);
        }
    });

    // One hundred deterministic-but-randomized edits, including reverts, must
    // be observed through the actual POST endpoint.
    let mut random_state = 0x9E37_79B9_u64;
    for iteration in 0..100 {
        random_state = random_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let is_revert = random_state.is_multiple_of(5);
        let value = (random_state % 17) + 2;
        let destination = if random_state & 1 == 0 {
            "target"
        } else {
            "helper"
        };
        let edited_source = if is_revert {
            baseline_source.to_string()
        } else {
            format!(
                "export function target(value: number) {{ return value + {value}; }}\nexport function helper() {{ return 2; }}\nexport function caller() {{ return {destination}(1); }}\n"
            )
        };
        let request = serde_json::json!({
            "file_path": "src/service.ts",
            "unsaved_buffer": edited_source,
            "baseline_snapshot_id": &baseline_id,
        });
        let response = json_request(
            addr,
            "POST",
            "/api/v1/simulate/edit",
            Some(&request.to_string()),
        );
        assert!(
            response.get("error").is_none(),
            "HTTP edit failed: {response}"
        );
        let candidate_id = response["candidate_snapshot_id"]
            .as_str()
            .unwrap_or_else(|| panic!("missing candidate snapshot at iteration {iteration}"));

        let overlays = BTreeMap::from([("src/service.ts".to_string(), edited_source.into_bytes())]);
        let cold = build_snapshot_and_graph_with_overlays_and_registry(
            &root, &config, None, None, &overlays, None, &registry,
        )
        .unwrap();
        assert_eq!(candidate_id, cold.snapshot.snapshot_id.to_string());
    }

    let encoded_id = encode_path_component("src/service.ts");
    let path_ego = json_request(
        addr,
        "GET",
        &format!("/api/v1/graph/ego/{encoded_id}"),
        None,
    );
    let query_ego = json_request(
        addr,
        "GET",
        &format!("/api/v1/graph/ego?id={encoded_id}"),
        None,
    );
    assert_eq!(path_ego, query_ego);

    let live_body = serde_json::json!({
        "event": "phase1.http_contract",
        "data": { "iteration": 100 }
    });
    let live_response = json_request(addr, "POST", "/api/v1/live", Some(&live_body.to_string()));
    assert_eq!(live_response["success"], true);
    assert!(live_response["seq_id"].as_i64().is_some());
    let sse = http_request(addr, "GET", "/api/v1/live?since=0", None);
    assert!(sse.contains("event: phase1.http_contract"));
    let replay = json_request(addr, "GET", "/api/v1/live/poll?since_seq=0", None);
    assert!(replay["events"].as_array().is_some_and(|events| {
        events
            .iter()
            .any(|event| event["event"] == "phase1.http_contract")
    }));
    let websocket = websocket_request(addr);
    assert_eq!(websocket["event"], "phase1.websocket_contract");

    server_thread
        .join()
        .expect("HTTP test server must shut down");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn phase1_http_corruption_recovery_reports_err_storage_102() {
    let root = std::env::temp_dir().join(format!(
        "synevid_phase1_http_corruption_e2e_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join(".autopsy")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/main.ts"),
        "export function main() { return 1; }\n",
    )
    .unwrap();
    fs::write(root.join(".autopsy/storage.db"), b"not a sqlite database").unwrap();

    let dashboard = DashboardServer::new(root.clone(), "127.0.0.1".into(), 0);
    let listener = Server::http(("127.0.0.1", 0)).unwrap();
    let addr = listener.server_addr().to_ip().unwrap();
    let server_thread = thread::spawn(move || {
        let request = listener
            .incoming_requests()
            .next()
            .expect("health request must arrive");
        dashboard.dispatch(request);
    });

    let health = json_request(addr, "GET", "/health", None);
    assert_eq!(health["error_code"], "ERR-STORAGE-102");
    assert_eq!(health["storage_recovery"]["error_code"], "ERR-STORAGE-102");
    let backup_path = health["storage_recovery"]["backup_path"]
        .as_str()
        .expect("recovery response must include the backup path");
    assert!(backup_path.ends_with(".bak"));
    assert!(!backup_path.contains('\\'));
    assert!(root.join(".autopsy/storage.db.bak").is_file());

    server_thread
        .join()
        .expect("HTTP corruption test server must shut down");
    let _ = fs::remove_dir_all(root);
}
