# Traceability Matrix

Canonical requirement IDs are maintained in the Requirements Engineering Specification (`02 CA-REQ-001`).

## Foundation Implementation Status

| Requirement ID | Description | Crates / Artifacts | Verification / Tests | Status |
|---|---|---|---|---|
| **Phase 0 Baseline** | Workspace Hardening & Lints | Workspace Cargo, `schemas/*.json` | `cargo fmt --check`, `clippy -D warnings`, `python -m json.tool` | **VERIFIED** |
| **FR-001** | Repository Discovery & Ignore Rules | `autopsy-repo` | `test_scan_repository_deterministic_discovery`, `test_e2e_repo_scan_and_ignores` | **VERIFIED** |
| **FR-002** | Immutable Snapshot Identity (BLAKE3) | `autopsy-domain`, `autopsy-repo` | `test_compute_snapshot_id_golden_100_runs`, `test_e2e_snapshot_id_100_runs_determinism` | **VERIFIED** |
| **FR-011** | Invariant Specification (.autopsy/invariants.yml) | `autopsy-repo` | `test_invariants_config_validation`, `test_e2e_valid_invariants_loading`, `test_e2e_invalid_invariants_diagnostics` | **VERIFIED** |
| **FR-026** | Read-Only Analyzer Engine Safety | `autopsy-repo`, `scripts/` | `test_scan_repository_deterministic_discovery`, `scripts/verify-arch-boundaries.sh` | **VERIFIED** |
| **FR-028** | Deterministic Normalization & Digesting | `autopsy-domain`, `autopsy-repo` | `test_file_set_digest_order_invariance`, `test_canonical_ordering_btreemap`, `test_e2e_compute_snapshot_id_direct_algorithm` | **VERIFIED** |
| **FR-029** | Configuration Validation (autopsy.toml) | `autopsy-repo` | `test_autopsy_config_validation`, `test_e2e_valid_autopsy_toml_loading`, `test_e2e_invalid_config_semantics` | **VERIFIED** |
| **FR-030** | Honest Semantic Coverage States | `autopsy-domain` | `test_coverage_state_serde_roundtrip`, `test_e2e_coverage_state_hard_boundary` | **VERIFIED** |
| **NFR-001** | 100/100 Deterministic Repeatability | `autopsy-domain`, `autopsy-repo`, `tests` | `test_compute_snapshot_id_golden_100_runs`, `test_e2e_snapshot_id_100_runs_determinism`, `scripts/verify-determinism.sh` | **VERIFIED** |
| **FR-003..005** | TS Adapter & Symbol Extraction | `autopsy-adapter-api`, `autopsy-adapter-typescript` | Phase 2 Target | Pending |
| **FR-006..007** | Typed Graph & Semantic Diff | `autopsy-graph`, `autopsy-diff` | Phase 3 Target | Pending |
| **FR-008..014** | Impact, Contracts, Invariants, Evidence | `autopsy-impact`, `autopsy-contracts`, `autopsy-invariants`, `autopsy-evidence` | Phase 4 Target | Pending |
| **FR-015..024** | CLI, SQLite Storage, Reporting | `autopsy-cli`, `autopsy-storage`, `autopsy-report` | Phase 5 Target | Pending |
| **FR-025** | Benchmark Harness & Baselines | `benchmarks/` | Phase 6 Target | Pending |
