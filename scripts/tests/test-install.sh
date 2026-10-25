#!/usr/bin/env bash
# scripts/tests/test-install.sh — offline self-test for the one-time setup path:
# scripts/install.sh (install / update / check / uninstall) and the host wiring
# in scripts/setup-agent-hosts.py.
#
# Everything runs against a fake release served by a fake `curl` shim: no
# network, no Rust toolchain, no real $HOME touched. Run by the pre-commit gate
# and by CI so installer changes cannot land untested.
#
# Usage: bash scripts/tests/test-install.sh
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALL="$REPO_ROOT/scripts/install.sh"
HOST_SETUP="$REPO_ROOT/scripts/setup-agent-hosts.py"

PASSES=0
FAILURES=0

check() { # check <description> <command...>
  local desc="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    PASSES=$((PASSES + 1))
    printf 'ok   %s\n' "$desc"
  else
    FAILURES=$((FAILURES + 1))
    printf 'FAIL %s\n' "$desc"
  fi
}

check_not() { # check_not <description> <command...> — succeeds when command fails
  local desc="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    FAILURES=$((FAILURES + 1))
    printf 'FAIL %s\n' "$desc"
  else
    PASSES=$((PASSES + 1))
    printf 'ok   %s\n' "$desc"
  fi
}

check_eq() { # check_eq <description> <expected> <actual>
  if [ "$2" = "$3" ]; then
    PASSES=$((PASSES + 1))
    printf 'ok   %s\n' "$1"
  else
    FAILURES=$((FAILURES + 1))
    printf 'FAIL %s (expected: %s, actual: %s)\n' "$1" "$2" "$3"
  fi
}

check_contains() { # check_contains <description> <needle> <text>
  case "$3" in
    *"$2"*)
      PASSES=$((PASSES + 1))
      printf 'ok   %s\n' "$1"
      ;;
    *)
      FAILURES=$((FAILURES + 1))
      printf 'FAIL %s (missing: %s)\n' "$1" "$2"
      ;;
  esac
}

check_json() { # check_json <description> <file> <python expression using `d`>
  if python3 -c "import json, sys; d = json.load(open(sys.argv[1])); assert ($3), 'expression false'" "$2" >/dev/null 2>&1; then
    PASSES=$((PASSES + 1))
    printf 'ok   %s\n' "$1"
  else
    FAILURES=$((FAILURES + 1))
    printf 'FAIL %s\n' "$1"
  fi
}

# --- platform gate ----------------------------------------------------------
case "$(uname -s)" in
  Linux) OS=linux ;;
  Darwin) OS=macos ;;
  *) echo "SKIP: installer self-test covers Linux/macOS (found $(uname -s))"; exit 0 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  arm64 | aarch64) ARCH=arm64 ;;
  *) echo "SKIP: unsupported arch $(uname -m)"; exit 0 ;;
esac
case "$OS-$ARCH" in
  linux-x86_64) TRIPLE=x86_64-unknown-linux-gnu ;;
  linux-arm64) TRIPLE=aarch64-unknown-linux-gnu ;;
  macos-arm64) TRIPLE=aarch64-apple-darwin ;;
  macos-x86_64) TRIPLE=x86_64-apple-darwin ;;
  *) echo "SKIP: no release mapping for $OS-$ARCH"; exit 0 ;;
esac

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FAKE_HOME="$TMP/home with space"
FAKE_BIN="$TMP/fakebin"
FAKE_RELEASE="$TMP/release"
PREFIX="$TMP/prefix with space"
PREFIX_BAD="$TMP/prefix-bad"
mkdir -p "$FAKE_HOME/.cursor" "$FAKE_HOME/.config/Code/User" "$FAKE_BIN" "$FAKE_RELEASE"

