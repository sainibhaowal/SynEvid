//! `synevid-lsp` binary: the IDE-native surface (Tier 3.2).
//!
//! Usage — editors launch this server over stdio:
//!
//! ```text
//! synevid-lsp [--repo-root <PATH>]
//! ```
//!
//! When `--repo-root` is omitted, the workspace root is taken from the client's
//! `initialize` request (`rootUri`, then the first workspace folder), falling back
//! to the process working directory. All diagnostics go to stderr; stdout carries
//! the JSON-RPC frames only, because that stream is the protocol.

use std::path::PathBuf;

fn usage() -> &'static str {
    "synevid-lsp — inline impact hovers from the Synevid engine (Tier 3.2)

USAGE:
    synevid-lsp [--repo-root <PATH>]

OPTIONS:
    -r, --repo-root <PATH>   Workspace root to analyze (default: the editor's
                             rootUri/workspace folder, then the current directory)
    -h, --help               Print this help
    -V, --version            Print version

PROTOCOL:
    Language Server Protocol (JSON-RPC 2.0) over stdin/stdout.
    Capabilities: hover (impact summary with callers/callees/coverage)."
}

fn main() -> std::process::ExitCode {
    let mut repo_root: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-r" | "--repo-root" => match args.next() {
                Some(value) => repo_root = Some(PathBuf::from(value)),
                None => {
                    eprintln!("synevid-lsp: --repo-root requires a path");
                    return std::process::ExitCode::from(2);
                }
            },
            "-h" | "--help" => {
                println!("{}", usage());
                return std::process::ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("{} {}", synevid_lsp::SERVER_NAME, env!("CARGO_PKG_VERSION"));
                return std::process::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("synevid-lsp: unknown argument `{other}`");
                eprintln!("{}", usage());
                return std::process::ExitCode::from(2);
            }
        }
    }

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let options = synevid_lsp::ServeOptions { repo_root };
    match synevid_lsp::serve_loop(stdin.lock(), stdout.lock(), options) {
        Ok(outcome) => std::process::ExitCode::from(outcome.exit_code as u8),
        Err(error) => {
            eprintln!("synevid-lsp: I/O failure: {error}");
            std::process::ExitCode::from(1)
        }
    }
}
