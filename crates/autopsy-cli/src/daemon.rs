//! Authenticated local-daemon client commands.
//!
//! `autopsy attach` and `autopsy status` are presentation-layer commands. They
//! never run source code or Git commands and never maintain a second repository
//! registry: the daemon remains the authority for the active repository,
//! snapshot, event cursor, and storage health.

use crate::{Cli, CliError, CliImpactDirection, Commands, EXIT_PASS, OutputFormat};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_DAEMON_HOST: &str = "127.0.0.1";
const DEFAULT_DAEMON_PORT: u16 = 3333;
const STATUS_TIMEOUT: Duration = Duration::from_millis(1500);
const ATTACH_TIMEOUT: Duration = Duration::from_secs(120);

fn daemon_client_id() -> String {
    daemon_client_id_for("cli")
}

fn daemon_client_id_for(client_type: &str) -> String {
    std::env::var("SYNEVID_CLIENT_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{client_type}:default"))
}

fn daemon_address() -> Result<String, CliError> {
    let host = std::env::var("SYNEVID_DAEMON_HOST").unwrap_or_else(|_| DEFAULT_DAEMON_HOST.into());
    let port = std::env::var("SYNEVID_DAEMON_PORT")
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()
        .map_err(|error| CliError::Config(format!("invalid SYNEVID_DAEMON_PORT: {error}")))?
        .unwrap_or(DEFAULT_DAEMON_PORT);
    Ok(format!("{host}:{port}"))
}

