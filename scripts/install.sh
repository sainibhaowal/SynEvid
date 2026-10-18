#!/usr/bin/env bash
#
# install.sh — One-line installer for SynEvid (Code Autopsy).
#
#   curl -fsSL https://raw.githubusercontent.com/sainibhaowal/SynEvid/main/scripts/install.sh | bash
#
# Strategy (in order):
#   1. Try a prebuilt release binary for this OS/arch from GitHub releases.
#   2. Fall back to a local source build (needs Rust 1.90+; installed via rustup if missing).
#   3. Optionally wire detected agent hosts (MCP) via setup-agent-hosts.py.
#
# Installs to $SYNEVID_PREFIX (default ~/.local). Never touches system paths.
# Set AUTOPSY_SKIP_HOSTS=1 to skip agent wiring, AUTOPSY_SKIP_BUILD=1 to
# only wire hosts against an existing binary, AUTOPSY_SKIP_MCP=1 to skip the
# MCP server build when installing from a cloned source tree.
#
set -euo pipefail

REPO_SLUG="${SYNEVID_REPO:-sainibhaowal/SynEvid}"
PREFIX="${SYNEVID_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) os="linux" ;;
    Darwin) os="macos" ;;
    MINGW*|MSYS*|CYGWIN*|Windows*) os="windows" ;;
    *) echo "unsupported OS: $os" >&2; return 1 ;;
  esac
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) echo "unsupported arch: $arch" >&2; return 1 ;;
  esac
  echo "${os}-${arch}"
}

try_release() {
  local target="$1" tmp ext url want triple
  tmp="$(mktemp -d)"
  if [ "$target" = "windows-x86_64" ]; then ext="zip"; else ext="tar.gz"; fi
  # Release asset naming: autopsy-<triple>.tar.gz (see post-merge.yml).
  # GitHub's latest-download endpoint keeps this working without API calls.
  case "$target" in
    linux-x86_64) want="x86_64-unknown-linux-gnu" ;;
    macos-arm64) want="aarch64-apple-darwin" ;;
    macos-x86_64) want="x86_64-apple-darwin" ;;
    windows-x86_64) want="x86_64-pc-windows-msvc" ;;
    *) rm -rf "$tmp"; return 1 ;;
  esac
  triple="$want"
  url="https://github.com/${REPO_SLUG}/releases/latest/download/autopsy-${triple}.${ext}"
  if command -v curl >/dev/null 2>&1 && curl -fsSL "$url" -o "$tmp/pkg.$ext" 2>/dev/null; then
    mkdir -p "$BIN_DIR"
    if [ "$ext" = "zip" ]; then
      unzip -q -o "$tmp/pkg.$ext" -d "$tmp/out"
      found="$(find "$tmp/out" -name "autopsy.exe" | head -1)"
      [ -n "$found" ] && cp "$found" "$BIN_DIR/autopsy.exe"
      found_lsp="$(find "$tmp/out" -name "synevid-lsp.exe" | head -1)"
      [ -n "$found_lsp" ] && cp "$found_lsp" "$BIN_DIR/synevid-lsp.exe"
    else
      tar -xzf "$tmp/pkg.$ext" -C "$tmp"
      found="$(find "$tmp" -name autopsy -type f | head -1)"
      [ -n "$found" ] && cp "$found" "$BIN_DIR/autopsy"
      found_lsp="$(find "$tmp" -name synevid-lsp -type f | head -1)"
      [ -n "$found_lsp" ] && cp "$found_lsp" "$BIN_DIR/synevid-lsp"
    fi
    chmod +x "$BIN_DIR/autopsy" "$BIN_DIR/autopsy.exe" \
             "$BIN_DIR/synevid-lsp" "$BIN_DIR/synevid-lsp.exe" 2>/dev/null || true
    if [ -x "$BIN_DIR/autopsy" ] || [ -x "$BIN_DIR/autopsy.exe" ]; then
      rm -rf "$tmp"
      return 0
    fi
  fi
  rm -rf "$tmp"
  return 1
}

source_build() {
  local src_dir="$1"
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust not found; installing via rustup..." >&2
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >&2
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
  fi
  (cd "$src_dir" && cargo build --release --locked -p autopsy-cli -p synevid-lsp)
  mkdir -p "$BIN_DIR"
  cp "$src_dir/target/release/autopsy" "$BIN_DIR/autopsy"
  if [ -f "$src_dir/target/release/synevid-lsp" ]; then
    cp "$src_dir/target/release/synevid-lsp" "$BIN_DIR/synevid-lsp"
  fi
}

main() {
  mkdir -p "$BIN_DIR"
  if [ "${AUTOPSY_SKIP_BUILD:-0}" != "1" ]; then
    if target="$(detect_target)" && try_release "$target"; then
      echo "installed prebuilt autopsy ($target) -> $BIN_DIR/autopsy"
    else
      echo "no prebuilt binary; building from source..." >&2
      here="$(cd "$(dirname "$0")" && pwd)"
      if [ -f "$here/../Cargo.toml" ]; then
        source_build "$(cd "$here/.." && pwd)"
      else
        tmp="$(mktemp -d)"
        trap 'rm -rf "$tmp"' EXIT
        git clone --depth 1 "https://github.com/${REPO_SLUG}.git" "$tmp/SynEvid"
        source_build "$tmp/SynEvid"
        # build the MCP bundle too when node is available
        if command -v npm >/dev/null 2>&1 && [ -z "${AUTOPSY_SKIP_MCP:-}" ]; then
          (cd "$tmp/SynEvid/apps/mcp-server" && npm ci && npm run build)
        fi
      fi
      echo "built autopsy from source -> $BIN_DIR/autopsy"
    fi
  fi

  if [ "${AUTOPSY_SKIP_HOSTS:-0}" != "1" ] && command -v python3 >/dev/null 2>&1; then
    here="$(cd "$(dirname "$0")" && pwd)"
    if [ -f "$here/setup-agent-hosts.py" ]; then
      python3 "$here/setup-agent-hosts.py" --autopsy-bin "$BIN_DIR/autopsy" || true
    fi
  fi

  echo "verifying installation..."
  "$BIN_DIR/autopsy" version
  if [ -x "$BIN_DIR/synevid-lsp" ]; then
    "$BIN_DIR/synevid-lsp" --version
  fi
  echo "add to PATH if needed: export PATH=\"$BIN_DIR:\$PATH\""
}

main "$@"
