#!/usr/bin/env bash
#
# install.sh — one-line installer, updater, and uninstaller for SynEvid.
#
#   curl -fsSL https://raw.githubusercontent.com/sainibhaowal/SynEvid/main/scripts/install.sh | bash
#
# Installs per-user (never touches system paths), so one installation serves
# every repository on the machine:
#
#   $PREFIX/bin/autopsy                    the engine (default PREFIX=~/.local)
#   $PREFIX/bin/synevid-lsp                the IDE language server
#   $PREFIX/share/synevid/mcp-server/index.js   single-file MCP server (agent bridge)
#   $PREFIX/share/synevid/setup-agent-hosts.py  host-wiring helper
#   $PREFIX/share/synevid/hooks/*.sh            advisory agent hooks
#
# Strategy, in order:
#   1. Download the prebuilt release archive for this OS/arch and verify it
#      against the release's SHA256SUMS.txt (fatal on mismatch).
#   2. Fall back to a source build (Rust 1.90+; rustup is installed if missing).
#   3. Configure PATH via one managed block in ~/.bashrc / ~/.zshrc / ~/.profile.
#   4. Wire detected agent hosts (Claude Code, Cursor, VS Code, Windsurf,
#      opencode, Gemini CLI) to the MCP server — idempotent, backup-first.
#
# Commands:
#   install.sh              install or update to the latest release (idempotent)
#   install.sh --update     same as above (explicit)
#   install.sh --check      report what is installed, offline, changing nothing
#   install.sh --uninstall  remove binaries, share files, PATH block, host entries
#   install.sh --help
#
# Environment:
#   SYNEVID_PREFIX=DIR      install prefix                    (default: ~/.local)
#   SYNEVID_REPO=owner/repo GitHub slug                       (default: sainibhaowal/SynEvid)
#   SYNEVID_NO_PATH=1       do not touch shell startup files
#   SYNEVID_STRICT=1        abort when release checksums cannot be verified
#   AUTOPSY_SKIP_HOSTS=1    skip agent-host wiring
#   AUTOPSY_SKIP_BUILD=1    do not download/build; wire hosts against existing binaries
#   AUTOPSY_SKIP_MCP=1      skip the npm bundling step in a source build
#
# Updating is deliberately explicit: re-run this installer. SynEvid never
# phones home and never auto-updates itself.
#
set -euo pipefail

REPO_SLUG="${SYNEVID_REPO:-sainibhaowal/SynEvid}"
PREFIX="${SYNEVID_PREFIX:-$HOME/.local}"
BIN_DIR="$PREFIX/bin"
SHARE_DIR="$PREFIX/share/synevid"

MARK_BEGIN="# >>> synevid >>>"
MARK_END="# <<< synevid <<<"

log() { printf '%s\n' "$*"; }
warn() { printf '%s\n' "$*" >&2; }

usage() {
  local self="${BASH_SOURCE[0]:-}"
  if [ -n "$self" ] && [ -f "$self" ]; then
    sed -n '2,/^set -euo pipefail/p' "$self" | sed '$d'
  else
    printf 'SynEvid installer — install, --update, --check, --uninstall, --help.\nDocs: https://github.com/%s\n' "$REPO_SLUG"
  fi
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) os="linux" ;;
    Darwin) os="macos" ;;
    MINGW*|MSYS*|CYGWIN*|Windows*) os="windows" ;;
    *) warn "unsupported OS: $os"; return 1 ;;
  esac
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) warn "unsupported arch: $arch"; return 1 ;;
  esac
  printf '%s-%s\n' "$os" "$arch"
}

asset_triple() {
  case "$1" in
    linux-x86_64) printf '%s\n' "x86_64-unknown-linux-gnu" ;;
    macos-arm64) printf '%s\n' "aarch64-apple-darwin" ;;
    macos-x86_64) printf '%s\n' "x86_64-apple-darwin" ;;
    windows-x86_64) printf '%s\n' "x86_64-pc-windows-msvc" ;;
    *) return 1 ;;
  esac
}

