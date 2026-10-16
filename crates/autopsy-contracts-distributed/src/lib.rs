//! `autopsy-contracts-distributed`: Pillar 3 of Phase 4.2 — distributed
//! interface contracts.
//!
//! * **Step 37** — OpenAPI 3.2.1 paths and schemas, breaking-change
//!   classification ([`openapi`], [`openapi_diff`]).
//! * **Step 38** — AsyncAPI 3.1.0 channels and messages, and
//!   producer-to-consumer payload compatibility ([`asyncapi`],
//!   [`asyncapi_diff`]).
//! * **Step 39** — Pact consumer/provider verification. *(not implemented)*
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

pub mod asyncapi;
pub mod asyncapi_diff;
pub mod change;
pub mod document;
pub mod openapi;
pub mod openapi_diff;
pub mod schema;

pub use asyncapi::{
    Action, AsyncApiError, AsyncApiSpec, AsyncOperation, Channel, Message, PayloadDirection,
    parse_asyncapi,
};
pub use asyncapi_diff::{
    FlowCheck, FlowFinding, FlowMismatch, FlowReport, UncheckedFlow, diff_asyncapi,
    findings_by_channel, severity_for, verify_flows,
};
pub use change::{
    ChangeKind, CompatibilityVerdict, ContractChange, ContractDelta, Severity, VersionChange,
};
pub use document::{DocFormat, DocumentError, RefError, SpecDocument};
pub use openapi::{
    HttpMethod, OpenApiSpec, Operation, ParamLocation, Parameter, PathItem, RequestBody, Response,
    parse_openapi,
};
pub use openapi_diff::{diff_openapi, parameter_identities};
pub use schema::{SchemaFacts, SchemaReader};
