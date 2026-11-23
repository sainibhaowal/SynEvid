//! Reactive Daemon server & REST API (autopsy ui).
//!
//! DEPRECATED: This is now a thin wrapper that prints a deprecation notice.
//! The canonical daemon is `autopsy-daemon`.
//! All API endpoints have moved to `autopsy-daemon` (crates/autopsy-daemon/src/main.rs).

use crate::{Cli, CliError, EXIT_PASS, OutputFormat};
use serde::Deserialize;

pub fn handle_ui(cli: &Cli, port: u16, host: &str, _open: bool) -> Result<i32, CliError> {
    // Phase 7: Deprecation notice - autopsy ui is now a thin wrapper
    // The canonical daemon is autopsy-daemon
    eprintln!("⚠️  DEPRECATED: 'autopsy ui' is deprecated and will be removed in a future version.");
    eprintln!("   The canonical daemon is 'autopsy-daemon' (single binary, same API).");
    eprintln!("   Run instead: autopsy-daemon --host {} --port {} --repo-root {}",
        host, port, cli.repo_root.display());
    eprintln!();
    eprintln!("   If you need the web dashboard, run 'autopsy-daemon' in one terminal,");
    eprintln!("   then open http://{}:{} in your browser.", host, port);
    eprintln!();

    // For JSON output, return the deprecation info
    if cli.format == OutputFormat::Json {
        let doc = serde_json::json!({
            "command": "ui",
            "status": "deprecated",
            "message": "Use 'autopsy-daemon' instead. This command will be removed.",
            "canonical_daemon": "autopsy-daemon",
            "example": format!("autopsy-daemon --host {} --port {} --repo-root {}", host, port, cli.repo_root.display()),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).map_err(|e| CliError::Analysis(e.to_string()))?
        );
        return Ok(EXIT_PASS);
    }

    // Exit with code indicating deprecation (not an error, but not success)
    eprintln!("Exiting. Please use 'autopsy-daemon' directly.");
    Ok(EXIT_PASS)
}