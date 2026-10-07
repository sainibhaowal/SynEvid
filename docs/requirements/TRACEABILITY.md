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
| **FR-003** | Versioned Language Adapter Interface | `autopsy-adapter-api` | `test_adapter_capabilities_honesty`, `test_e2e_ts_adapter_conformance_and_capabilities` | **VERIFIED** |
| **FR-004** | Syntax Parsing & Diagnostics Handling | `autopsy-adapter-typescript` | `test_syntax_error_diagnostics_without_crashing`, `test_discover_extensions` | **VERIFIED** |
| **FR-005** | Stable Symbol IDs Resilient to Line Shifts | `autopsy-adapter-typescript` | `test_stable_symbol_id_resilience_to_line_shifts`, `test_e2e_ts_real_repo_parsing_and_symbol_extraction` | **VERIFIED** |
| **FR-009** | Normalized Contract Extraction | `autopsy-adapter-typescript` | `test_extract_contracts`, `test_e2e_ts_real_repo_parsing_and_symbol_extraction` | **VERIFIED** |
| **FR-006** | Typed Multigraph with Edge Provenance | `autopsy-graph` | `test_multigraph_support`, `test_bounded_reachability_forward_and_backward`, `test_cycle_detection_via_tarjan_scc`, `test_e2e_dependency_multigraph_cycles_and_reachability` | **VERIFIED** |
| **FR-007** | Semantic Diff Engine & Ambiguous Rename Rule | `autopsy-diff` | `test_unambiguous_file_rename`, `test_ambiguous_file_rename_stays_add_and_delete`, `test_contract_breaking_change_detection`, `test_e2e_semantic_diff_unambiguous_and_ambiguous_renames`, `test_e2e_golden_diff_fixture_exact_json_match` | **VERIFIED** |
| **FR-008..014** | Impact, Invariants, Evidence Engine | `autopsy-impact`, `autopsy-contracts`, `autopsy-invariants`, `autopsy-evidence` | Phase 4 Target | Pending |
| **FR-015..024** | CLI, SQLite Storage, Reporting | `autopsy-cli`, `autopsy-storage`, `autopsy-report` | Phase 5 Target | Pending |
| **FR-025** | Benchmark Harness & Baselines | `benchmarks/` | Phase 6 Target | Pending |

