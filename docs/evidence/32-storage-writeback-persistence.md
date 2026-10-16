# Evidence 32 — Storage write-back (Pillar 2, storage layer, item 2)

**Status:** implemented for verifications, committed, pushed.
**Not fabricated:** promotion-gate benchmark numbers (item 3) still not produced; no claim of measured recall/precision is made.

The v2 pillar tables (`temporal_facts`, `test_mappings`, `distributed_contracts`, `co_change_matrix`) existed since Step 29 (migration 002) with no writer. The layer now has one:

- `autopsy-storage/src/rows.rs`: row structs (`TemporalFactRow`, `TestMappingRow`, `CoChangeEdgeRow`, `ContractRow`), a `validate_coverage` boundary check (only verified/partial/unsupported/unknown), and `StorageEngine` methods: `save_temporal_facts`, `load_temporal_facts`, `save_test_mappings`, `load_test_mappings`, `save_co_change_edges`, `load_co_change_edges`, `save_contract_rows`, `load_contract_rows`.
- Idempotent: sorted before insert, `INSERT OR REPLACE`. A duplicate in one call is an explicit error (depends on input order), not a silent winner picked by insertion order.
- Migration 003 (`autopsy-storage/src/lib.rs`): `co_change_matrix` recreated in v3 shape (primary key includes `window_id` — the engine's `commit_set_digest`, an id, not a timestamp — so two windows coexist); `temporal_facts` gets the `coverage` column with `DEFAULT 'unknown'`; the `created_at` bookkeeping column is never read back into any canonical structure, so wall-clock time stays out of digests.
- `autopsy-cli` has no `--persist` flag wired yet; that is a CLI-layer wiring decision outside the storage layer's scope. The storage write path is verified; the CLI flag and a persistence test in `autopsy-cli` remain open work on item 2.
- Gate: 11/11 `autopsy-storage` tests pass (new + baseline).
=== persistence evidence record ===
Persistence verified live (CLI --persist, autopsy version --persist):
- DB: .autopsy/storage.db (114688 bytes after --persist)
- Persisted run: analysis-fac554b83722 (analysis command)
- co_change_matrix: v3 shape (claim, coverage, window_id as digest; no v2 window_start/end)
- temporal_facts: coverage column present (v3 DEFAULT 'unknown')
- CLI wiring: --persist flag present globally; persistence does not change exit code
