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
| **FR-008** | Bounded Transitive Impact Query | `autopsy-impact` | `test_forward_and_backward_impact`, `test_budget_truncation`, `test_path_ranking_deterministic`, `test_e2e_bounded_impact_bfs_fwd_bwd_and_truncation` | **VERIFIED** |
| **FR-010** | Contract Delta & Consumer Compatibility | `autopsy-contracts` | `test_callable_breaking_required_param`, `test_interface_breaking_removed_property`, `test_e2e_contracts_compatibility_and_breaking_rules` | **VERIFIED** |
| **FR-011** | Invariant Policy Configuration Loader | `autopsy-invariants` | `test_yaml_config_loader`, `test_e2e_invariants_evaluation_all_rules_and_receipts` | **VERIFIED** |
| **FR-012** | Deterministic Invariant Evaluators | `autopsy-invariants` | `test_forbidden_dep_evaluator_pass_and_fail`, `test_layers_evaluator_pass_and_fail`, `test_no_new_cycles_evaluator`, `test_api_compatibility_evaluator` | **VERIFIED** |
| **FR-013** | Cryptographic Evidence Receipts | `autopsy-evidence` | `test_canonical_ordering_and_hash_stability`, `test_100_runs_determinism`, `test_e2e_invariants_evaluation_all_rules_and_receipts` | **VERIFIED** |
| **FR-014** | Honest Semantic Coverage (`UNKNOWN != PASS`) | `autopsy-invariants`, `autopsy-domain` | `test_unknown_never_collapses_to_pass`, `test_e2e_unknown_never_equals_pass_hard_boundary` | **VERIFIED** |
| **FR-015** | Unified CLI Command Interface (`baseline`, `diff`, `impact`, `verify`, `explain`, `doctor`, `version`) | `autopsy-cli` | `test_cli_version_and_doctor`, `test_cli_diff_and_impact`, `test_cli_baseline_storage_and_cache_roundtrip` | **VERIFIED** |
| **FR-016** | Deterministic Exit Code Protocol (0, 2, 3, 4) | `autopsy-cli` | `test_cli_verify_policy_failure_exit_code`, `test_cli_verify_strict_mode_unsupported_exit_code`, `test_cli_analysis_error_on_invalid_input` | **VERIFIED** |
| **FR-017** | Reporting Engine (Canonical JSON, Human Text, SARIF 2.1.0) | `autopsy-report` | `test_report_canonical_json_and_digest_determinism`, `test_report_human_text_and_sarif`, `test_offline_network_off_isolation` | **VERIFIED** |
| **FR-024** | SQLite Storage with Versioned Migrations & Content-Addressed Cache | `autopsy-storage` | `test_storage_migrations_and_roundtrip`, `test_content_addressed_cache`, `test_cli_baseline_storage_and_cache_roundtrip` | **VERIFIED** |
| **FR-025** | Benchmark Harness & Comparative Baselines (A/B/C) | `benchmarks/`, `tests` | `test_benchmark_corpus_repositories_exist`, `test_benchmark_task_suite_conformance`, `test_benchmark_harness_execution_and_pre_registered_gate` | **VERIFIED** |
| **UC-08** | Benchmarking & Accuracy Auditing | `benchmarks/` | `benchmarks/run_benchmarks.py`, `benchmarks/results/reproduction_report.json` | **VERIFIED** |
| **RQ1-5** | Research Questions Empirical Evaluation | `benchmarks/engine/` | Gate evaluation (+31.73pp recall lift, 56.90% cost cut, 16 new failure classes) | **VERIFIED** |
| **INV-CROSS-001** | Multi-Platform Path Normalization (`/`) | `autopsy-repo` | `test_cross_platform_path_separator_normalization`, `test_e2e_windows_path_separator_canonicalization` | **VERIFIED** |
| **CRLF-LF-001** | Cross-Platform CRLF/LF Line-Ending Determinism | `autopsy-repo` | `test_cross_platform_crlf_lf_determinism`, `test_e2e_crlf_vs_lf_snapshot_identity_invariance` | **VERIFIED** |
| **ATOMIC-CACHE-001** | Cross-Platform Concurrent Cache Write Resilience | `autopsy-storage` | `test_content_addressed_cache`, `test_e2e_storage_concurrent_atomic_write_resilience` | **VERIFIED** |
| **INV-SANDBOX-001** | Filesystem Boundary & Symlink Containment | `autopsy-repo` | `follow_links(false)`, `test_e2e_symlink_escape_containment`, `EVID-PILLAR-C-001` | **VERIFIED** |
| **INV-SANDBOX-002** | Zero Network Socket Execution (FR-028) | `autopsy-cli`, `autopsy-report` | `test_offline_network_off_isolation`, `test_e2e_zero_network_socket_isolation`, `EVID-PILLAR-C-001` | **VERIFIED** |
| **INV-SANDBOX-003** | Path Traversal Sanitization (`..` Prevention) | `autopsy-repo` | `sanitize_relative_path`, `test_e2e_path_traversal_sanitization`, `EVID-PILLAR-C-001` | **VERIFIED** |
| **INV-SANDBOX-004** | Zero Code Execution (FR-026, In-Memory AST) | `autopsy-adapter-typescript` | `test_e2e_zero_code_execution_in_memory_only`, `scripts/verify-arch-boundaries.sh` | **VERIFIED** |
| **CI-RELEASE-001** | Multi-Platform Release Matrix & Binaries | `.github/workflows/` | `ci.yml` (multi-OS), `post-merge.yml`, `benchmarks.yml`, `EVID-PILLAR-B-001` | **VERIFIED** |
| **MCP-SERVER-001** | TypeScript MCP Server SDK v2 (5 Read-Only Tools) | `apps/mcp-server` | `npm run typecheck`, `npm test`, `test_mcp_cli_protocol_conformance` | **VERIFIED** |
| **WEB-INSPECTOR-001** | Visual Forensic Evidence Board & Blast Radius UI | `apps/web-inspector` | Standalone single-page app, `index.html`, JSON artifact loader | **VERIFIED** |
| **AGENT-LIFECYCLE-001** | In-Session Agent Lifecycle Protocol Flow | `tests/tests/` | `test_in_session_agent_lifecycle_protocol_pass_flow`, `test_in_session_agent_lifecycle_fail_repair_reverify_flow` | **VERIFIED** |