fn daemon_request(
    method: &str,
    route: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<Value, CliError> {
    daemon_request_as(method, route, body, timeout, "cli")
}

fn daemon_request_as(
    method: &str,
    route: &str,
    body: Option<&str>,
    timeout: Duration,
    client_type: &str,
) -> Result<Value, CliError> {
    let address = daemon_address()?;
    let mut addresses = address.to_socket_addrs().map_err(|error| {
        CliError::Analysis(format!("daemon address resolution failed: {error}"))
    })?;
    let socket = addresses.next().ok_or_else(|| {
        CliError::Analysis("daemon address did not resolve to a local endpoint".to_string())
    })?;
    let mut stream = TcpStream::connect_timeout(&socket, timeout).map_err(|error| {
        CliError::Analysis(format!(
            "daemon unavailable at {address}; start synevid-ui and retry: {error}"
        ))
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .and_then(|_| stream.set_write_timeout(Some(timeout)))
        .map_err(CliError::Io)?;

    let body = body.unwrap_or("");
    let token = std::env::var("SYNEVID_LOOPBACK_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty());
    let authorization = token
        .as_deref()
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    let client_id = daemon_client_id_for(client_type);
    let lease_id = std::env::var("SYNEVID_LEASE_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default();
    let request = format!(
        "{method} {route} HTTP/1.1\r\nHost: {DEFAULT_DAEMON_HOST}\r\nConnection: close\r\nContent-Type: application/json\r\nX-Synevid-Client-Id: {client_id}\r\nX-Synevid-Client-Type: {client_type}\r\n{lease_header}Content-Length: {}\r\n{authorization}\r\n{body}",
        body.len(),
        lease_header = if lease_id.is_empty() {
            String::new()
        } else {
            format!("X-Synevid-Lease-Id: {lease_id}\r\n")
        }
    );
    stream.write_all(request.as_bytes()).map_err(CliError::Io)?;
    stream.flush().map_err(CliError::Io)?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).map_err(CliError::Io)?;
    let response = String::from_utf8(response)
        .map_err(|error| CliError::Analysis(format!("daemon response was not UTF-8: {error}")))?;
    let (headers, payload) = response.split_once("\r\n\r\n").ok_or_else(|| {
        CliError::Analysis("daemon returned an incomplete HTTP response".to_string())
    })?;
    let status_line = headers.lines().next().unwrap_or_default();
    let status_code = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0);
    let value: Value = serde_json::from_str(payload).map_err(|error| {
        CliError::Analysis(format!(
            "daemon returned invalid JSON ({status_code}): {error}"
        ))
    })?;
    if !(200..300).contains(&status_code) {
        return Err(CliError::Analysis(format!(
            "daemon rejected {method} {route} with HTTP {status_code}: {}",
            value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("no error detail")
        )));
    }
    Ok(value)
}

/// Read the daemon-owned active repository and cursor. Presentation clients
/// use this instead of opening their own registry so every surface sees the
/// same canonical state.
pub fn fetch_status() -> Result<Value, CliError> {
    daemon_request("GET", "/api/v1/status", None, STATUS_TIMEOUT)
}

pub fn fetch_status_for_client(client_type: &str) -> Result<Value, CliError> {
    daemon_request_as("GET", "/api/v1/status", None, STATUS_TIMEOUT, client_type)
}

/// Replay daemon events strictly after a client cursor. The daemon remains the
/// only source of truth; an unavailable daemon is reported as an error rather
/// than converted into an empty successful stream.
pub fn fetch_live_events(since_seq: i64) -> Result<Value, CliError> {
    daemon_request(
        "GET",
        &format!("/api/v1/live/poll?since_seq={since_seq}"),
        None,
        STATUS_TIMEOUT,
    )
}

pub fn fetch_live_events_for_client(since_seq: i64, client_type: &str) -> Result<Value, CliError> {
    daemon_request_as(
        "GET",
        &format!("/api/v1/live/poll?since_seq={since_seq}"),
        None,
        STATUS_TIMEOUT,
        client_type,
    )
}

pub fn fetch_analytics_report_for_client(
    client_type: &str,
    compact: bool,
    evidence: bool,
) -> Result<Value, CliError> {
    let mut query = "/api/v1/analytics/report?format=json".to_string();
    if compact {
        query.push_str("&compact=true");
    }
    if evidence {
        query.push_str("&evidence=true");
    }
    daemon_request_as("GET", &query, None, ATTACH_TIMEOUT, client_type)
}

pub fn handle_analytics_report(
    cli: &Cli,
    format: &str,
    compact: bool,
    evidence: bool,
    state: Option<&str>,
    severity: Option<&str>,
) -> Result<i32, CliError> {
    if !matches!(format, "json" | "markdown" | "html" | "sarif" | "bundle") {
        return Err(CliError::Config(
            "analytics format must be json, markdown, html, sarif, or bundle".into(),
        ));
    }
    let mut query = format!("/api/v1/analytics/report?format={format}");
    if compact {
        query.push_str("&compact=true");
    }
    if evidence {
        query.push_str("&evidence=true");
    }
    if let Some(s) = state {
        query.push_str(&format!("&state={}", encode_query_component(s)));
    }
    if let Some(s) = severity {
        query.push_str(&format!("&severity={}", encode_query_component(s)));
    }
    let value = daemon_request("GET", &query, None, ATTACH_TIMEOUT)?;
    match cli.format {
        OutputFormat::Json | OutputFormat::Sarif => println!(
            "{}",
            serde_json::to_string_pretty(&value)
                .map_err(|error| CliError::Analysis(error.to_string()))?
        ),
        OutputFormat::Text => {
            let report = value.get("report").unwrap_or(&value);
            println!(
                "report_id: {}",
                report
                    .get("report_id")
                    .and_then(Value::as_str)
                    .unwrap_or("UNKNOWN")
            );
            println!(
                "repository_id: {}",
                report["scope"]["repository_id"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
            );
            println!(
                "snapshot_id: {}",
                report["scope"]["snapshot_id"].as_str().unwrap_or("UNKNOWN")
            );
            println!(
                "generated_at: {}",
                report["generated_at"].as_u64().unwrap_or(0)
            );

            // Print evidence-backed findings
            if evidence {
                println!("\n=== EVIDENCE-BACKED FINDINGS ===");
                print_evidence_findings(report, "static_unreferenced", "verified_candidates");
                print_evidence_findings(report, "static_unreferenced", "unknown_candidates");
                print_evidence_findings(report, "runtime_unobserved", "unobserved_candidates");
            } else {
                println!(
                    "static_verified_candidates: {}",
                    report["static_unreferenced"]["verified_candidates"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0)
                );
                println!(
                    "static_unknown_candidates: {}",
                    report["static_unreferenced"]["unknown_candidates"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0)
                );
                println!(
                    "static_excluded_count: {}",
                    report["static_unreferenced"]["excluded_count"]
                        .as_u64()
                        .unwrap_or(0)
                );
                println!(
                    "runtime_unobserved_status: {}",
                    report["runtime_unobserved"]["status"]
                        .as_str()
                        .unwrap_or("UNKNOWN")
                );
                println!(
                    "runtime_windows_analyzed: {}",
                    report["runtime_unobserved"]["windows_analyzed"]
                        .as_u64()
                        .unwrap_or(0)
                );
                println!(
                    "runtime_unobserved_candidates: {}",
                    report["runtime_unobserved"]["unobserved_candidates"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0)
                );
                println!(
                    "semantic_git_status: {}",
                    report["git_evolution"]["semantic_git_risk"]["status"]
                        .as_str()
                        .unwrap_or("UNKNOWN")
                );
                println!(
                    "semantic_git_risk_score: {:.2}",
                    report["git_evolution"]["semantic_git_risk"]["risk_score"]
                        .as_f64()
                        .unwrap_or(0.0)
                );
                println!(
                    "git_velocity_commits: {}",
                    report["git_evolution"]["velocity"]["commits_analyzed"]
                        .as_u64()
                        .unwrap_or(0)
                );
                println!(
                    "change_concentration_hot_files: {}",
                    report["git_evolution"]["change_concentration"]["hot_files"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0)
                );
            }

            println!(
                "coverage_state: {:?}",
                report["coverage_state"].as_str().unwrap_or("unknown")
            );
        }
    }
    Ok(EXIT_PASS)
}

fn print_evidence_findings(report: &Value, section: &str, subsection: &str) {
    if let Some(candidates) = report[section][subsection].as_array() {
        for candidate in candidates {
            println!(
                "\n  Finding: {}",
                candidate["qualified_name"].as_str().unwrap_or("UNKNOWN")
            );
            println!(
                "    stable_id: {}",
                candidate["stable_id"].as_str().unwrap_or("UNKNOWN")
            );
            println!(
                "    file: {}",
                candidate["file_path"].as_str().unwrap_or("UNKNOWN")
            );
            println!("    line: {}", candidate["line"].as_u64().unwrap_or(0));
            println!(
                "    state: {}",
                candidate["finding_state"].as_str().unwrap_or("UNKNOWN")
            );
            println!(
                "    evidence_basis: {}",
                candidate["evidence"]["evidence_basis"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
            );
            println!(
                "    coverage: {:?}",
                candidate["evidence"]["coverage_state"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
            );
            println!(
                "    reason: {}",
                candidate["evidence"]["reason"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
            );
            println!(
                "    analysis_method: {}",
                candidate["evidence"]["analysis_method"]
                    .as_str()
                    .unwrap_or("UNKNOWN")
            );

            if let Some(residual) = candidate["evidence"]["residual_unknowns"].as_array()
                && !residual.is_empty()
            {
                println!(
                    "    residual_unknowns: {:?}",
                    residual
                        .iter()
                        .map(|v| v.as_str().unwrap_or(""))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }

            if let Some(controls) = candidate["false_positive_controls"].as_array()
                && !controls.is_empty()
            {
                println!("    false_positive_controls:");
                for control in controls {
                    println!(
                        "      - {}: {}",
                        control["category"].as_str().unwrap_or("UNKNOWN"),
                        control["description"].as_str().unwrap_or("")
                    );
                }
            }
        }
    }
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

/// Query the daemon-owned impact result for the active repository.
pub fn fetch_impact(seed: &str) -> Result<Value, CliError> {
    daemon_request(
        "GET",
        &format!("/api/impact?seed={}", encode_query_component(seed)),
        None,
        ATTACH_TIMEOUT,
    )
}

pub fn fetch_impact_for_client(seed: &str, client_type: &str) -> Result<Value, CliError> {
    daemon_request_as(
        "GET",
        &format!("/api/impact?seed={}", encode_query_component(seed)),
        None,
        ATTACH_TIMEOUT,
        client_type,
    )
}

/// Fetch baseline from daemon
pub fn fetch_baseline(
    _cli: &Cli,
    revision: Option<&str>,
    incremental: bool,
    baseline_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "incremental": incremental,
    });
    if let Some(rev) = revision {
        body["revision"] = serde_json::Value::String(rev.to_string());
    }
    if let Some(id) = baseline_id {
        body["baseline_id"] = serde_json::Value::String(id.to_string());
    }
    daemon_request(
        "POST",
        "/api/v1/baseline",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch diff from daemon
pub fn fetch_diff(
    _cli: &Cli,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({});
    if let Some(b) = before {
        body["before"] = serde_json::Value::String(b.to_string());
    }
    if let Some(a) = after {
        body["after"] = serde_json::Value::String(a.to_string());
    }
    daemon_request(
        "POST",
        "/api/v1/diff",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch verify from daemon
pub fn fetch_verify(
    _cli: &Cli,
    strict: bool,
    invariants_file: Option<&Path>,
    baseline_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "strict": strict,
    });
    if let Some(id) = baseline_id {
        body["baseline_id"] = serde_json::Value::String(id.to_string());
    }
    if let Some(file) = invariants_file {
        body["invariants_file"] = serde_json::Value::String(file.to_string_lossy().to_string());
    }
    daemon_request(
        "POST",
        "/api/v1/verify",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch symbols from daemon
pub fn fetch_symbols(
    _cli: &Cli,
    limit: usize,
    offset: usize,
    kind: Option<&str>,
    snapshot_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut query = format!("/api/v1/symbols?limit={}&offset={}", limit, offset);
    if let Some(k) = kind {
        query.push_str(&format!("&kind={}", encode_query_component(k)));
    }
    if let Some(s) = snapshot_id {
        query.push_str(&format!("&snapshot_id={}", encode_query_component(s)));
    }
    daemon_request("GET", &query, None, ATTACH_TIMEOUT)
}

/// Fetch find from daemon
pub fn fetch_find(
    _cli: &Cli,
    pattern: &str,
    kind: Option<&str>,
    name_only: bool,
    limit: usize,
    offset: usize,
    snapshot_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut query = format!(
        "/api/v1/find?pattern={}&limit={}&offset={}",
        encode_query_component(pattern),
        limit,
        offset
    );
    if let Some(k) = kind {
        query.push_str(&format!("&kind={}", encode_query_component(k)));
    }
    if name_only {
        query.push_str("&name_only=true");
    }
    if let Some(s) = snapshot_id {
        query.push_str(&format!("&snapshot_id={}", encode_query_component(s)));
    }
    daemon_request("GET", &query, None, ATTACH_TIMEOUT)
}

/// Fetch edges from daemon
pub fn fetch_edges(
    _cli: &Cli,
    source: &str,
    direction: CliImpactDirection,
    kind: Option<&str>,
    limit: usize,
    offset: usize,
    snapshot_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut query = format!(
        "/api/v1/edges?source={}&limit={}&offset={}",
        encode_query_component(source),
        limit,
        offset
    );
    query.push_str(&format!(
        "&direction={}",
        match direction {
            CliImpactDirection::Forward => "forward",
            CliImpactDirection::Backward => "backward",
            CliImpactDirection::Bidirectional => "bidirectional",
        }
    ));
    if let Some(k) = kind {
        query.push_str(&format!("&kind={}", encode_query_component(k)));
    }
    if let Some(s) = snapshot_id {
        query.push_str(&format!("&snapshot_id={}", encode_query_component(s)));
    }
    daemon_request("GET", &query, None, ATTACH_TIMEOUT)
}

/// Fetch map from daemon
pub fn fetch_map(
    _cli: &Cli,
    top: usize,
    under: Option<&str>,
    snapshot_id: Option<&str>,
) -> Result<Value, CliError> {
    let mut query = format!("/api/v1/map?top={}", top);
    if let Some(u) = under {
        query.push_str(&format!("&under={}", encode_query_component(u)));
    }
    if let Some(s) = snapshot_id {
        query.push_str(&format!("&snapshot_id={}", encode_query_component(s)));
    }
    daemon_request("GET", &query, None, ATTACH_TIMEOUT)
}

/// Fetch session from daemon
pub fn fetch_session(
    _cli: &Cli,
    action: &str,
    session_id: Option<&str>,
    invoked: Option<Vec<String>>,
    exit_code: Option<i32>,
    snapshot_id: Option<&str>,
    note: Option<&str>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "action": action,
    });
    if let Some(id) = session_id {
        body["session_id"] = serde_json::Value::String(id.to_string());
    }
    if let Some(inv) = invoked {
        body["invoked"] =
            serde_json::Value::Array(inv.into_iter().map(serde_json::Value::String).collect());
    }
    if let Some(code) = exit_code {
        body["exit_code"] = serde_json::Value::Number(code.into());
    }
    if let Some(s) = snapshot_id {
        body["snapshot_id"] = serde_json::Value::String(s.to_string());
    }
    if let Some(n) = note {
        body["note"] = serde_json::Value::String(n.to_string());
    }
    daemon_request(
        "POST",
        "/api/v1/forensics/sessions",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch git command from daemon
pub fn fetch_git(_cli: &Cli, subcommand: &str, args: &[String]) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "command": subcommand,
    });
    if !args.is_empty() {
        body["args"] = serde_json::Value::Array(
            args.iter()
                .map(|a| serde_json::Value::String(a.clone()))
                .collect(),
        );
    }
    daemon_request(
        "POST",
        "/api/v1/git",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch analytics report from daemon
pub fn fetch_analytics_report(
    _cli: &Cli,
    format: &str,
    compact: bool,
    evidence: bool,
    state: Option<&str>,
    severity: Option<&str>,
) -> Result<Value, CliError> {
    let mut query = format!("/api/v1/analytics/report?format={}", format);
    if compact {
        query.push_str("&compact=true");
    }
    if evidence {
        query.push_str("&evidence=true");
    }
    if let Some(s) = state {
        query.push_str(&format!("&state={}", encode_query_component(s)));
    }
    if let Some(s) = severity {
        query.push_str(&format!("&severity={}", encode_query_component(s)));
    }
    daemon_request("GET", &query, None, ATTACH_TIMEOUT)
}

/// Fetch temporal from daemon
pub fn fetch_temporal(
    _cli: &Cli,
    at: Option<&str>,
    max_commits: usize,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut query = format!(
        "/api/v1/temporal?max_commits={}&strict={}&compact={}",
        max_commits, strict, compact
    );
    if let Some(a) = at {
        query.push_str(&format!("&at={}", encode_query_component(a)));
    }
    if let Some(k) = top_k {
        query.push_str(&format!("&top_k={}", k));
    }
    daemon_request("GET", &query, None, ATTACH_TIMEOUT)
}

/// Fetch lineage from daemon
#[allow(clippy::too_many_arguments)]
pub fn fetch_lineage(
    _cli: &Cli,
    from: Option<&str>,
    to: Option<&str>,
    threshold: Option<f64>,
    margin: Option<f64>,
    max_files: usize,
    max_file_bytes: u64,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "max_files": max_files,
        "max_file_bytes": max_file_bytes,
        "strict": strict,
        "compact": compact,
    });
    if let Some(f) = from {
        body["from"] = serde_json::Value::String(f.to_string());
    }
    if let Some(t) = to {
        body["to"] = serde_json::Value::String(t.to_string());
    }
    if let Some(th) = threshold {
        body["threshold"] = serde_json::Value::Number(serde_json::Number::from_f64(th).unwrap());
    }
    if let Some(m) = margin {
        body["margin"] = serde_json::Value::Number(serde_json::Number::from_f64(m).unwrap());
    }
    if let Some(k) = top_k {
        body["top_k"] = serde_json::Value::Number(k.into());
    }
    daemon_request(
        "POST",
        "/api/v1/lineage",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch cochange from daemon
#[allow(clippy::too_many_arguments)]
pub fn fetch_cochange(
    _cli: &Cli,
    window_seconds: i64,
    max_commits: usize,
    max_files_per_commit: usize,
    min_pair_commits: u32,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "window_seconds": window_seconds,
        "max_commits": max_commits,
        "max_files_per_commit": max_files_per_commit,
        "min_pair_commits": min_pair_commits,
        "strict": strict,
        "compact": compact,
    });
    if let Some(k) = top_k {
        body["top_k"] = serde_json::Value::Number(k.into());
    }
    daemon_request(
        "POST",
        "/api/v1/cochange",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch contract-dist from daemon
pub fn fetch_contract_dist(
    _cli: &Cli,
    from: &Path,
    to: &Path,
    protocol: crate::extensions::ContractProtocol,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "from": from.to_string_lossy().to_string(),
        "to": to.to_string_lossy().to_string(),
        "protocol": format!("{:?}", protocol).to_lowercase(),
        "strict": strict,
        "compact": compact,
    });
    if let Some(k) = top_k {
        body["top_k"] = serde_json::Value::Number(k.into());
    }
    daemon_request(
        "POST",
        "/api/v1/contract-dist",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch test-select from daemon
pub fn fetch_test_select(
    _cli: &Cli,
    changed: Vec<String>,
    lcov: Vec<PathBuf>,
    fail_if_all_tests_required: bool,
    strict: bool,
) -> Result<Value, CliError> {
    let body = serde_json::json!({
        "changed": changed,
        "lcov": lcov.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "fail_if_all_tests_required": fail_if_all_tests_required,
        "strict": strict,
    });
    daemon_request(
        "POST",
        "/api/v1/test-select",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch catalog from daemon
pub fn fetch_catalog(
    _cli: &Cli,
    root: Option<&Path>,
    max_file_bytes: Option<u64>,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "strict": strict,
        "compact": compact,
    });
    if let Some(r) = root {
        body["root"] = serde_json::Value::String(r.to_string_lossy().to_string());
    }
    if let Some(m) = max_file_bytes {
        body["max_file_bytes"] = serde_json::Value::Number(m.into());
    }
    if let Some(k) = top_k {
        body["top_k"] = serde_json::Value::Number(k.into());
    }
    daemon_request(
        "POST",
        "/api/v1/catalog",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch blast-radius from daemon
pub fn fetch_blast_radius(
    _cli: &Cli,
    reference: &str,
    root: Option<&Path>,
    max_file_bytes: Option<u64>,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "reference": reference,
        "strict": strict,
        "compact": compact,
    });
    if let Some(r) = root {
        body["root"] = serde_json::Value::String(r.to_string_lossy().to_string());
    }
    if let Some(m) = max_file_bytes {
        body["max_file_bytes"] = serde_json::Value::Number(m.into());
    }
    if let Some(k) = top_k {
        body["top_k"] = serde_json::Value::Number(k.into());
    }
    daemon_request(
        "POST",
        "/api/v1/blast-radius",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Fetch telemetry from daemon
#[allow(clippy::too_many_arguments)]
pub fn fetch_telemetry(
    _cli: &Cli,
    traces: &Path,
    salt: &str,
    surface: Option<&Path>,
    windows: u32,
    complete_observation: bool,
    dispatch_static: bool,
    strict: bool,
    compact: bool,
    top_k: Option<usize>,
) -> Result<Value, CliError> {
    let mut body = serde_json::json!({
        "traces": traces.to_string_lossy().to_string(),
        "salt": salt,
        "windows": windows,
        "complete_observation": complete_observation,
        "dispatch_static": dispatch_static,
        "strict": strict,
        "compact": compact,
    });
    if let Some(s) = surface {
        body["surface"] = serde_json::Value::String(s.to_string_lossy().to_string());
    }
    if let Some(k) = top_k {
        body["top_k"] = serde_json::Value::Number(k.into());
    }
    daemon_request(
        "POST",
        "/api/v1/telemetry",
        Some(&body.to_string()),
        ATTACH_TIMEOUT,
    )
}

/// Handle command via daemon (for offline=false default)
pub fn handle_via_daemon(cli: &Cli) -> Result<i32, CliError> {
    // Connect to daemon first
    let runtime_request = serde_json::json!({
        "client_id": daemon_client_id(),
        "client_type": "cli",
        "repository_id": Value::Null,
    });
    let _runtime = daemon_request(
        "POST",
        "/api/v1/runtime/connect",
        Some(&runtime_request.to_string()),
        STATUS_TIMEOUT,
    )?;

    // Route to appropriate daemon handler
    match &cli.command {
        Commands::Baseline {
            revision,
            incremental,
            baseline_id,
        } => {
            let value = fetch_baseline(
                cli,
                revision.as_deref(),
                *incremental,
                baseline_id.as_deref(),
            )?;
            print_result(cli, &value)
        }
        Commands::Diff { before, after } => {
            let value = fetch_diff(cli, before.as_deref(), after.as_deref())?;
            print_result(cli, &value)
        }
        Commands::Impact {
            symbol,
            direction,
            max_depth,
            budget,
            snapshot_id,
            edge_kinds,
            profile,
            top_k,
            detail,
        } => {
            let mut query = format!(
                "/api/impact?seed={}&direction={}&max_depth={}&budget={}&profile={:?}&detail={:?}",
                encode_query_component(&symbol[0]),
                match direction {
                    Some(d) => format!("{:?}", d).to_lowercase(),
                    None => "forward".to_string(),
                },
                max_depth,
                budget,
                profile,
                detail
            );
            if let Some(s) = &snapshot_id {
                query.push_str(&format!("&snapshot_id={}", encode_query_component(s)));
            }
            if let Some(e) = &edge_kinds {
                query.push_str(&format!("&edge_kinds={}", encode_query_component(e)));
            }
            if let Some(k) = top_k {
                query.push_str(&format!("&top_k={}", k));
            }
            let value = daemon_request("GET", &query, None, ATTACH_TIMEOUT)?;
            print_result(cli, &value)
        }
        Commands::Verify {
            strict,
            invariants_file,
            baseline_id,
        } => {
            let value = fetch_verify(
                cli,
                *strict,
                invariants_file.as_deref(),
                baseline_id.as_deref(),
            )?;
            print_result(cli, &value)
        }
        Commands::Symbols {
            limit,
            offset,
            kind,
            snapshot_id,
        } => {
            let value = fetch_symbols(
                cli,
                *limit,
                *offset,
                kind.as_deref(),
                snapshot_id.as_deref(),
            )?;
            print_result(cli, &value)
        }
        Commands::Find {
            pattern,
            kind,
            name_only,
            limit,
            offset,
            snapshot_id,
        } => {
            let value = fetch_find(
                cli,
                pattern,
                kind.as_deref(),
                *name_only,
                *limit,
                *offset,
                snapshot_id.as_deref(),
            )?;
            print_result(cli, &value)
        }
        Commands::Edges {
            source,
            direction,
            kind,
            limit,
            offset,
            snapshot_id,
        } => {
            let value = fetch_edges(
                cli,
                source,
                *direction,
                kind.as_deref(),
                *limit,
                *offset,
                snapshot_id.as_deref(),
            )?;
            print_result(cli, &value)
        }
        Commands::Map {
            top,
            under,
            snapshot_id,
        } => {
            let value = fetch_map(cli, *top, under.as_deref(), snapshot_id.as_deref())?;
            print_result(cli, &value)
        }
        Commands::Session { action } => match action {
            crate::session::SessionAction::Start => {
                let value = fetch_session(cli, "start", None, None, None, None, None)?;
                print_result(cli, &value)
            }
            crate::session::SessionAction::Record {
                session_id,
                cli_arg,
                exit_code,
                snapshot_id,
                note,
            } => {
                let value = fetch_session(
                    cli,
                    "record",
                    Some(session_id),
                    Some(cli_arg.clone()),
                    Some(*exit_code),
                    snapshot_id.as_deref(),
                    note.as_deref(),
                )?;
                print_result(cli, &value)
            }
            crate::session::SessionAction::Show { session_id } => {
                let value = fetch_session(cli, "show", Some(session_id), None, None, None, None)?;
                print_result(cli, &value)
            }
            crate::session::SessionAction::Close { session_id } => {
                let value = fetch_session(cli, "close", Some(session_id), None, None, None, None)?;
                print_result(cli, &value)
            }
            crate::session::SessionAction::Report { session_id } => {
                let value =
                    fetch_session(cli, "report", session_id.as_deref(), None, None, None, None)?;
                print_result(cli, &value)
            }
            crate::session::SessionAction::List => {
                let value = fetch_session(cli, "list", None, None, None, None, None)?;
                print_result(cli, &value)
            }
        },
        Commands::Git { command } => {
            let args: Vec<String> = match command {
                crate::git_cmd::GitSubcommands::Status => vec!["status".to_string()],
                crate::git_cmd::GitSubcommands::Galaxy { limit } => vec![
                    "galaxy".to_string(),
                    "--limit".to_string(),
                    limit.to_string(),
                ],
                crate::git_cmd::GitSubcommands::Diff { base, head } => vec![
                    "diff".to_string(),
                    "--base".to_string(),
                    base.to_string(),
                    "--head".to_string(),
                    head.to_string(),
                ],
                crate::git_cmd::GitSubcommands::Stage { paths } => vec!["stage".to_string()]
                    .into_iter()
                    .chain(paths.iter().cloned())
                    .collect(),
                crate::git_cmd::GitSubcommands::Commit {
                    message,
                    tag,
                    author,
                    email,
                } => {
                    let mut v = vec![
                        "commit".to_string(),
                        "--message".to_string(),
                        message.to_string(),
                    ];
                    if let Some(t) = tag {
                        v.push("--tag".to_string());
                        v.push(t.to_string());
                    }
                    if let Some(a) = author {
                        v.push("--author".to_string());
                        v.push(a.to_string());
                    }
                    if let Some(e) = email {
                        v.push("--email".to_string());
                        v.push(e.to_string());
                    }
                    v
                }
                crate::git_cmd::GitSubcommands::Branch { name, action } => {
                    let mut v = vec!["branch".to_string()];
                    if let Some(n) = name {
                        v.push(n.to_string());
                    }
                    v.push(action.to_string());
                    v
                }
                crate::git_cmd::GitSubcommands::Tag {
                    name,
                    target,
                    delete,
                } => {
                    let mut v = vec!["tag".to_string(), name.to_string()];
                    if let Some(t) = target {
                        v.push("--target".to_string());
                        v.push(t.to_string());
                    }
                    if *delete {
                        v.push("--delete".to_string());
                    }
                    v
                }
                crate::git_cmd::GitSubcommands::Merge { source } => vec![
                    "merge".to_string(),
                    "--source".to_string(),
                    source.to_string(),
                ],
                crate::git_cmd::GitSubcommands::DiffFile { path } => vec![
                    "diff-file".to_string(),
                    "--path".to_string(),
                    path.to_string(),
                ],
                crate::git_cmd::GitSubcommands::Unstage { paths } => {
                    let mut v = vec!["unstage".to_string()];
                    v.extend(paths.iter().cloned());
                    v
                }
                crate::git_cmd::GitSubcommands::Bisect { good, bad } => vec![
                    "bisect".to_string(),
                    "--good".to_string(),
                    good.to_string(),
                    "--bad".to_string(),
                    bad.to_string(),
                ],
                crate::git_cmd::GitSubcommands::Undo => vec!["undo".to_string()],
            };
            let value = fetch_git(cli, "git", &args)?;
            print_result(cli, &value)
        }
        Commands::AnalyticsReport {
            format,
            compact,
            evidence,
            state,
            severity,
        } => {
            let value = fetch_analytics_report(
                cli,
                format,
                *compact,
                *evidence,
                state.as_deref(),
                severity.as_deref(),
            )?;
            print_result(cli, &value)
        }
        Commands::Temporal {
            at,
            max_commits,
            strict,
            compact,
            top_k,
        } => {
            let value =
                fetch_temporal(cli, at.as_deref(), *max_commits, *strict, *compact, *top_k)?;
            print_result(cli, &value)
        }
        Commands::Lineage {
            from,
            to,
            threshold,
            margin,
            max_files,
            max_file_bytes,
            strict,
            compact,
            top_k,
        } => {
            let value = fetch_lineage(
                cli,
                from.as_deref(),
                to.as_deref(),
                *threshold,
                *margin,
                *max_files,
                *max_file_bytes,
                *strict,
                *compact,
                *top_k,
            )?;
            print_result(cli, &value)
        }
        Commands::Cochange {
            window_seconds,
            max_commits,
            max_files_per_commit,
            min_pair_commits,
            strict,
            compact,
            top_k,
        } => {
            let value = fetch_cochange(
                cli,
                *window_seconds,
                *max_commits,
                *max_files_per_commit,
                *min_pair_commits,
                *strict,
                *compact,
                *top_k,
            )?;
            print_result(cli, &value)
        }
        Commands::ContractDist {
            from,
            to,
            protocol,
            strict,
            compact,
            top_k,
        } => {
            let value = fetch_contract_dist(cli, from, to, *protocol, *strict, *compact, *top_k)?;
            print_result(cli, &value)
        }
        Commands::TestSelect {
            changed,
            lcov,
            fail_if_all_tests_required,
            strict,
        } => {
            let value = fetch_test_select(
                cli,
                changed.clone(),
                lcov.clone(),
                *fail_if_all_tests_required,
                *strict,
            )?;
            print_result(cli, &value)
        }
        Commands::Catalog {
            root,
            max_file_bytes,
            strict,
            compact,
            top_k,
        } => {
            let value = fetch_catalog(
                cli,
                root.as_deref(),
                *max_file_bytes,
                *strict,
                *compact,
                *top_k,
            )?;
            print_result(cli, &value)
        }
        Commands::BlastRadius {
            reference,
            root,
            max_file_bytes,
            strict,
            compact,
            top_k,
        } => {
            let value = fetch_blast_radius(
                cli,
                reference,
                root.as_deref(),
                *max_file_bytes,
                *strict,
                *compact,
                *top_k,
            )?;
            print_result(cli, &value)
        }
        Commands::Telemetry {
            input,
            salt,
            salt_file,
            surface,
            windows,
            complete_observation,
            dispatch_static,
            strict,
            compact,
            top_k,
        } => {
            let salt_str = if let Some(sf) = salt_file {
                std::fs::read_to_string(sf)
                    .map_err(|e| CliError::Analysis(e.to_string()))?
                    .trim()
                    .to_string()
            } else {
                salt.clone().unwrap_or_default()
            };
            let value = fetch_telemetry(
                cli,
                input,
                &salt_str,
                surface.as_deref(),
                *windows,
                *complete_observation,
                *dispatch_static,
                *strict,
                *compact,
                *top_k,
            )?;
            print_result(cli, &value)
        }
        _ => {
            // Commands that don't have daemon handlers yet - fall back to local execution
            crate::execute(cli)
        }
    }
}

fn print_result(cli: &Cli, value: &Value) -> Result<i32, CliError> {
    match cli.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(value)
                .map_err(|error| CliError::Analysis(error.to_string()))?
        ),
        OutputFormat::Text => {
            println!(
                "daemon: {}",
                value
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("UNKNOWN")
            );
            for key in [
                "active_repo_id",
                "active_repo_path",
                "snapshot_id",
                "last_event_seq",
                "storage_health",
            ] {
                if let Some(item) = value.get(key) {
                    let rendered = item
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| item.to_string());
                    println!("{key}: {rendered}");
                }
            }
        }
        OutputFormat::Sarif => println!(
            "{}",
            serde_json::to_string(value).map_err(|error| CliError::Analysis(error.to_string()))?
        ),
    }
    Ok(EXIT_PASS)
}

pub fn handle_attach(
    cli: &Cli,
    requested_path: &Path,
    display_name: Option<&str>,
) -> Result<i32, CliError> {
    let path = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        cli.repo_root.join(requested_path)
    };
    let canonical = path.canonicalize().map_err(|error| {
        CliError::Analysis(format!(
            "ERR-REPO-001: repository directory does not exist or is not accessible: {} ({error})",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(CliError::Analysis(format!(
            "ERR-REPO-001: repository path is not a directory: {}",
            canonical.display()
        )));
    }
    let canonical_path = canonical.to_string_lossy().replace('\\', "/");
    let inferred_name = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repository");
    let request = serde_json::json!({
        "root_path": canonical_path,
        "display_name": display_name.filter(|name| !name.trim().is_empty()).unwrap_or(inferred_name),
    });
    let value = daemon_request(
        "POST",
        "/api/v1/repos/connect",
        Some(&request.to_string()),
        ATTACH_TIMEOUT,
    )?;
    let runtime_request = serde_json::json!({
        "client_id": daemon_client_id(),
        "client_type": "cli",
        "repository_id": value.get("repo_id")
    });
    let runtime = daemon_request(
        "POST",
        "/api/v1/runtime/connect",
        Some(&runtime_request.to_string()),
        STATUS_TIMEOUT,
    )?;
    print_result(cli, &runtime)
}

pub fn handle_status(cli: &Cli) -> Result<i32, CliError> {
    let value = fetch_status()?;
    print_result(cli, &value)
}

pub fn handle_active_repo(cli: &Cli) -> Result<i32, CliError> {
    handle_status(cli)
}

pub fn handle_events(cli: &Cli, since_seq: i64) -> Result<i32, CliError> {
    let value = fetch_live_events(since_seq)?;
    print_result(cli, &value)
}

pub fn handle_disconnect(cli: &Cli) -> Result<i32, CliError> {
    let body = serde_json::json!({
        "client_id": daemon_client_id(),
        "lease_id": std::env::var("SYNEVID_LEASE_ID").ok()
    });
    let value = daemon_request(
        "POST",
        "/api/v1/runtime/disconnect",
        Some(&body.to_string()),
        STATUS_TIMEOUT,
    )?;
    print_result(cli, &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn attach_rejects_missing_path_without_fallback() {
        let cli = Cli::parse_from([
            "autopsy",
            "--repo-root",
            ".",
            "attach",
            "missing-repository",
        ]);
        let error = handle_attach(&cli, Path::new("missing-repository"), None).unwrap_err();
        assert!(error.to_string().contains("ERR-REPO-001"));
    }

    #[test]
    fn impact_seed_query_encoding_rejects_url_delimiters() {
        assert_eq!(
            encode_query_component("pkg::Thing#run/a"),
            "pkg%3A%3AThing%23run%2Fa"
        );
        assert_eq!(encode_query_component("simple-name_1"), "simple-name_1");
    }
}
