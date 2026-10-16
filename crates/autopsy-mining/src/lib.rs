//! # autopsy-mining — hidden-coupling discovery (Pillar 5, Step 42)
//!
//! Mines a repository's recent history for paths that change together, and reports the
//! result in a form that cannot be mistaken for a dependency graph.
//!
//! ## The pipeline
//!
//! ```text
//!   GitObjectStore ──▶ history::recent_changes ──▶ HistoryWindow
//!                      read-only, first-parent,        change sets + an exhaustive
//!                      sliding window                  account of what was excluded
//!                                                        │
//!                              cochange::mine ◀──────────┘
//!                                     │
//!                                     ▼
//!                          CoChangeReport
//!                          Support · Confidence (+ Wilson interval) · Lift
//!                          every edge tagged `temporal_co_occurrence_only`
//! ```
//!
//! ## The one thing this crate must never be read as saying
//!
//! Co-change is **temporal co-occurrence**. It is not a dependency, not a call, not a data
//! flow, and not a cause `R32``R33`. Two files changed together in every commit of the
//! window because a code generator writes both, or because one reviewer always touches
//! both, or because a release script bumps both versions — and the numbers are identical in
//! all three cases, and identical again in the case where one genuinely depends on the
//! other.
//!
//! A warning in a document is not a structural guarantee. So the warning is a **field of
//! every edge** ([`cochange::EdgeClaim`], a single-variant enum, which cannot be omitted or
//! defaulted), and the edge kind is the domain's own [`autopsy_domain::EdgeKind`] with the
//! variant [`autopsy_domain::EdgeKind::StatisticalCoChange`], distinct from
//! [`autopsy_domain::EdgeKind::Calls`] at the type level. Code that wants to treat
//! co-change as a dependency has to name it explicitly, and a person reading a filter list
//! sees the word.
//!
//! ## Honest coverage
//!
//! Every ratio in the report is a ratio over a population, and the report always states
//! which population:
//!
//! * `commits_seen == commits_considered + Σ excluded[*]`, an identity a test asserts, so
//!   no commit can be silently dropped from the denominator — dropping one *raises* every
//!   support figure while looking like nothing happened;
//! * mass commits, undecodable trees and timestamp-less commits degrade coverage, because
//!   each of them removes real code changes from the population;
//! * an empty window is `Unknown`, never a report of zero couplings;
//! * a bounded pair set or a bounded history walk is disclosed as a lower bound rather
//!   than presented as complete.
//!
//! ## Boundaries
//!
//! * **Read-only (FR-026)** — git objects are read through `autopsy-temporal`'s pure-Rust
//!   store. No sub-process, no network, no write, and git history is never mutated.
//! * **Deterministic (NFR-001)** — no clock reading. The window is anchored at the newest
//!   commit's timestamp *from the repository*, and what enters the digest is the set of
//!   commit **ids** that were considered, which is content-addressed and timestamp-free.
//!   Ratios are integers in parts per million, never floats.
//! * **Zero LLM** — the whole engine is counting and division.
//! * **Bounded (`INV-SANDBOX`)** — commit count, tree depth, paths per commit, and distinct
//!   pairs are all capped, and reaching a cap is reported.

pub mod cochange;
pub mod history;

pub use cochange::{
    CoChangeEdge, CoChangeOptions, CoChangeReport, DEFAULT_MAX_EDGES, DEFAULT_MAX_PAIRS,
    DEFAULT_MIN_PAIR_COMMITS, EdgeClaim, PPM, WindowSummary, mine, mine_repository, ratio_ppm,
    wilson_interval_ppm,
};
pub use history::{
    ChangeSet, DEFAULT_MAX_COMMITS, DEFAULT_MAX_FILES_PER_COMMIT, DEFAULT_WINDOW_SECONDS,
    ExclusionReason, HistoryOptions, HistoryWindow, MAX_TREE_DEPTH, MiningError, WindowAccounting,
    recent_changes,
};
