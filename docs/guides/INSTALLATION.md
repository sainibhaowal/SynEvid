# SynEvid Installation Guide

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: All platforms (Linux, macOS, Windows)

## System Requirements

### Minimum Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 2 cores | 8+ cores |
| RAM | 2 GB | 16+ GB |
| Disk | 2 GB free | 50+ GB free (NVMe) |
| OS | Linux/macOS/Windows | Ubuntu 22.04+, macOS 13+, Windows 10+ |
| Rust | 1.90+ (MSRV) | Latest stable |

### Software Dependencies

| Dependency | Version | Purpose |
|------------|---------|---------|
| Rust | 1.90+ | Core engine |
| Git | 2.40+ | Repository access |
| SQLite | 3.41+ | Embedded storage |
| Node.js | 20+ | Web UI/Electron |
| Python | 3.11+ | Benchmarks/scripts |

## Installation Methods

### Method 1: Cargo Install (Recommended)

```bash
# Install from crates.io (when published)
cargo install autopsy-daemon autopsy-cli

# Or from source
git clone https://github.com/sainibhaowal/SynEvid
cd SynEvid
cargo install --path crates/autopsy-daemon --path crates/autopsy-cli --locked
```

### Method 2: Binary Release

```bash
# Download from GitHub Releases
# Linux x86_64
wget https://github.com/sainibhaowal/SynEvid/releases/download/v0.6.0/autopsy-x86_64-unknown-linux-gnu.tar.gz
tar -xzf autopsy-x86_64-unknown-linux-gnu.tar.gz
sudo mv autopsy-daemon autopsy-cli /usr/local/bin/

# macOS (Apple Silicon)
wget https://github.com/sainibhaowal/SynEvid/releases/download/v0.6.0/autopsy-aarch64-apple-darwin.tar.gz

# Windows
# Download autopsy-x86_64-pc-windows-msvc.zip from releases
```

### Method 3: From Source (Development)

```bash
# Clone and build
git clone https://github.com/sainibhaowal/SynEvid
cd SynEvid
cargo build --release --workspace

# Binaries at:
# ./target/release/autopsy-daemon
# ./target/release/autopsy
```

### Method 4: Docker

```dockerfile
# Dockerfile
FROM rust:1.90-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release --workspace

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    git sqlite3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/autopsy-daemon /usr/local/bin/
COPY --from=builder /app/target/release/autopsy /usr/local/bin/

EXPOSE 3333
ENTRYPOINT ["autopsy-daemon"]
CMD ["--port", "3333", "--no-auth"]
```

```bash
docker build -t synvid/autopsy:0.6.0 .
docker run -d -p 3333:3333 -v /data:/data synvid/autopsy:0.6.0
```

### Method 5: Cargo Binstall (Fastest)

```bash
# Install binstall
cargo install cargo-binstall

# Install pre-built binaries
cargo binstall autopsy-daemon autopsy-cli
```

## Platform-Specific Notes

### Linux

```bash
# Install dependencies
sudo apt-get update && apt-get install -y \
    git sqlite3 libsqlite3-dev \
    pkg-config libssl-dev \
    pkg-config libclang-dev \
    clang

# For Wayland (Electron)
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.0-dev

# Systemd service
sudo tee /etc/systemd/system/autopsy-daemon.service <<EOF
[Unit]
Description=SynEvid Authenticated Daemon
After=network.target

[Service]
Type=simple
User=synevid
WorkingDirectory=/home/synevid
ExecStart=/usr/local/bin/autopsy-daemon --port 3333 --data-dir /var/lib/synevid
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now autopsy-daemon
```

### macOS

```bash
# Homebrew
brew install git sqlite rust

# Or MacPorts
sudo port install git sqlite3 rust

# LaunchAgent for daemon
cat > ~/Library/LaunchAgents/com.synevid.daemon.plist <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>com.synevid.daemon</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/autopsy-daemon</string>
        <string>--port</string><string>3333</string>
        <string>--data-dir</string><string>/Users/$(whoami)/.autopsy</string>
    </array>
    <key>RunAtLoad</key><true/>
    <key>KeepAlive</key><true/>
</dict>
</plist>

launchctl load ~/Library/LaunchAgents/com.synevid.daemon.plist
```

### Windows

