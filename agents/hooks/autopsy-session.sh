#!/usr/bin/env sh
# Opt-in Synevid session wiring (Tier 2.5).
#
# A pre-commit hook for AGENT-DRIVEN repositories (not for this engine's own
# CI): when the workspace runs an autopsy repair loop, `session start` opens
# a receipt, evidence steps get recorded explicitly by the agent, and this
# hook closes the latest open session on commit — mirroring the closing
# verdict as its exit code so a failing repair loop blocks the commit and the
# store shows exactly why.
#
# Wiring (in the consumer repo):
#   ln -sf agents/hooks/autopsy-session.sh .git/hooks/pre-commit
#
# The hook is a no-op unless the workspace actually holds an autopsy session
# store: it must never slow down or write into a repository that does not use
# Synevid.
set -u

ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
STORE="$ROOT/.autopsy/sessions.json"
[ -f "$STORE" ] || exit 0

# An open session id — the lexicographically first in the store's canonical
# BTreeMap order — resolved as each `session_id` line followed by its
# `status: open` line. No jq dependency; the deterministic pretty format
# makes the scan a stable two-line window. An agent running parallel loops
# should open exactly one session per workspace; if it opens several, the
# hook closes the first open one and lets the rest surface on inspection.
SID=$(awk '
  /"session_id":/ { id=$2 }
  /"status": "open"/ { if (id != "") { print id; exit } }
' "$STORE" | tr -d '",')
[ -n "$SID" ] || exit 0  # every session already closed: nothing to gate

# Resolve the autopsy binary the same way the engine documents it: explicit
# AUTOPSY_BIN first, then PATH, then the two standard install prefixes, then
# a workspace debug build (for development).
BIN="${AUTOPSY_BIN:-}"
if [ -z "$BIN" ]; then
  BIN=$(command -v autopsy 2>/dev/null || true)
fi
if [ -z "$BIN" ]; then
  for cand in "$HOME/.local/bin/autopsy" "$HOME/.cargo/bin/autopsy" "$(pwd)/target/debug/autopsy"; do
    if [ -x "$cand" ]; then
      BIN="$cand"
      break
    fi
  done
fi
[ -n "$BIN" ] || exit 0  # no engine available: never block on tooling absence

# Close the open session: the verdict (and therefore the exit code) is derived
# from the recorded evidence steps, never from this hook's own opinion.
exec "$BIN" --repo-root "$ROOT" --format json session close --session-id "$SID"
