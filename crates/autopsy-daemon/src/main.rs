//! autopsy-daemon: Consolidated Daemon Binary
//!
//! This is the consolidated daemon binary that replaces both:
//! - `autopsy ui` (CLI-embedded server in crates/autopsy-cli/src/ui.rs)
//! - `synevid-ui` (Electron sidecar in apps/synevid-ui/src/server.rs)
//!
//! This is a thin wrapper that delegates to the working `synevid-ui` implementation.
//! All clients (CLI, TUI, MCP, Web, Electron) connect to this daemon.

use std::path::PathBuf;

use clap::Parser;
use synevid_ui::server::DashboardServer;

#[derive(Parser, Debug)]
#[command(
    name = "autopsy-daemon",
    author = "Synevid Core Team",
    version = env!("CARGO_PKG_VERSION"),
    about = "SynEvid Authenticated Local Daemon — Single Source of Truth for Repository, Session, Snapshot, and Event State"
)]
struct Args {
    /// Port to listen on (default: 3333)
    #[arg(long, default_value_t = 3333)]
    port: u16,

    /// Host interface to bind to (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Daemon-owned application data directory (default: ~/.autopsy or %APPDATA%\SynEvid)
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Repository root for managed runtime mode (daemon selects active repo from registry only)
    #[arg(long)]
    repo_root: Option<PathBuf>,

    /// Run in managed runtime mode: do not auto-register repo_root, only restore from registry
    #[arg(long)]
    managed_runtime: bool,

    /// Disable loopback authentication (NOT for production)
    #[arg(long)]
    no_auth: bool,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Enable structured JSON logging
    #[arg(long)]
    json_logs: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize tracing
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&args.log_level));
    
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);
    
    if args.json_logs {
        subscriber
            .with_ansi(false)
            .init();
    } else {
        subscriber.init();
    }

    tracing::info!("Starting SynEvid Authenticated Daemon v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Managed runtime: {}", args.managed_runtime);
    tracing::info!("Auth enabled: {}", !args.no_auth);

    // Determine data directory
    let data_dir = args.data_dir.unwrap_or_else(|| {
        if cfg!(target_os = "windows") {
            dirs::data_dir()
                .map(|p| p.join("SynEvid"))
                .unwrap_or_else(|| PathBuf::from(".autopsy"))
        } else {
            dirs::home_dir()
                .map(|p| p.join(".autopsy"))
                .unwrap_or_else(|| PathBuf::from(".autopsy"))
        }
    });

    // Create data directory
    std::fs::create_dir_all(&data_dir)?;

    // Set up loopback token
    if !args.no_auth {
        // Generate a loopback token if not already set
        if std::env::var("SYNEVID_LOOPBACK_TOKEN").is_err() {
            let token = blake3::hash(
                format!("synevid-loopback-{}-{}", 
                    std::process::id(), 
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ).as_bytes()
            ).to_hex().to_string();
            std::env::set_var("SYNEVID_LOOPBACK_TOKEN", &token);
        }
    }

    // Determine repo_root for managed runtime
    let repo_root = if args.managed_runtime {
        data_dir.clone()
    } else {
        args.repo_root.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    };

    tracing::info!("Starting SynEvid Authenticated Daemon v{}", env!("CARGO_PKG_VERSION"));
    tracing::info!("Managed runtime: {}", args.managed_runtime);
    tracing::info!("Auth enabled: {}", !args.no_auth);

    // Create and run server
    let addr: std::net::SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let server = if args.managed_runtime {
        DashboardServer::new_managed_runtime(repo_root, args.host.clone(), args.port)
    } else {
        DashboardServer::new(repo_root, args.host.clone(), args.port)
    };
    
    tracing::info!("Daemon listening on http://{}", addr);
    tracing::info!("Data directory: {}", data_dir.display());
    tracing::info!("Press Ctrl+C to stop");

    // Run server (blocks until shutdown)
    server.run()?;
    
    Ok(())
}