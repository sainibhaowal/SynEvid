# Evidence Document 45: Isolation of Experimental Research Crates

## Executive Summary
This production receipt documents the physical isolation of speculative research crates (`autopsy-behavior-research`, `autopsy-incidents`, `autopsy-hidden-coupling`, `autopsy-schema-registry`) into a dedicated `experimental/` workspace directory. This eliminates dilution of the core SynEvid v0.1 production verification message, narrows the core test and maintenance surface, and creates a clean architectural boundary between production-grade offline deterministic verification and exploratory research prototypes.

---

## 1. Motivation & Architectural Rationale

* **Product Message Clarity (v0.1 Focus):** SynEvid's core identity is an offline, deterministic software verification and change-impact engine. Sitting 4 speculative research crates directly alongside core crates (`autopsy-diff`, `autopsy-impact`, `autopsy-invariants`, `autopsy-contracts`) diluted the production readiness signal for adopters and auditors.
* **Decoupled Maintenance & Test Surface:** The core verification loop (`autopsy verify`, `autopsy impact`, `autopsy baseline`) does not depend on these four exploratory modules. Isolating them inside `experimental/` keeps the `crates/` directory dedicated 100% to production code.
* **Sandbox & Invariant Preservation:** The `experimental/` crates continue to adhere to core architectural invariants: Zero LLM, Zero Network Sockets, and Zero Subprocess Execution.

---

## 2. Structural Changes

| Previous Path | New Path | Purpose |
|---|---|---|
| `crates/autopsy-behavior-research` | `experimental/autopsy-behavior-research` | Behavioral differential comparator prototype (FR-ADV-023/024) |
| `crates/autopsy-incidents` | `experimental/autopsy-incidents` | Incident correlation candidate ranking prototype (FR-ADV-025/026) |
| `crates/autopsy-hidden-coupling` | `experimental/autopsy-hidden-coupling` | Text-based env/config/queue coupling extractor prototype (FR-ADV-021) |
| `crates/autopsy-schema-registry` | `experimental/autopsy-schema-registry` | Pluggable schema-registry policy prototype (FR-ADV-015) |

### Workspace Configuration Updates
- **Root `Cargo.toml`:** Updated workspace members to clearly separate core crates from `experimental/*`.
- **Crate Manifests:** Updated `autopsy-domain` path references from `../autopsy-domain` to `../../crates/autopsy-domain`.
- **Boundary Verification (`scripts/verify-arch-boundaries.sh`):** Expanded automated boundary scanning across both `crates/` and `experimental/` to ensure zero LLM, zero networking, and zero subprocess execution across all modules.

---

## 3. Verification Telemetry

* `cargo check --workspace --all-targets`: **PASSED (0 errors, 0 warnings)**
* `cargo test -p autopsy-behavior-research -p autopsy-incidents -p autopsy-hidden-coupling -p autopsy-schema-registry`: **13/13 passed**
* `cargo fmt --all -- --check`: **PASSED**
* `./scripts/verify-arch-boundaries.sh`: **PASSED**

---

## 4. Conclusion
The core `crates/` directory now represents a laser-focused, production-grade engine, while experimental and exploratory research crates are cleanly isolated under `experimental/` without loss of functionality or test coverage.
