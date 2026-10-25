#!/usr/bin/env bash
#
# autopsy-task-complete.sh — End-of-task forensic trajectory & verification hook for AI coding agents.
#
# Designed to run automatically when an AI coding agent (Claude Code `Stop` hook,
# Cursor, Windsurf, Gemini CLI, opencode, or terminal agent) finishes a task turn:
#
#   1. Locates the repository root (if inside a git worktree or autopsy workspace).
#   2. Ensures an active repair-loop session exists in `.autopsy/sessions.json`
#      (opening one with `autopsy session start` if none is currently open).
#   3. Captures the post-task baseline snapshot (`autopsy baseline --persist`) and
#      records the step and snapshot ID into the session.
#   4. Evaluates architectural and contract invariants (`autopsy verify`) against
#      the session's starting snapshot (when available) and records the verdict.
#   5. Closes the session (`autopsy session close`), which deterministically writes:
#        - `.autopsy/runs/<session_id>.json` and `.autopsy/runs/<session_id>.md`
#        - `.autopsy/runs/latest.json` and `.autopsy/runs/latest.md`
#      and prints a compact JSON summary for the agent host and human developer.
#
# Contract:
#   - Advisory by default (exits 0 unless `SYNEVID_STRICT_STOP=1` is set, in which
#     case it mirrors the closing verdict exit code so a failing task cannot stop
#     without repair).
#   - 100% deterministic: zero timestamps, BLAKE3 digest chain.
#
set -uo pipefail

BIN="${AUTOPSY_BIN:-}"
if [ -z "$BIN" ]; then
  BIN="$(command -v autopsy 2>/dev/null || true)"
fi
if [ -z "$BIN" ]; then
  for cand in "$HOME/.local/bin/autopsy" "$HOME/.cargo/bin/autopsy" "$(pwd)/target/release/autopsy" "$(pwd)/target/debug/autopsy"; do
    if [ -x "$cand" ]; then
      BIN="$cand"
      break
    fi
  done
fi
[ -n "$BIN" ] && [ -x "$BIN" ] || exit 0

# Drain stdin if invoked as a Claude Code / agent hook (non-blocking)
if [ ! -t 0 ]; then
  cat >/dev/null 2>&1 || true
fi

ROOT="${SYNEVID_REPO_ROOT:-}"
if [ -z "$ROOT" ]; then
  ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
fi
[ -d "$ROOT" ] || exit 0

# Only run in repositories that either opted into SynEvid (`autopsy.toml` or `.autopsy/`)
# or when explicitly requested via `SYNEVID_FORCE_TASK_HOOK=1`.
if [ "${SYNEVID_FORCE_TASK_HOOK:-0}" != "1" ] && [ ! -f "$ROOT/autopsy.toml" ] && [ ! -d "$ROOT/.autopsy" ]; then
  exit 0
fi

STORE="$ROOT/.autopsy/sessions.json"
SID=""
if [ -f "$STORE" ]; then
  SID="$(awk '
    /"session_id":/ { id=$2 }
    /"status": "open"/ { if (id != "") { print id; exit } }
  ' "$STORE" | tr -d '",')"
fi

if [ -z "$SID" ]; then
  START_OUT="$("$BIN" --repo-root "$ROOT" --format json session start 2>/dev/null || true)"
  SID="$(printf '%s' "$START_OUT" | python3 -c "import json,sys; print(json.load(sys.stdin).get('session_id',''))" 2>/dev/null || true)"
fi
[ -n "$SID" ] || exit 0

# Read started_snapshot if recorded on session open
START_SNAP=""
if [ -f "$STORE" ] && command -v python3 >/dev/null 2>&1; then
  START_SNAP="$(python3 -c "
import json, sys
s = json.load(open(sys.argv[1])).get('sessions', {}).get(sys.argv[2], {})
print(s.get('started_snapshot') or '')
" "$STORE" "$SID" 2>/dev/null || true)"
fi

# 1. Capture post-task baseline snapshot
BASE_OUT="$("$BIN" --repo-root "$ROOT" --format json --persist baseline 2>/dev/null)"
BASE_RC=$?
END_SNAP=""
if [ -n "$BASE_OUT" ] && command -v python3 >/dev/null 2>&1; then
  END_SNAP="$(printf '%s' "$BASE_OUT" | python3 -c "
import json, sys
d = json.load(sys.stdin)
snaps = d.get('snapshots') or []
print(snaps[0].get('snapshot_id', '') if snaps else '')
" 2>/dev/null || true)"
fi

REC_ARGS=(--repo-root "$ROOT" --format json session record --session-id "$SID" --exit-code "$BASE_RC")
[ -n "$END_SNAP" ] && REC_ARGS+=(--snapshot-id "$END_SNAP")
REC_ARGS+=(--note "end-of-task baseline snapshot" -- baseline --persist)
"$BIN" "${REC_ARGS[@]}" >/dev/null 2>&1 || true

# 2. Evaluate invariants & contract compatibility
VERIFY_CMD=(verify)
if [ -n "$START_SNAP" ]; then
  VERIFY_CMD+=(--baseline-id "$START_SNAP")
fi
"$BIN" --repo-root "$ROOT" --format json "${VERIFY_CMD[@]}" >/dev/null 2>&1
VERIFY_RC=$?

VREC_ARGS=(--repo-root "$ROOT" --format json session record --session-id "$SID" --exit-code "$VERIFY_RC")
[ -n "$END_SNAP" ] && VREC_ARGS+=(--snapshot-id "$END_SNAP")
VREC_ARGS+=(--note "end-of-task invariant and contract verification" -- "${VERIFY_CMD[@]}")
"$BIN" "${VREC_ARGS[@]}" >/dev/null 2>&1 || true

# 3. Close the session (writes .autopsy/runs/<session_id>.{json,md} and latest.{json,md})
"$BIN" --repo-root "$ROOT" --format json session close --session-id "$SID" >/dev/null 2>&1
CLOSE_RC=$?

if [ -f "$ROOT/.autopsy/runs/latest.json" ] && command -v python3 >/dev/null 2>&1; then
  python3 -c "
import json, sys
r = json.load(open(sys.argv[1]))
print(json.dumps({
    'forensic_task_complete': True,
    'session_id': r.get('session_id'),
    'verdict': r.get('verdict'),
    'evidence_exit_code': r.get('evidence_exit_code'),
    'completed_snapshot': r.get('completed_snapshot'),
    'report_digest': r.get('report_digest'),
    'report_markdown': '.autopsy/runs/latest.md',
    'report_json': '.autopsy/runs/latest.json',
    'next_agent_guidance': r.get('next_agent_guidance'),
}))
" "$ROOT/.autopsy/runs/latest.json"
fi

if [ "${SYNEVID_STRICT_STOP:-0}" = "1" ]; then
  exit "$CLOSE_RC"
fi
exit 0
