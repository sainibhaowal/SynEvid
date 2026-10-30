//! synevid-ui: Local Web Dashboard Server for SynEvid (Code Autopsy).

mod indexer;
mod server;

use clap::Parser;
use server::DashboardServer;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "synevid-ui",
    author = "Synevid Core Team",
    version = "0.1.0",
    about = "Embedded Local Web Dashboard for SynEvid change-impact and forensic trajectory analysis"
)]
struct Args {
    /// Port to listen on (default: 3333)
    #[arg(long, default_value_t = 3333)]
    port: u16,

    /// Host interface to bind to (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Target repository root directory (default: current directory)
    #[arg(short = 'r', long, default_value = ".")]
    repo_root: PathBuf,

    /// Automatically open dashboard in default web browser
    #[arg(long)]
    open: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let server = DashboardServer::new(args.repo_root, args.host.clone(), args.port);

    if args.open {
        let url = format!("http://{}:{}", args.host, args.port);
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd")
            .args(["/c", "start", &url])
            .spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&url).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }

    server.run()
}
