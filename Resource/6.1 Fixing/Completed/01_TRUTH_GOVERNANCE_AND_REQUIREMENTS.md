# Phase 0 — Truth, Requirements, and Evidence Governance

## Objective

Create the rules that prevent SynEvid from claiming more than it proved.

## Current findings

- The former fallback metrics, fabricated terminal blast-radius sentence, and Git
  false-PASS path are repaired and covered by the current truth/evidence gate.
- The machine-readable registry and shared evidence envelope are now checked in CI.
- Source-text tests remain supplemental only; behavioral evidence is recorded
  separately in the Phase 0/1 dossier and native runner matrix.

## Requirements

### R0.1 Requirement registry

Every functional and non-functional requirement must have:

- stable ID;
- source PRD and section;
- owner component;
- acceptance test;
- evidence location;
- current state: `PLANNED`, `IMPLEMENTED`, `PARTIAL`, `BLOCKED`, or `VERIFIED`.

### R0.2 Truth states

Define one vocabulary across Rust, JSON, MCP, CLI, UI, reports, and logs:

- `VERIFIED`: evidence was complete for the stated scope;
- `PASS`: no violation was found within verified scope;
- `FAIL`: a verified violation exists;
- `WARN`: a possible issue exists but is not conclusive;
- `PARTIAL`: some relevant input or behavior was not analyzable;
- `UNKNOWN`: the engine cannot prove the claim;
- `ERROR`: the operation could not be completed;
- `UNAVAILABLE`: the required service or artifact was not reachable.

`UNKNOWN`, `PARTIAL`, `ERROR`, and `UNAVAILABLE` must never become `PASS`.

### R0.3 Evidence ledger

For every result, persist a receipt containing the input identities, configuration, parser versions, timing samples, coverage, warnings, output digest, and test reference. Timestamps may be operational metadata but must not enter semantic digests.

### R0.4 No fabricated data

Production code must not contain:

- fake affected counts;
- fake token savings;
- fake latency values;
- demo repository names presented as live data;
- success fallbacks after an error;
- empty-success responses when data is unavailable.

Demo fixtures must be explicitly labeled and isolated from production paths.

## Non-requirements

- No LLM inside correctness crates.
- No probabilistic PASS decision.
- No replacing unknown behavior with a guessed answer.
- No changing ground truth to improve a benchmark.
- No requirement that SynEvid prove arbitrary runtime behavior statically.
- No requirement to preserve an old duplicate UI implementation.

## Required records

- requirements traceability;
- ADR for read-only analysis versus explicit Git mutation;
- ADR for one-daemon multi-client synchronization;
- evidence dossier per release gate;
- defect register with severity, reproducibility, owner, and exit criteria;
- decision log for changed targets.

## Verification

- Run a static scan for hardcoded operational metrics and fixed result strings.
- Run all negative tests for unsupported constructs.
- Verify that every error response has a non-success state.
- Verify that JSON, SARIF, CLI, MCP, and UI expose the same verdict and coverage.
- Review every “PASS” producer in core and adapters.

## Frontend and UX contract

Every panel must show the evidence scope beside the conclusion. A `PASS` must say “within analyzed scope.” An `UNKNOWN` result must show why and how the user can obtain more evidence. Loading, empty, stale, disconnected, and error states are first-class UI states.

## Gate G0

G0 passes only when fabricated output scans are clean, every requirement is traceable, and a reviewer can follow one result from UI to receipt to source bytes and algorithm.

### G0 implementation record

The executable registry is `docs/requirements/registry.yaml`; its completeness
gate is `scripts/validate-requirements.py`. The shared response contract is
`schemas/evidence-envelope.schema.json`; its truth-boundary gate is
`scripts/validate-evidence-contract.py`. Evidence receipt 101 records the local
Windows verification and names the native Linux/macOS CI execution as the only
remaining platform-evidence boundary. No fabricated operational value is used by
the audited production paths, and unsupported or unavailable evidence remains
non-success.

