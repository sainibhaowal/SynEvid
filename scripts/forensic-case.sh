#!/usr/bin/env bash
#
# forensic-case.sh — Design-partner case bundle builder.
#
# Runs SynEvid on a historical change (base ref -> worktree) and bundles
# every artifact a reviewer needs to judge "would this have caught it":
#   baseline + diff + verify --baseline-id + per-file impact (callers
#   profile, top-10) + SARIF, all under <case-dir>/.
#
# Usage:
#   ./scripts/forensic-case.sh <repo-root> <base-ref> <case-dir> [seed ...]
#
#   <repo-root>  working copy containing the AFTER state (uncommitted or
#                committed changes are both fine; the engine never mutates)
#   <base-ref>   git ref for the BEFORE state (materialized in a scratch
#                git worktree; removed on exit)
#   <case-dir>   output directory for the evidence bundle
#   [seed ...]   optional impact seeds (qualified names or file paths);
#                defaults to every changed .ts/.tsx file as a file seed
#
# Requirements: git, cargo-built ./target/debug/autopsy (or --bin), python3.
# Exit codes mirror the verify verdict (0 pass, 2 policy-fail).
#
set -euo pipefail

REPO="${1:?usage: forensic-case.sh <repo-root> <base-ref> <case-dir> [seed ...]}"
BASE_REF="${2:?usage: forensic-case.sh <repo-root> <base-ref> <case-dir> [seed ...]}"
CASE_DIR="${3:?usage: forensic-case.sh <repo-root> <base-ref> <case-dir> [seed ...]}"
shift 3 || true

BIN="${AUTOPSY_BIN:-target/debug/autopsy}"
if [ ! -x "$BIN" ]; then
  echo "building autopsy CLI..." >&2
  cargo build -p autopsy-cli >&2
fi

WORKTREE=""
cleanup() {
  if [ -n "$WORKTREE" ] && [ -d "$WORKTREE" ]; then
    git -C "$REPO" worktree remove --force "$WORKTREE" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

mkdir -p "$CASE_DIR"
WORKTREE="$(mktemp -d)/base"
git -C "$REPO" worktree add --detach "$WORKTREE" "$BASE_REF" >/dev/null

# One case-local store for every step: snapshots and caches accumulate
# here, leaving the repository's own .autopsy directory untouched.
DB="$CASE_DIR/case.db"
CACHE="$CASE_DIR/case-cache"
FLAGS=(--db-path "$DB" --cache-dir "$CACHE")

echo "==> baseline AFTER (worktree $REPO)" >&2
AFTER_SNAP=$("$BIN" --repo-root "$REPO" "${FLAGS[@]}" baseline --format json | tee "$CASE_DIR/after.baseline.json" | python3 -c "import json,sys; print(json.load(sys.stdin)['snapshots'][0]['snapshot_id'])")

echo "==> baseline BEFORE (scratch worktree $BASE_REF)" >&2
BEFORE_SNAP=$("$BIN" --repo-root "$WORKTREE" "${FLAGS[@]}" baseline --format json | tee "$CASE_DIR/before.baseline.json" | python3 -c "import json,sys; print(json.load(sys.stdin)['snapshots'][0]['snapshot_id'])")
echo "before=$BEFORE_SNAP after=$AFTER_SNAP" | tee "$CASE_DIR/snapshots.txt"

echo "==> diff BEFORE -> worktree" >&2
"$BIN" --repo-root "$REPO" "${FLAGS[@]}" diff --before "$BEFORE_SNAP" --format json > "$CASE_DIR/change.diff.json" || true

echo "==> verify AFTER against BEFORE" >&2
set +e
"$BIN" --repo-root "$REPO" "${FLAGS[@]}" verify --baseline-id "$BEFORE_SNAP" --format json > "$CASE_DIR/after.verify.json"
"$BIN" --repo-root "$REPO" "${FLAGS[@]}" verify --baseline-id "$BEFORE_SNAP" --format sarif > "$CASE_DIR/after.verify.sarif"
VERIFY_CODE=$?
set -e
echo "verify exit: $VERIFY_CODE" | tee "$CASE_DIR/verdict.txt"

echo "==> impact (callers profile, top-10)" >&2
SEEDS=("$@")
if [ "${#SEEDS[@]}" -eq 0 ]; then
  mapfile -t SEEDS < <(git -C "$REPO" diff --name-only "$BASE_REF"...HEAD -- '*.ts' '*.tsx' 2>/dev/null || git -C "$REPO" diff --name-only "$BASE_REF" -- '*.ts' '*.tsx' || true)
fi
i=0
for seed in "${SEEDS[@]}"; do
  [ -n "$seed" ] || continue
  i=$((i + 1))
  "$BIN" --repo-root "$REPO" "${FLAGS[@]}" impact -s "$seed" --profile callers --top-k 10 --format json > "$CASE_DIR/impact.$i.json" 2>/dev/null || echo "{\"seed\":\"$seed\",\"error\":\"impact failed\"}" > "$CASE_DIR/impact.$i.json"
done
echo "impact queries: $i" | tee -a "$CASE_DIR/verdict.txt"

python3 - "$CASE_DIR" <<'PYEOF'
import json, sys, pathlib
case = pathlib.Path(sys.argv[1])
print("=" * 64)
print("FORENSIC CASE SUMMARY")
print("=" * 64)
try:
    v = json.load(open(case / "after.verify.json"))
    fails = [f for f in v.get("findings", []) if f.get("status") == "fail"]
    print(f"verify findings: {len(v.get('findings', []))} ({len(fails)} fail)")
    for f in fails:
        print(f"  FAIL [{f['rule_id']}] {f['message'][:120]}")
except Exception as e:
    print(f"verify report unreadable: {e}")
try:
    d = json.load(open(case / "change.diff.json"))
    cs = d.get("change_set", {})
    print(f"diff: {len(cs.get('symbol_deltas', []))} symbol, {len(cs.get('contract_deltas', []))} contract deltas")
    for c in cs.get("contract_deltas", []):
        if isinstance(c, dict) and c.get("is_breaking"):
            print(f"  BREAKING: {c.get('owner')}: {c.get('breaking_reasons')}")
except Exception as e:
    print(f"diff report unreadable: {e}")
print(f"digest: {(v.get('result_digest', '') if 'v' in dir() else '')[:32]}")
print("=" * 64)
PYEOF

exit "$VERIFY_CODE"