printf 'export EDITOR=vim\n' > "$FAKE_HOME/.bashrc"
printf '{\n  "mcpServers": {\n    "other": {\n      "command": "x"\n    }\n  }\n}\n' > "$FAKE_HOME/.cursor/mcp.json"
printf '{}\n' > "$FAKE_HOME/.config/Code/User/mcp.json"
printf '{\n  "hooks": {\n    "PreToolUse": [\n      {\n        "matcher": "Bash",\n        "hooks": [\n          {\n            "type": "command",\n            "command": "echo keep"\n          }\n        ]\n      }\n    ]\n  }\n}\n' > "$FAKE_HOME/.claude.json"

# --- fake curl shim ---------------------------------------------------------
cat > "$FAKE_BIN/curl" <<'SHIM'
#!/usr/bin/env bash
# Test shim: serve files from $FAKE_RELEASE_DIR for GitHub release URLs.
set -u
url=""
out=""
while [ $# -gt 0 ]; do
  case "$1" in
    -o)
      out="$2"
      shift 2
      ;;
    http://* | https://*)
      url="$1"
      shift
      ;;
    *) shift ;;
  esac
done
case "$url" in
  https://github.com/*/releases/latest/download/*)
    name="${url##*/}"
    src="${FAKE_RELEASE_DIR:?}/$name"
    [ -f "$src" ] || exit 22
    if [ -n "$out" ]; then cp "$src" "$out"; else cat "$src"; fi
    ;;
  *) exit 22 ;;
esac
SHIM
chmod +x "$FAKE_BIN/curl"

# --- fake release -----------------------------------------------------------
STAGE="$TMP/stage"
ROOT_DIR="autopsy-$TRIPLE"
mkdir -p "$STAGE/$ROOT_DIR/mcp-server" "$STAGE/$ROOT_DIR/hooks"
cat > "$STAGE/$ROOT_DIR/autopsy" <<'BIN'
#!/bin/sh
if [ "${1:-}" = "version" ]; then
  echo "Synevid (Code Autopsy) v9.9.9-fake"
fi
exit 0
BIN
cat > "$STAGE/$ROOT_DIR/synevid-lsp" <<'BIN'
#!/bin/sh
echo "synevid-lsp 9.9.9-fake"
BIN
chmod +x "$STAGE/$ROOT_DIR/autopsy" "$STAGE/$ROOT_DIR/synevid-lsp"
printf 'console.error("fake mcp server");\n' > "$STAGE/$ROOT_DIR/mcp-server/index.js"
cp "$HOST_SETUP" "$STAGE/$ROOT_DIR/setup-agent-hosts.py"
cp "$REPO_ROOT/agents/hooks/safe-edit.sh" "$STAGE/$ROOT_DIR/hooks/safe-edit.sh"
cp "$REPO_ROOT/agents/hooks/autopsy-session.sh" "$STAGE/$ROOT_DIR/hooks/autopsy-session.sh"
cp "$REPO_ROOT/agents/hooks/autopsy-task-complete.sh" "$STAGE/$ROOT_DIR/hooks/autopsy-task-complete.sh"
tar -czf "$FAKE_RELEASE/autopsy-$TRIPLE.tar.gz" -C "$STAGE" "$ROOT_DIR"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}
printf '%s  %s\n' "$(sha256_of "$FAKE_RELEASE/autopsy-$TRIPLE.tar.gz")" "autopsy-$TRIPLE.tar.gz" \
  > "$FAKE_RELEASE/SHA256SUMS.txt"

run_installer() { # run_installer <prefix> [args...]
  local prefix="$1"
  shift
  local release="${FAKE_RELEASE_OVERRIDE:-$FAKE_RELEASE}"
  HOME="$FAKE_HOME" SYNEVID_PREFIX="$prefix" SYNEVID_REPO="fake/repo" \
    PATH="$FAKE_BIN:$PATH" FAKE_RELEASE_DIR="$release" \
    bash "$INSTALL" "$@"
}

# --- 1. syntax + help -------------------------------------------------------
check "install.sh parses (bash -n)" bash -n "$INSTALL"
check "setup-agent-hosts.py parses (py_compile)" \
  python3 -m py_compile "$HOST_SETUP"

