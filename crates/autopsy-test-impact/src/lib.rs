//! autopsy-test-impact: Pillar 2 — deterministic predictive test impact.
//!
//! Steps 33–36 of the 4.2 sequence:
//! * **Step 33** — test inventory discovery and manifest mapping (this module:
//!   [`inventory`]).
//! * **Step 34** — observed coverage ingestion (LCOV).
//! * **Step 35** — conservative test selection with explicit fallbacks.
//! * **Step 36** — advisory predictive ranking sidecar.
//!
//! # Hard boundaries honoured here
//! * **Zero LLM**: selection is reverse reachability over the call graph.
//! * **Honest coverage** (`FR-014`, `FR-030`): an unresolvable import, an
//!   undetectable framework, or missing coverage data yields
//!   [`CoverageState::Unknown`] or [`CoverageState::Partial`] and forces a
//!   documented fallback — never a silent narrow selection.
//! * **Determinism** (`NFR-001`): every collection is a `BTreeMap`/`BTreeSet`.

pub mod inventory;
pub mod lcov;
pub mod selection;

pub use inventory::{
    ImportSpec, TestCase, TestCaseKind, TestDiscovery, TestFramework, TestInventory, TestSuite,
    discover_inventory, is_test_file,
};
pub use lcov::{CoverageMatrix, LcovError, LineHit, SuiteCoverage, merge_matrices, parse_lcov};
pub use selection::{
    ChangeSetSummary, EdgeMapReachability, MAX_SEEDS, NoReachability, PredictiveRanking,
    RankedTest, Reachability, SelectionError, SelectionFallback, SelectionPlan, SelectionReason,
    select_affected_tests,
};
