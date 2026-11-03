//! Phase 4 real edit -> simulate -> verify -> working-memory integration.
//!
//! This test intentionally drives the daemon handlers against a temporary
//! repository. It does not invent AST payloads, caller counts, receipts, or
//! token savings: the first cache response comes from the persisted graph and
//! the second response is measured from that stored result.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use synevid_ui::server::DashboardServer;

fn fixture_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "synevid-phase4-real-cache-{}-{}",
        std::process::id(),
        blake3::hash(format!("{:?}", std::time::SystemTime::now()).as_bytes()).to_hex()
    ));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/PaymentService.ts"),
        "export function chargeCard(amount: number) { return amount; }\n",
    )
    .unwrap();
    fs::write(
        root.join("src/OrderController.ts"),
        "import { chargeCard } from './PaymentService';\nexport function checkout(amount: number) { return chargeCard(amount); }\n",
    )
    .unwrap();
    root
}

fn json_response(raw: &str) -> Value {
    serde_json::from_str(raw)
        .unwrap_or_else(|error| panic!("daemon returned invalid JSON: {error}; {raw}"))
}

#[test]
fn test_phase4_real_edit_simulate_verify_and_working_memory() {
    let root = fixture_root();
    let server = DashboardServer::new(root.clone(), "127.0.0.1".to_string(), 0);

    let session = json_response(
        &server.handle_forensic_sessions_start(
            &serde_json::json!({
                "session_id": "sess_phase4_real",
                "actor_kind": "AI_AGENT",
                "actor_name": "integration-test"
            })
            .to_string(),
        ),
    );
    assert_eq!(session["status"], "ACTIVE");

    let original = fs::read_to_string(root.join("src/PaymentService.ts")).unwrap();
    // Warm the real repository/index artifact path before measuring the
    // steady-state edit guardrail. Cold startup work is not the edit budget.
    let _warmup = json_response(
        &server.handle_simulate_edit(
            &serde_json::json!({
                "file_path": "src/PaymentService.ts",
                "unsaved_buffer": original.clone(),
                "session_id": "sess_phase4_real"
            })
            .to_string(),
        ),
    );
    let simulation = json_response(&server.handle_simulate_edit(
        &serde_json::json!({
            "file_path": "src/PaymentService.ts",
            "unsaved_buffer": "export function chargeCard(amount: number, token: string) { return amount; }\n",
            "session_id": "sess_phase4_real"
        })
        .to_string(),
    ));
    assert!(simulation["verdict"].is_string());
    assert!(simulation["candidate_snapshot_id"].is_string());
    assert!(
        simulation["computation_latency_ms"]
            .as_u64()
            .is_some_and(|latency| latency <= 50),
        "real in-memory simulation exceeded the 50 ms Phase 4 budget: {}",
        simulation["computation_latency_ms"]
    );
    assert_eq!(
        fs::read_to_string(root.join("src/PaymentService.ts")).unwrap(),
        original
    );

    // The agent now applies the edit outside the daemon, then the daemon verifies
    // the real working tree. This proves the benchmark exercises a real loop.
    let edited = "export function chargeCard(amount: number, token: string) { return amount; }\n";
    fs::write(root.join("src/PaymentService.ts"), edited).unwrap();
    let verification = json_response(&server.handle_verify());
    assert!(verification["status"].is_string());
    assert!(
        !verification.get("error").is_some(),
        "verification must return evidence"
    );

    let status = json_response(&server.handle_status());
    let snapshot_id = status["snapshot_id"].as_str().expect("real snapshot id");
    let storage = server
        .get_storage_for_active_repo()
        .expect("repository storage");
    let (nodes, _) = storage
        .graph_rows_for_snapshot(snapshot_id)
        .expect("indexed graph rows");
    let target = nodes
        .iter()
        .find(|node| node.node_kind != "file" && node.node_kind != "unknown")
        .expect("fixture must produce a semantic graph node");

    let query = serde_json::json!({
        "session_id": "sess_phase4_real",
        "symbol": target.qualified_name.clone(),
        "query_kind": "full_subgraph",
        "cache_mode": "read"
    })
    .to_string();
    let first = json_response(&server.handle_ai_cache_query(&query));
    assert_eq!(first["status"], "CACHED_NEW");
    assert_eq!(first["compact_pointer"], false);
    assert!(first["query"]["target_node"].is_object());
    assert!(first["tokens_spent"].as_i64().unwrap_or(0) > 0);

    let second = json_response(&server.handle_ai_cache_query(&query));
    assert_eq!(second["status"], "UNCHANGED_SINCE_PREVIOUS_STEP");
    assert_eq!(second["compact_pointer"], true);
    assert!(second["tokens_saved"].as_i64().unwrap_or(0) > 0);
    assert_eq!(first["receipt_token"], second["receipt_token"]);

    let ledger =
        json_response(&server.handle_ai_ledger("/api/v1/ai/ledger?session_id=sess_phase4_real"));
    assert_eq!(ledger["ledger"]["total_queries"], 2);
    assert_eq!(ledger["ledger"]["cache_hits"], 1);
    assert!(ledger["ledger"]["tokens_saved"].as_i64().unwrap_or(0) > 0);

    let timeline = json_response(
        &server.handle_forensic_timeline("/api/v1/forensics/timeline?session_id=sess_phase4_real"),
    );
    assert!(timeline["events"].as_array().is_some_and(|events| {
        events
            .iter()
            .any(|event| event["event_type"] == "simulate_edit")
    }));

    // Acceptance benchmark: six real edit -> verify -> graph-query turns. The
    // uncached arm invalidates the persisted entry before every query; the cached
    // arm keeps the same snapshot/symbol key and must return one full receipt plus
    // compact pointers. Both arms execute the same simulator and verifier, so the
    // reduction is measured on real semantic responses rather than a handcrafted
    // token constant.
    let benchmark_query = |session_id: &str| {
        serde_json::json!({
            "session_id": session_id,
            "symbol": target.qualified_name.clone(),
            "query_kind": "full_subgraph",
            "cache_mode": "read"
        })
        .to_string()
    };
    let mut uncached_bytes = 0usize;
    let mut uncached_semantics: Option<Value> = None;
    for turn in 0..6 {
        let simulated = json_response(&server.handle_simulate_edit(
            &serde_json::json!({
                "file_path": "src/PaymentService.ts",
                "unsaved_buffer": format!(
                    "export function chargeCard(amount: number, token: string) {{ return amount + {}; }}\n",
                    turn
                ),
                "session_id": "sess_phase4_uncached"
            })
            .to_string(),
        ));
        assert!(simulated["verdict"].is_string());
        let verified = json_response(&server.handle_verify());
        assert!(verified["status"].is_string());
        let query = benchmark_query("sess_phase4_uncached");
        let _ = json_response(
            &server.handle_ai_cache_query(
                &serde_json::json!({
                    "session_id": "sess_phase4_uncached",
                    "symbol": target.qualified_name.clone(),
                    "query_kind": "full_subgraph",
                    "cache_mode": "invalidate"
                })
                .to_string(),
            ),
        );
        let response = json_response(&server.handle_ai_cache_query(&query));
        assert_eq!(response["status"], "CACHED_NEW");
        if uncached_semantics.is_none() {
            uncached_semantics = Some(response["query"].clone());
        }
        uncached_bytes += response.to_string().len();
    }

    let mut cached_bytes = 0usize;
    let mut cached_semantics: Option<Value> = None;
    for turn in 0..6 {
        let simulated = json_response(&server.handle_simulate_edit(
            &serde_json::json!({
                "file_path": "src/PaymentService.ts",
                "unsaved_buffer": format!(
                    "export function chargeCard(amount: number, token: string) {{ return amount + {}; }}\n",
                    turn
                ),
                "session_id": "sess_phase4_cached"
            })
            .to_string(),
        ));
        assert!(simulated["verdict"].is_string());
        let verified = json_response(&server.handle_verify());
        assert!(verified["status"].is_string());
        let response =
            json_response(&server.handle_ai_cache_query(&benchmark_query("sess_phase4_cached")));
        if turn == 0 {
            assert_eq!(response["status"], "CACHED_NEW");
            cached_semantics = Some(response["query"].clone());
        } else {
            assert_eq!(response["status"], "UNCHANGED_SINCE_PREVIOUS_STEP");
            assert_eq!(response["compact_pointer"], true);
        }
        cached_bytes += response.to_string().len();
    }
    assert_eq!(
        uncached_semantics, cached_semantics,
        "cache must preserve semantic query results"
    );
    let reduction =
        (uncached_bytes.saturating_sub(cached_bytes) as f64 / uncached_bytes as f64) * 100.0;
    assert!(
        reduction >= 60.0,
        "six-turn working-memory reduction was only {reduction:.2}% (uncached={uncached_bytes}, cached={cached_bytes})"
    );

    fs::remove_dir_all(root).ok();
}