out="$(run_installer "$PREFIX" --help 2>&1)"
rc=$?
check_eq "--help exits 0" "0" "$rc"
check_contains "--help documents --uninstall" "--uninstall" "$out"
check_contains "--help documents --update" "--update" "$out"

# --- 2. install from a (fake) prebuilt release ------------------------------
out="$(run_installer "$PREFIX" 2>&1)"
rc=$?
check_eq "install exits 0" "0" "$rc"
check_contains "checksum verification ran" "checksum verified" "$out"
check "engine installed and executable" test -x "$PREFIX/bin/autopsy"
check "LSP installed and executable" test -x "$PREFIX/bin/synevid-lsp"
check "MCP server installed" test -f "$PREFIX/share/synevid/mcp-server/index.js"
check "wiring helper installed" test -f "$PREFIX/share/synevid/setup-agent-hosts.py"
check "safe-edit hook installed" test -x "$PREFIX/share/synevid/hooks/safe-edit.sh"
check "session hook installed" test -x "$PREFIX/share/synevid/hooks/autopsy-session.sh"
check "task-complete forensic hook installed" test -x "$PREFIX/share/synevid/hooks/autopsy-task-complete.sh"
check_contains "installed engine reports version" "9.9.9-fake" \
  "$("$PREFIX/bin/autopsy" version 2>&1)"
check "PATH block added to .bashrc" grep -qF "# >>> synevid >>>" "$FAKE_HOME/.bashrc"
check "existing .bashrc lines preserved" grep -qF "EDITOR=vim" "$FAKE_HOME/.bashrc"
check "PATH block points at the prefix" grep -qF "$PREFIX/bin" "$FAKE_HOME/.bashrc"

check_json "Cursor wired to the installed MCP server" "$FAKE_HOME/.cursor/mcp.json" \
  "d['mcpServers']['synevid']['command'] == 'node' and d['mcpServers']['synevid']['args'][0].endswith('share/synevid/mcp-server/index.js') and d['mcpServers']['synevid']['env']['AUTOPSY_BIN'].endswith('bin/autopsy')"
check_json "Cursor's other entries preserved" "$FAKE_HOME/.cursor/mcp.json" \
  "d['mcpServers']['other']['command'] == 'x'"
check_json "VS Code wired (servers key, stdio type)" "$FAKE_HOME/.config/Code/User/mcp.json" \
  "d['servers']['synevid']['type'] == 'stdio' and d['servers']['synevid']['args'][0].endswith('mcp-server/index.js')"

# Opt-in --hooks wiring with paths containing spaces (shlex.quote verification)
HOME="$FAKE_HOME" python3 "$PREFIX/share/synevid/setup-agent-hosts.py" \
  --autopsy-bin "$PREFIX/bin/autopsy" \
  --mcp-server "$PREFIX/share/synevid/mcp-server/index.js" \
  --hooks >/dev/null 2>&1
check_json "Claude Code safe-edit hook registered and shell-quoted for spaces" "$FAKE_HOME/.claude.json" \
  "len(__import__('shlex').split([h['command'] for e in d['hooks']['PreToolUse'] for h in e.get('hooks', []) if 'safe-edit.sh' in h.get('command', '')][0])) == 2 and __import__('shlex').split([h['command'] for e in d['hooks']['PreToolUse'] for h in e.get('hooks', []) if 'safe-edit.sh' in h.get('command', '')][0])[1].endswith('share/synevid/hooks/safe-edit.sh')"
check_json "Claude Code Stop hook registered for autopsy-task-complete.sh" "$FAKE_HOME/.claude.json" \
  "any('autopsy-task-complete.sh' in str(h.get('command', '')) for e in d['hooks'].get('Stop', []) for h in e.get('hooks', []))"
