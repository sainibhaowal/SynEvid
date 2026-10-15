//! `autopsy-contracts-distributed`: Pillar 3 of Phase 4.2 — distributed
//! interface contracts.
//!
//! * **Step 37** — OpenAPI 3.2.1 paths and schemas, breaking-change
//!   classification ([`openapi`], [`openapi_diff`]).
//! * **Step 38** — AsyncAPI 3.1.0 channels and messages.
//! * **Step 39** — Pact consumer/provider verification.
//!
//! # The honesty rule that shapes every module here
//! A compatibility checker has an unusually dangerous failure mode. For a
//! linter, "I did not understand this file" produces a visible gap. For a
//! breaking-change detector, "I did not understand this file" produces the
//! answer **"compatible"** — which reads as an approval.
//!
//! So the verdict is computed as:
//!
//! ```text
//! Breaking   if any change classified Breaking was found
//! Compatible only if coverage is Verified
//! Unknown    otherwise
//! ```
//!
//! A definitive breaking change is still reported even when coverage is
//! incomplete, because a positive finding needs no completeness. But a
//! *negative* finding does, and it is never asserted without it
//! (`FR-014`, `FR-030`).
//!
//! # Hard boundaries
//! * **Zero LLM.** Classification is a deterministic comparison of parsed
//!   structures.
//! * **Read-only** (`FR-026`): the crate takes documents by value and returns
//!   values. It opens no files, spawns no processes, and writes nothing.
//! * **Determinism** (`NFR-001`): every map in a canonical structure is a
//!   `BTreeMap`. `serde_json`'s default map is `BTreeMap`-backed, and this
//!   workspace does not enable `preserve_order`, so no document order can reach
//!   a digest.

pub mod document;
pub mod openapi;
pub mod openapi_diff;

pub use document::{DocFormat, DocumentError, RefError, SpecDocument};
pub use openapi::{
    HttpMethod, OpenApiSpec, Operation, ParamLocation, Parameter, PathItem, RequestBody, Response,
    SchemaFacts, parse_openapi,
};
pub use openapi_diff::{
    ChangeKind, CompatibilityVerdict, ContractChange, ContractDelta, Severity, diff_openapi,
};