asset_ext() {
  if [ "$1" = "windows-x86_64" ]; then printf '%s\n' "zip"; else printf '%s\n' "tar.gz"; fi
}

# --- checksum verification -------------------------------------------------
# Returns 0 verified, 1 mismatch (fatal), 2 checksums unavailable.
verify_checksum() {
  local file="$1" asset="$2" tmp want got
  tmp="$(mktemp -d)"
  if ! curl -fsSL "https://github.com/${REPO_SLUG}/releases/latest/download/SHA256SUMS.txt" \
      -o "$tmp/SHA256SUMS.txt" 2>/dev/null; then
    rm -rf "$tmp"
    return 2
  fi
  want="$(awk -v a="$asset" '$2 == a { print $1 }' "$tmp/SHA256SUMS.txt" | head -1)"
  rm -rf "$tmp"
  if [ -z "$want" ]; then return 2; fi
  if command -v sha256sum >/dev/null 2>&1; then
    got="$(sha256sum "$file" | awk '{print $1}')"
  elif command -v shasum >/dev/null 2>&1; then
    got="$(shasum -a 256 "$file" | awk '{print $1}')"
  elif command -v python3 >/dev/null 2>&1; then
    got="$(python3 -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$file")"
  else
    return 2
  fi
  if [ "$want" = "$got" ]; then return 0; fi
  warn "checksum MISMATCH for $asset (expected $want, got $got)"
  return 1
}

# --- extras: MCP server, host-wiring helper, hooks --------------------------
# Accepts both the release layout (mcp-server/, setup-agent-hosts.py, hooks/)
# and a repository checkout (apps/mcp-server/, scripts/, agents/hooks/).
install_extras_from_dir() {
  local src="$1" cand
  mkdir -p "$SHARE_DIR/mcp-server" "$SHARE_DIR/hooks"
  for cand in "$src/mcp-server/index.js" \
              "$src/apps/mcp-server/dist/index.bundle.js" \
              "$src/apps/mcp-server/dist/index.js"; do
    if [ -f "$cand" ]; then
      cp "$cand" "$SHARE_DIR/mcp-server/index.js"
      log "  MCP server  -> $SHARE_DIR/mcp-server/index.js"
      break
    fi
  done
  for cand in "$src/setup-agent-hosts.py" "$src/scripts/setup-agent-hosts.py"; do
    if [ -f "$cand" ]; then
      cp "$cand" "$SHARE_DIR/setup-agent-hosts.py"
      chmod +x "$SHARE_DIR/setup-agent-hosts.py" 2>/dev/null || true
      break
    fi
  done
  local hook
  for hook in safe-edit.sh autopsy-session.sh; do
    for cand in "$src/hooks/$hook" "$src/agents/hooks/$hook"; do
      if [ -f "$cand" ]; then
        cp "$cand" "$SHARE_DIR/hooks/$hook"
        chmod +x "$SHARE_DIR/hooks/$hook" 2>/dev/null || true
        break
      fi
    done
  done
}

