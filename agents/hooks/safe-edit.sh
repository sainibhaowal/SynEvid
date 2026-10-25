#!/usr/bin/env bash
#
# safe-edit.sh — Pre-edit blast-radius advisor hook for coding agents.
#
# Reads a JSON event from stdin: {"tool": "Edit|Write", "path": "/abs/file"}.
# For analyzed languages (.ts/.tsx/.js/.jsx/.py/.go/.rs), runs a fast caller-closure
# impact query and prints machine-readable guidance to stdout.
#
# Contract (fail-open by design):
#   - ALWAYS exits 0 (advisory only; never blocks an edit).
#   - Prints NOTHING when the file is outside analyzed languages, the CLI
#     is missing, or analysis fails (agent proceeds normally).
#   - Prints a compact JSON verdict otherwise:
#       {"advisory": true, "top": [...up to 5 impacted...], "total": N,
#        "digest": "<result_digest>", "command": "<rerun command>"}
#
# The digest lets the agent (or human) re-verify: identical digest on
# rerun proves the evidence came from the engine, not from model text.
#
set -uo pipefail

BIN="${AUTOPSY_BIN:-}"
if [ -z "$BIN" ]; then
  for cand in "$HOME/.local/bin/autopsy" "$HOME/.cargo/bin/autopsy"; do
    if [ -x "$cand" ]; then BIN="$cand"; break; fi
  done
fi
[ -n "$BIN" ] && [ -x "$BIN" ] || exit 0

INPUT="$(cat)"
FILE="$(printf '%s' "$INPUT" | python3 -c "import json,sys; print(json.load(sys.stdin).get('path',''))" 2>/dev/null || true)"
[ -n "$FILE" ] || exit 0

case "$FILE" in
  *.ts|*.tsx|*.js|*.jsx|*.py|*.go|*.rs) ;;
  *) exit 0 ;;
esac

# Repository root: nearest ancestor containing autopsy.toml, else file dir.
DIR="$(dirname "$FILE")"
REPO="$DIR"
while [ "$REPO" != "/" ] && [ ! -f "$REPO/autopsy.toml" ]; do
  REPO="$(dirname "$REPO")"
done
[ -f "$REPO/autopsy.toml" ] || REPO="$DIR"

# Engine seeds are repository-relative: relativize the absolute path.
if [ "$FILE" = "$REPO" ]; then
  REL="$FILE"
else
  REL="${FILE#"$REPO"/}"
fi
[ -n "$REL" ] || exit 0

OUT="$("$BIN" --repo-root "$REPO" impact -s "$REL" --profile callers --top-k 5 --format json 2>/dev/null)" || exit 0
printf '%s' "$OUT" | python3 -c "
import json, shlex, sys
d = json.load(sys.stdin)
imp = d.get('impact', {}) or {}
top = imp.get('ranked_top', []) or imp.get('impacted_entities', [])
total = imp.get('total_impacted', len(imp.get('impacted_entities', [])))
print(json.dumps({
    'advisory': True,
    'top': top[:5],
    'total': total,
    'digest': d.get('result_digest', ''),
    'command': 'autopsy impact -s ' + shlex.quote(sys.argv[1]) + ' --profile callers --top-k 5',
}))
" "$REL"
