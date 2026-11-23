//! Daemon configuration and CLI argument parsing

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;
use figment::{Figment, providers::{Format, Toml, Env}};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};

/// Daemon configuration loaded from CLI args, env vars, and config file
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// Host interface to bind to
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// Daemon-owned application data directory
    pub data_dir: PathBuf,
    /// Repository root for managed runtime mode
    pub repo_root: Option<PathBuf>,
    /// Run in managed runtime mode (restore from registry only)
    pub managed_runtime: bool,
    /// Disable loopback authentication (NOT for production)
    pub no_auth: bool,
    /// Log level
    pub log_level: String,
    /// Enable structured JSON logging
    pub json_logs: bool,
    /// Read timeout for requests
    #[serde_as(as = "DurationSeconds<u64>")]
    pub read_timeout: Duration,
    /// Write timeout for responses
    #[serde_as(as = "DurationSeconds<u64>")]
    pub write_timeout: Duration,
    /// Idle timeout for keep-alive connections
    #[serde_as(as = "DurationSeconds<u64>")]
    pub idle_timeout: Duration,
    /// Max request body size (bytes)
    pub max_body_size: usize,
    /// Enable WebSocket support
    pub enable_websocket: bool,
    /// Max WebSocket frame size
    pub max_ws_frame_size: usize,
    /// Max concurrent connections
    pub max_connections: usize,
    /// Request timeout
    #[serde_as(as = "DurationSeconds<u64>")]
    pub request_timeout: Duration,
    /// Enable request/response compression
    pub compression: bool,
    /// Trusted proxy headers
    pub trusted_proxies: Vec<String>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3333,
            data_dir: default_data_dir(),
            repo_root: None,
            managed_runtime: false,
            no_auth: false,
            log_level: "info".to_string(),
            json_logs: false,
            read_timeout: Duration::from_secs(30),
            write_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(60),
            max_body_size: 50 * 1024 * 1024, // 50MB
            enable_websocket: true,
            max_ws_frame_size: 16 * 1024 * 1024, // 16MB
            max_connections: 1000,
            request_timeout: Duration::from_secs(120),
            compression: true,
            trusted_proxies: vec!["127.0.0.1".to_string(), "::1".to_string()],
        }
    }
}

fn default_data_dir() -> PathBuf {
    if cfg!(target_os = "windows") {
        dirs::data_dir()
            .map(|p| p.join("SynEvid"))
            .unwrap_or_else(|| PathBuf::from(".autopsy"))
    } else {
        dirs::home_dir()
            .map(|p| p.join(".autopsy"))
            .unwrap_or_else(|| PathBuf::from(".autopsy"))
    }
}

impl DaemonConfig {
    /// Load configuration from CLI args, environment variables, and config file
    pub fn load() -> anyhow::Result<Self> {
        let args = Args::parse();
        
        let mut figment = Figment::new()
            .merge(Serialized::defaults(Self::default()))
            .merge(Env::prefixed("SYNEVID_").split("_"))
            .merge(Toml::file("autopsy-daemon.toml").nested());
        
        // Apply CLI args
        if args.port != 3333 { figment = figment.merge(("port", args.port)); }
        if args.host != "127.0.0.1" { figment = figment.merge(("host", args.host.clone())); }
        if let Some(data_dir) = &args.data_dir { figment = figment.merge(("data_dir", data_dir.clone())); }
        if let Some(repo_root) = &args.repo_root { figment = figment.merge(("repo_root", repo_root.clone())); }
        if args.managed_runtime { figment = figment.merge(("managed_runtime", true)); }
        if args.no_auth { figment = figment.merge(("no_auth", true)); }
        if args.log_level != "info" { figment = figment.merge(("log_level", args.log_level.clone())); }
        if args.json_logs { figment = figment.merge(("json_logs", true)); }
        
        let config: DaemonConfig = figment.extract()?;
        
        // Validate
        if config.port == 0 {
            anyhow::bail!("Port cannot be 0");
        }
        if config.max_body_size == 0 {
            anyhow::bail!("Max body size cannot be 0");
        }
        if config.max_connections == 0 {
            anyhow::bail!("Max connections cannot be 0");
        }
        
        Ok(config)
    }
    
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port).parse()
            .expect("Invalid socket address")
    }
    
    pub fn base_url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }
}

/// CLI arguments (parsed by clap)
#[derive(Parser, Debug)]
#[command(
    name = "autopsy-daemon",
    author = "Synevid Core Team",
    version = env!("CARGO_PKG_VERSION"),
    about = "SynEvid Authenticated Local Daemon — Single Source of Truth for Repository, Session, Snapshot, Event Cursor, and Lease Management"
)]
struct Args {
    /// Port to listen on (default: 3333)
    #[arg(long, default_value_t = 3333)]
    port: u16,

    /// Host interface to bind to (default: 127.0.0.1)
    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Daemon-owned application data directory
    #[arg(long)]
    data_dir: Option<PathBuf>,

    /// Repository root for managed runtime mode
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config() {
        let config = DaemonConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3333);
        assert!(!config.no_auth);
    }
}