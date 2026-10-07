#!/usr/bin/env bash
set -euo pipefail

echo "Executing 100-run snapshot determinism invariant verification..."
cargo test -p autopsy-repo -- test_compute_snapshot_id_golden_100_runs -- --nocapture
cargo test -p autopsy-tests -- test_e2e_snapshot_id_100_runs_determinism -- --nocapture

echo "Determinism verification PASSED: 100/100 runs produced byte-identical digests."