# --- release download -------------------------------------------------------
try_release() {
  local target="$1" tmp ext triple url asset root ok
  tmp="$(mktemp -d)"
  triple="$(asset_triple "$target")" || { rm -rf "$tmp"; return 1; }
  ext="$(asset_ext "$target")"
  asset="autopsy-${triple}.${ext}"
  url="https://github.com/${REPO_SLUG}/releases/latest/download/${asset}"

  command -v curl >/dev/null 2>&1 || { rm -rf "$tmp"; return 1; }
  if ! curl -fsSL "$url" -o "$tmp/$asset" 2>/dev/null; then
    rm -rf "$tmp"
    return 1
  fi

  ok=0
  verify_checksum "$tmp/$asset" "$asset" || ok=$?
  case "$ok" in
    0) log "  checksum verified against SHA256SUMS.txt" ;;
    2)
      if [ "${SYNEVID_STRICT:-0}" = "1" ]; then
        warn "release checksums unavailable and SYNEVID_STRICT=1; aborting"
        rm -rf "$tmp"
        return 2
      fi
      warn "note: release checksums unavailable; installing without verification"
      ;;
    *)
      warn "aborting: release integrity verification failed"
      rm -rf "$tmp"
      return 2
      ;;
  esac

  mkdir -p "$tmp/out"
  if [ "$ext" = "zip" ]; then
    command -v unzip >/dev/null 2>&1 || { rm -rf "$tmp"; return 1; }
    unzip -q -o "$tmp/$asset" -d "$tmp/out"
  else
    tar -xzf "$tmp/$asset" -C "$tmp/out"
  fi

  root="$(dirname "$(find "$tmp/out" -type f \( -name autopsy -o -name autopsy.exe \) | head -1)")"
  if [ -z "$root" ] || [ ! -d "$root" ]; then
    rm -rf "$tmp"
    return 1
  fi

  mkdir -p "$BIN_DIR"
  local name
  for name in autopsy autopsy.exe synevid-lsp synevid-lsp.exe; do
    if [ -f "$root/$name" ]; then
      cp "$root/$name" "$BIN_DIR/$name"
      chmod +x "$BIN_DIR/$name" 2>/dev/null || true
    fi
  done
  install_extras_from_dir "$root"
  rm -rf "$tmp"
  if [ -x "$BIN_DIR/autopsy" ] || [ -x "$BIN_DIR/autopsy.exe" ]; then
    return 0
  fi
  return 1
}

# --- source build fallback --------------------------------------------------
source_build() {
  local src="$1"
  if ! command -v cargo >/dev/null 2>&1; then
    log "Rust toolchain not found; installing via rustup (minimal profile)..."
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >&2
    # shellcheck disable=SC1090,SC1091
    . "$HOME/.cargo/env"
  fi
  (cd "$src" && cargo build --release --locked -p autopsy-cli -p synevid-lsp)
  mkdir -p "$BIN_DIR"
  cp "$src/target/release/autopsy" "$BIN_DIR/autopsy"
  if [ -f "$src/target/release/synevid-lsp" ]; then
    cp "$src/target/release/synevid-lsp" "$BIN_DIR/synevid-lsp"
  fi
  # Single-file MCP server, so agent hosts do not need npm at run time.
  if [ "${AUTOPSY_SKIP_MCP:-0}" != "1" ] \
     && [ ! -f "$src/apps/mcp-server/dist/index.bundle.js" ] \
     && command -v npm >/dev/null 2>&1; then
    if ! (cd "$src/apps/mcp-server" && npm ci --no-audit --no-fund && npm run build:bundle); then
      warn "MCP server bundle build failed; run 'npm ci && npm run build' in apps/mcp-server later"
    fi
  fi
  install_extras_from_dir "$src"
}

# --- PATH management --------------------------------------------------------
path_block() {
  printf '%s\n' "$MARK_BEGIN"
  printf '%s\n' "# Added by the SynEvid installer; remove with: install.sh --uninstall"
  printf 'export PATH="%s:$PATH"\n' "$BIN_DIR"
  printf '%s\n' "$MARK_END"
}

strip_path_block() { # stdin -> stdout without the managed block
  awk -v b="$MARK_BEGIN" -v e="$MARK_END" '
    $0 == b { inb=1; next }
    $0 == e { inb=0; next }
    inb != 1 { print }
  '
}

rc_files() {
  local found=0 rc
  for rc in "$HOME/.bashrc" "$HOME/.zshrc"; do
    if [ -f "$rc" ]; then
      printf '%s\n' "$rc"
      found=1
    fi
  done
  if [ "$found" = 0 ]; then printf '%s\n' "$HOME/.profile"; fi
}

ensure_path_block() {
  local rc tmp
  for rc in $(rc_files); do
    [ -e "$rc" ] || touch "$rc" 2>/dev/null || { warn "  PATH: cannot write $rc"; continue; }
    tmp="$(mktemp)"
    strip_path_block < "$rc" > "$tmp"
    path_block >> "$tmp"
    if cmp -s "$tmp" "$rc"; then
      log "  PATH: already configured in $rc"
    else
      cp "$tmp" "$rc"
      log "  PATH: configured in $rc"
    fi
    rm -f "$tmp"
  done
}

