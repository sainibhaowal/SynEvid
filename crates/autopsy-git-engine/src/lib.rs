//! # SynEvid Native Git Engine (`crates/autopsy-git-engine`)
//!
//! Provides in-memory, pure-Rust `.git` object reading, commit DAG reconstruction,
//! virtual historical time-travel without disk mutation, any-to-any commit diffing,
//! and guarded in-UI Git control with atomic undo checkpoints under `refs/autopsy/undo/`.
//!
//! Complies strictly with:
//! - `INV-SANDBOX-003`: Zero sub-process execution (pure-Rust object parsing, zero `git` CLI calls).
//! - `FR-026`: Read-only analyzer mode preserves source tree and working tree untouched.
//! - `NFR-001`: 100/100 deterministic floating-point layout and BLAKE3 receipts.

pub mod differ;
pub mod model;
pub mod mutation;
pub mod reader;
pub mod status;
pub mod virtual_tree;

pub use differ::{GitDiffer, GitDiffer as GitDiffEngine};
pub use model::*;
pub use mutation::GitMutationManager;
pub use reader::{GitGalaxyReader, GitGalaxyReader as GitReader};
pub use status::GitStatusScanner;
pub use virtual_tree::VirtualTreeBuilder;

#[cfg(test)]
mod tests;
