#!/usr/bin/env bash
#
# verify-perf-tiers.sh — Performance regression tripwire (4.1 Ch.7 scheduled).
#
# Measures wall-clock baseline + impact latency on the bundled corpus repos
# and fails when any tier budget is exceeded. Budgets carry wide headroom
# (debug builds on shared CI runners); tighten only from measured data.
#
# Tier budgets (per repo, single commands):
#   T1 small corpus (<= 15 files): baseline <= 30s, impact <= 30s
#   T2 benchmark corpus (repo-api..repo-dead-call): baseline <= 60s each
#
# This is a smoke tripwire, not a benchmark: it guards against
# order-of-magnitude regressions (accidental O(V^2), unbounded reads).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${AUTOPSY_BIN:-$ROOT/target/debug/autopsy}"
if [ ! -x "$BIN" ]; then
  echo "building autopsy CLI for perf measurement..." >&2
  cargo build -p autopsy-cli >&2
fi

# Allow override for slow/shared runners: PERF_BUDGET_SCALE=3 ./scripts/verify-perf-tiers.sh
SCALE="${PERF_BUDGET_SCALE:-1}"

check_budget() {
  local label="$1"; local elapsed_ms="$2"; local budget_ms="$3"
  local scaled=$((budget_ms * SCALE))
  if [ "$elapsed_ms" -gt "$scaled" ]; then
    echo "FAIL: $label took ${elapsed_ms}ms (budget ${scaled}ms)" >&2
    return 1
  fi
  echo "ok: $label ${elapsed_ms}ms <= ${scaled}ms"
}

measure_ms() {
  local start end
  start=$(date +%s%3N)
  "$@" >/dev/null 2>&1
  end=$(date +%s%3N)
  echo $((end - start))
}

fail=0
for repo in "$ROOT"/benchmarks/corpus/*/; do
  [ -d "$repo" ] || continue
  name="$(basename "$repo")"
  # Isolated store: corpus checkouts are never mutated.
  tmp="$(mktemp -d)"
  ms=$(measure_ms "$BIN" --repo-root "$repo" --db-path "$tmp/db" --cache-dir "$tmp/cache" baseline)
  check_budget "$name baseline" "$ms" 60000 || fail=1
  seed="$("$BIN" --repo-root "$repo" --db-path "$tmp/db" --cache-dir "$tmp/cache" symbols --limit 1 --format json 2>/dev/null | python3 -c "import json,sys; d=json.load(sys.stdin); print(d['symbols'][0]['stable_id'] if d['symbols'] else '')")"
  if [ -n "$seed" ]; then
    ms=$(measure_ms "$BIN" --repo-root "$repo" --db-path "$tmp/db" --cache-dir "$tmp/cache" impact -s "$seed" --profile callers --top-k 10)
    check_budget "$name impact(callers)" "$ms" 30000 || fail=1
  fi
  rm -rf "$tmp"
done

if [ "$fail" -ne 0 ]; then
  echo "PERF TIERS FAILED" >&2
  exit 1
fi
echo "PERF TIERS PASSED"