remove_path_block() {
  local rc tmp removed=0
  for rc in $(rc_files); do
    [ -f "$rc" ] || continue
    grep -qF "$MARK_BEGIN" "$rc" || continue
    tmp="$(mktemp)"
    strip_path_block < "$rc" > "$tmp"
    cp "$tmp" "$rc"
    rm -f "$tmp"
    log "  PATH block removed from $rc"
    removed=1
  done
  [ "$removed" = 1 ] || log "  PATH: no managed block found"
}

# --- agent-host wiring ------------------------------------------------------
wire_hosts() {
  if [ "${AUTOPSY_SKIP_HOSTS:-0}" = "1" ]; then
    log "  hosts: skipped (AUTOPSY_SKIP_HOSTS=1)"
    return 0
  fi
  if ! command -v python3 >/dev/null 2>&1; then
    warn "  hosts: python3 not found; wire agent hosts later with setup-agent-hosts.py"
    return 0
  fi
  local helper="$SHARE_DIR/setup-agent-hosts.py"
  if [ ! -f "$helper" ]; then
    warn "  hosts: wiring helper not installed; skipping"
    return 0
  fi
  local args=(--autopsy-bin "$BIN_DIR/autopsy")
  if [ -f "$SHARE_DIR/mcp-server/index.js" ]; then
    args+=(--mcp-server "$SHARE_DIR/mcp-server/index.js")
  fi
  python3 "$helper" "${args[@]}" || warn "host wiring reported issues; re-run setup-agent-hosts.py to retry"
}

unwire_hosts() {
  local helper="$SHARE_DIR/setup-agent-hosts.py"
  if [ -f "$helper" ] && command -v python3 >/dev/null 2>&1; then
    python3 "$helper" --unwire || warn "host unwiring reported issues"
  fi
}

# --- commands ---------------------------------------------------------------
cmd_check() {
  log "SynEvid installation check (prefix: $PREFIX)"
  if [ -x "$BIN_DIR/autopsy" ]; then
    "$BIN_DIR/autopsy" version
  elif [ -x "$BIN_DIR/autopsy.exe" ]; then
    "$BIN_DIR/autopsy.exe" version
  else
    log "  autopsy: not installed (looked in $BIN_DIR)"
  fi
  if [ -x "$BIN_DIR/synevid-lsp" ]; then "$BIN_DIR/synevid-lsp" --version; fi
  if [ -f "$SHARE_DIR/mcp-server/index.js" ]; then
    log "  MCP server:  $SHARE_DIR/mcp-server/index.js"
  else
    log "  MCP server:  not installed"
  fi
  local rc
  for rc in $(rc_files); do
    if [ -f "$rc" ] && grep -qF "$MARK_BEGIN" "$rc"; then
      log "  PATH block:  $rc"
    fi
  done
  log "  update:      re-run this installer (or: install.sh --update)"
  log "  uninstall:   install.sh --uninstall"
}

cmd_uninstall() {
  log "Uninstalling SynEvid from $PREFIX ..."
  unwire_hosts
  remove_path_block
  local removed=0 name
  for name in autopsy autopsy.exe synevid-lsp synevid-lsp.exe; do
    if [ -e "$BIN_DIR/$name" ]; then
      rm -f "$BIN_DIR/$name"
      log "  removed $BIN_DIR/$name"
      removed=1
    fi
  done
  if [ -d "$SHARE_DIR" ]; then
    rm -rf "$SHARE_DIR"
    log "  removed $SHARE_DIR"
    removed=1
  fi
  [ "$removed" = 1 ] || log "  nothing to remove (SynEvid is not installed in $PREFIX)"
  log "Done. Open a new terminal so the PATH change takes effect."
}

