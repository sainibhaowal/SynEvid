//! autopsy-temporal: Pillar 1 — Temporal Semantic Graph.
//!
//! Treats code evolution as an append-only event journal of [`TemporalFact`]
//! records with explicit `[valid_from, valid_to)` intervals, rather than a
//! single mutable snapshot.
//!
//! # Hard boundaries honoured here
//! * **Zero sub-process execution** (`INV-SANDBOX-001..004`): git history is
//!   read directly from the object database in [`git`], never via `git log`.
//! * **Read-only** (`FR-026`): nothing in this crate writes to the repository.
//! * **Determinism** (`NFR-001`): every collection is ordered; no wall-clock
//!   time participates in any digest.

pub mod fact;
pub mod git;
pub mod lineage;
pub mod timeline;

pub use fact::{FactId, FactPredicate, JournalStats, TemporalFact};
pub use lineage::{FileEntry, LineageConfig, LineageEvent, LineageEventKind, detect_lineage};
pub use timeline::{
    CommitNode, DEFAULT_MAX_COMMITS, EdgeTriple, GraphSnapshot, SourceExtractor, TemporalError,
    TemporalJournal, Timeline, TreeExtractor,
};
