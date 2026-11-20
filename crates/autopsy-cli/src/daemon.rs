//! Authenticated local-daemon client commands.
//!
//! `autopsy attach` and `autopsy status` are presentation-layer commands. They
//! never run source code or Git commands and never maintain a second repository
//! registry: the daemon remains the authority for the active repository,
//! snapshot, event cursor, and storage health.

use crate::{Cli, CliError, EXIT_PASS, OutputFormat};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
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
///
/// The TUI uses this endpoint whenever the daemon is available so it cannot
/// silently display a graph built from a different snapshot than the Electron
/// UI, CLI attach session, or MCP client.  The response is the same envelope
/// returned by the web terminal's `autopsy impact` command.
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