# --- main -------------------------------------------------------------------
main() {
  case "${1:-}" in
    "") : ;;
    --update) : ;;
    --help|-h) usage; return 0 ;;
    --check) cmd_check; return 0 ;;
    --uninstall) cmd_uninstall; return 0 ;;
    *)
      warn "unknown option: $1"
      usage >&2
      return 2
      ;;
  esac

  local target=""
  target="$(detect_target 2>/dev/null || true)"
  if [ -n "$target" ]; then
    log "SynEvid installer ($target)"
  else
    log "SynEvid installer (unknown platform; will try a source build)"
  fi

  mkdir -p "$BIN_DIR" "$SHARE_DIR"

  if [ "${AUTOPSY_SKIP_BUILD:-0}" = "1" ]; then
    if [ ! -x "$BIN_DIR/autopsy" ] && [ ! -x "$BIN_DIR/autopsy.exe" ]; then
      warn "AUTOPSY_SKIP_BUILD=1 but no autopsy binary in $BIN_DIR"
      return 1
    fi
    log "skipping download/build (AUTOPSY_SKIP_BUILD=1)"
  else
    local installed=0 rel_rc=0
    if [ -n "$target" ]; then
      try_release "$target" || rel_rc=$?
      case "$rel_rc" in
        0)
          installed=1
          log "installed prebuilt release for $target"
          ;;
        2)
          warn "release integrity verification failed; aborting (no silent fallback)"
          return 1
          ;;
      esac
    fi
    if [ "$installed" != 1 ]; then
      log "no prebuilt release available; building from source (needs Rust; takes a few minutes)..."
      local script_path="${BASH_SOURCE[0]:-}"
      local here=""
      if [ -n "$script_path" ] && [ -f "$script_path" ]; then
        here="$(cd "$(dirname "$script_path")" && pwd)"
      fi
      if [ -n "$here" ] && [ -f "$here/../Cargo.toml" ]; then
        source_build "$(cd "$here/.." && pwd)"
      else
        local tmp
        tmp="$(mktemp -d)"
        trap 'rm -rf "$tmp"' EXIT
        git clone --depth 1 "https://github.com/${REPO_SLUG}.git" "$tmp/SynEvid"
        source_build "$tmp/SynEvid"
      fi
      log "built from source"
    fi
  fi

  case "$target" in
    windows-*)
      log "PATH: on Windows add \"$BIN_DIR\" to PATH manually (Git Bash / PowerShell profile)"
      ;;
    *)
      if [ "${SYNEVID_NO_PATH:-0}" = "1" ]; then
        log "PATH: skipped (SYNEVID_NO_PATH=1)"
      else
        log "configuring PATH..."
        ensure_path_block
      fi
      ;;
  esac

  log "wiring agent hosts (Claude Code, Cursor, VS Code, Windsurf, opencode, Gemini CLI)..."
  wire_hosts

  log "verifying installation..."
  if [ -x "$BIN_DIR/autopsy" ]; then
    "$BIN_DIR/autopsy" version
  elif [ -x "$BIN_DIR/autopsy.exe" ]; then
    "$BIN_DIR/autopsy.exe" version
  else
    warn "verification failed: autopsy binary missing in $BIN_DIR"
    return 1
  fi

  log ""
  log "SynEvid is installed — one installation serves every repository on this machine."
  log "  autopsy     -> $BIN_DIR/autopsy"
  [ -x "$BIN_DIR/synevid-lsp" ] && log "  synevid-lsp -> $BIN_DIR/synevid-lsp"
  [ -f "$SHARE_DIR/mcp-server/index.js" ] && log "  MCP server  -> $SHARE_DIR/mcp-server/index.js"
  log ""
  log "Try it:  cd <any-repo> && autopsy doctor      (open a new terminal first)"
  log "Editors: restart them once; the synevid MCP tools appear to your agent."
  log "Update:  re-run this installer.   Remove: install.sh --uninstall"
}

if [[ "${BASH_SOURCE[0]:-$0}" == "$0" ]]; then
  main "$@"
fi