```powershell
# Chocolatey
choco install git sqlite rust

# Scoop
scoop install git sqlite rust

# Download and install from releases
# 1. Download autopsy-x86_64-pc-windows-msvc.zip
# 2. Extract to C:\Program Files\SynEvid\
# 3. Add to PATH

# Run as service (NSSM)
nssm install SynEvidDaemon "C:\Program Files\SynEvid\autopsy-daemon.exe" "--port 3333 --data-dir C:\ProgramData\SynEvid"
nssm set SynEvidDaemon AppStdout "C:\ProgramData\SynEvid\daemon.log"
nssm set SynEvidDaemon AppStderr "C:\ProgramData\SynEvid\daemon.err"
nssm start SynEvidDaemon
```

## Post-Installation

### Verify Installation

```bash
# Check versions
autopsy-daemon --version
autopsy --version

# Test daemon
autopsy-daemon --port 3333 --no-auth &
sleep 2
curl http://127.0.0.1:3333/api/v1/health
# Should return: {"status":"ok","version":"0.6.0","daemon":"autopsy-daemon"}

# Test CLI
autopsy doctor
# Should show all checks passing
```

### Initialize Repository

```bash
cd /path/to/your/repo

# Initialize (creates .autopsy/ and baseline)
autopsy baseline --persist

# Verify
autopsy status
```

### Configure Authentication

```bash
# Generate loopback token (optional, auto-generated if not set)
export SYNEVID_LOOPBACK_TOKEN=$(autopsy-daemon --generate-token)

# Or set in environment
echo 'export SYNEVID_LOOPBACK_TOKEN="your-token-here"' >> ~/.bashrc
source ~/.bashrc
```

### Configure Daemon

```bash
# Create config file
cat > autopsy.toml <<EOF
[daemon]
port = 3333
host = "127.0.0.1"
data_dir = "~/.autopsy"
managed_runtime = false
no_auth = false
log_level = "info"
json_logs = false
read_timeout_secs = 30
write_timeout_secs = 30
idle_timeout_secs = 60
max_body_size = 10485760
enable_websocket = true
max_ws_frame_size = 1048576
max_connections = 1000
request_timeout_secs = 60
compression = true
trusted_proxies = []
EOF
```

## Verification

### Health Checks

```bash
# Daemon health
curl -H "Authorization: Bearer $SYNEVID_LOOPBACK_TOKEN" \
  http://127.0.0.1:3333/api/v1/health

# Repository status
curl -H "Authorization: Bearer $SYNEVID_LOOPBACK_TOKEN" \
  http://127.0.0.1:3333/api/v1/status

# List repositories
curl -H "Authorization: Bearer $SYNEVID_LOOPBACK_TOKEN" \
  http://127.0.0.1:3333/api/v1/repos/list
```

### CLI Verification

```bash
# Basic commands
autopsy status
autopsy baseline --persist
autopsy impact -s "main"
autopsy verify --strict
autopsy doctor
```

### Web UI

```bash
# Start web UI
autopsy ui --port 3000 --open

# Or start daemon with web
autopsy-daemon --port 3333 --web-port 3000
```

## Uninstallation

```bash
# Cargo install
cargo uninstall autopsy-daemon autopsy-cli

# Binary
sudo rm /usr/local/bin/autopsy-daemon /usr/local/bin/autopsy

# Docker
docker rmi synvid/autopsy:0.6.0

# Systemd
sudo systemctl stop autopsy-daemon
sudo systemctl disable autopsy-daemon
sudo rm /etc/systemd/system/autopsy-daemon.service
sudo systemctl daemon-reload

# Data (optional - preserves data)
# rm -rf ~/.autopsy /var/lib/synevid
```

## Troubleshooting

### Common Issues

| Issue | Solution |
|-------|----------|
| `error: linker cc not found` | Install build tools: `apt-get install build-essential` (Linux) / `xcode-select --install` (macOS) |
| `error: linker link.exe not found` | Install Visual Studio Build Tools (Windows) |
| `sqlite3: not found` | Install sqlite3: `apt-get install sqlite3` / `brew install sqlite` |
| `address already in use` | Change port: `autopsy-daemon --port 3334` |
| `permission denied` | Fix permissions: `chmod 700 .autopsy && chmod 600 .autopsy/*` |
| `loopback token invalid` | Regenerate: `rm ~/.autopsy/loopback-token` |

### Logs

```bash
# Daemon logs
journalctl -u autopsy-daemon -f
# or
tail -f ~/.autopsy/daemon.log

# CLI debug
RUST_LOG=debug autopsy baseline --persist
```

## Next Steps

1. Read [Configuration Guide](CONFIGURATION.md)
2. Learn [Basic Usage](../guides/USAGE.md)
3. Set up [CI Integration](../guides/CI_INTEGRATION.md)
4. Configure [IDE Plugins](../guides/IDE_PLUGINS.md)