//! Web Dashboard launcher & guidance command for SynEvid (Code Autopsy).
//!
//! Provides CLI instructions and JSON telemetry for launching the zero-npm
//! embedded web dashboard (`synevid-ui`).
//!
//! In accordance with the SynEvid architectural inversion rules (INV-SANDBOX-004
//! and ARCH_NO_CORE_TO_MCP), networking sockets and presentation HTTP servers
//! live strictly in `apps/*` (specifically `apps/synevid-ui`), while core crates
//! maintain 100% offline determinism and zero network sockets.

use crate::{Cli, CliError, EXIT_PASS, OutputFormat};
use serde_json::json;

pub fn handle_ui(cli: &Cli, port: u16, host: &str, open: bool) -> Result<i32, CliError> {
    let url = format!("http://{}:{}", host, port);

    if cli.format == OutputFormat::Json {
        let doc = json!({
            "command": "ui",
            "status": "ready",
            "server_binary": "synevid-ui",
            "url": url,
            "port": port,
            "host": host,
            "open_browser": open,
            "repo_root": cli.repo_root.display().to_string(),
            "instructions": format!("Run `synevid-ui --port {} --host {} --repo-root {}` to start the embedded web dashboard.", port, host, cli.repo_root.display())
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&doc).map_err(|e| CliError::Analysis(e.to_string()))?
        );
    } else {
        println!(
            "╔════════════════════════════════════════════════════════════════════════════════════╗"
        );
        println!(
            "║  SynEvid Embedded Web Dashboard                                                    ║"
        );
        println!(
            "╚════════════════════════════════════════════════════════════════════════════════════╝"
        );
        println!();
        println!("  To launch the zero-npm offline Web Inspector, run:");
        println!(
            "    synevid-ui --port {} --host {} --repo-root \"{}\"{}",
            port,
            host,
            cli.repo_root.display(),
            if open { " --open" } else { "" }
        );
        println!();
        println!("  Local Dashboard URL: {}", url);
        println!("  Architecture Note: `synevid-ui` runs in apps/synevid-ui to preserve");
        println!("  the strict zero-network-socket sandbox in core crates.");
    }

    Ok(EXIT_PASS)
}