check_json "Claude Code unrelated PreToolUse hooks preserved" "$FAKE_HOME/.claude.json" \
  "any(h.get('command') == 'echo keep' for e in d['hooks']['PreToolUse'] for h in e.get('hooks', []))"

# --- 3. update run is idempotent --------------------------------------------
out="$(run_installer "$PREFIX" --update 2>&1)"
rc=$?
check_eq "--update exits 0" "0" "$rc"
check "PATH block not duplicated by re-run" \
  bash -c '[ "$(grep -cF "# >>> synevid >>>" "$1")" = "1" ]' _ "$FAKE_HOME/.bashrc"
check_contains "re-run reports PATH already configured" "already configured" "$out"
check_contains "re-run rewires hosts idempotently" "entry already current" "$out"

# --- 4. checksum mismatch aborts without fallback ---------------------------
BAD_RELEASE="$TMP/release-bad"
mkdir -p "$BAD_RELEASE"
cp "$FAKE_RELEASE/autopsy-$TRIPLE.tar.gz" "$BAD_RELEASE/"
printf '0000000000000000000000000000000000000000000000000000000000000000  %s\n' \
  "autopsy-$TRIPLE.tar.gz" > "$BAD_RELEASE/SHA256SUMS.txt"
out="$(FAKE_RELEASE_OVERRIDE="$BAD_RELEASE" run_installer "$PREFIX_BAD" 2>&1)"
rc=$?
check_not "checksum mismatch fails the install" test "$rc" -eq 0
check_contains "mismatch is reported" "integrity" "$out"
check_not "no binary installed after mismatch" test -e "$PREFIX_BAD/bin/autopsy"

# --- 5. --check is read-only and reports the layout --------------------------
before="$(cat "$FAKE_HOME/.bashrc")"
out="$(run_installer "$PREFIX" --check 2>&1)"
rc=$?
check_eq "--check exits 0" "0" "$rc"
check_contains "--check reports the installed version" "9.9.9-fake" "$out"
check_contains "--check reports the MCP server" "mcp-server" "$out"
check_eq "--check leaves .bashrc untouched" "$before" "$(cat "$FAKE_HOME/.bashrc")"

# --- 6. uninstall removes everything the install added -----------------------
out="$(run_installer "$PREFIX" --uninstall 2>&1)"
rc=$?
check_eq "--uninstall exits 0" "0" "$rc"
check_not "engine removed" test -e "$PREFIX/bin/autopsy"
check_not "LSP removed" test -e "$PREFIX/bin/synevid-lsp"
check_not "share dir removed" test -d "$PREFIX/share/synevid"
check_not "PATH block removed from .bashrc" grep -qF "# >>> synevid >>>" "$FAKE_HOME/.bashrc"
check "unrelated .bashrc lines survive uninstall" grep -qF "EDITOR=vim" "$FAKE_HOME/.bashrc"
check_json "Cursor unwired" "$FAKE_HOME/.cursor/mcp.json" \
  "'synevid' not in d['mcpServers'] and d['mcpServers']['other']['command'] == 'x'"
check_json "VS Code unwired" "$FAKE_HOME/.config/Code/User/mcp.json" \
  "'synevid' not in d.get('servers', {})"
check_json "Claude Code safe-edit and Stop hooks unwired by --uninstall and unrelated hook preserved" "$FAKE_HOME/.claude.json" \
  "'synevid' not in d.get('mcpServers', {}) and not any('safe-edit.sh' in str(h.get('command', '')) for e in d.get('hooks', {}).get('PreToolUse', []) for h in e.get('hooks', [])) and not any('autopsy-task-complete.sh' in str(h.get('command', '')) for e in d.get('hooks', {}).get('Stop', []) for h in e.get('hooks', [])) and any(h.get('command') == 'echo keep' for e in d.get('hooks', {}).get('PreToolUse', []) for h in e.get('hooks', []))"

# --- summary ----------------------------------------------------------------
echo
echo "installer self-test: $PASSES passed, $FAILURES failed"
[ "$FAILURES" -eq 0 ]
