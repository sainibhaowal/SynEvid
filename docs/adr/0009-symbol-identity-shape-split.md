# ADR-0009: Symbol identity/shape split (no signature hash in `SymbolId`)

- Status: Accepted
- Date: 2026-10-09
- Scope: `autopsy-symbols`, `autopsy-adapter-typescript`, `autopsy-diff`, `autopsy-contracts`, `autopsy-invariants`, `autopsy-cli`

## Context

Stable identities were derived as `{file}#kind:{name}:{sig_hash}` with a
BLAKE3 signature fingerprint embedded. Any parameter, return-type, or member
edit therefore produced a *new* identity, so the primary forensic events
looked like delete+create pairs:

- `DiffEngine::compute_symbol_deltas` matched on `stable_id`, so signature
  edits surfaced as `Removed` + `Added` and `SignatureModified` was
  unreachable in practice;
- `compute_contract_deltas` and `evaluate_api_compat` join before/after
  contracts by owner id, so breaking signature changes were invisible to
  both diff and invariant verification (the `is_breaking` paths were dead
  code on the main event class).

This defeated FR-007/FR-009/FR-010 while every unit test still passed,
because fixtures never exercised a signature edit end to end.

## Decision

Split identity from shape (single authority: `autopsy-symbols`):

- Identity = canonical file path + kind tag + lexical scope + declared
  name only: `{file}#fn:{name}`, `{file}#method:{Parent}.{name}`,
  `{file}#{class,interface,type,enum,variable}:{name}`, plus deterministic
  `:overload:N` suffixes for same-name collisions (ordered by
  normalized-signature bytes).
- Shape = `Symbol.normalized_signature` and `Contract.inputs/output`,
  compared explicitly by the diff and compatibility engines.
- No line numbers in identities (unchanged); overload ordering is
  content-derived, hence platform-deterministic.

## Consequences

- Signature edits now yield `SignatureModified` + breaking
  `ContractDelta::Modified` with reasons; rename matching by
  (file, kind, signature) is unaffected.
- Stored pre-split identities (with `:hex12` suffixes) do not join new
  ones; snapshots must be re-baselined after upgrade (documented in
  evidence 08). No migration shim: silent dual-format joins would risk
  false forensic links.
- Overload groups share one qualified name; the symbol index keeps every
  overload id, and call resolution treats multi-candidate targets as
  ambiguous (stays `Partial`, never guesses).

## Verification

- `autopsy-symbols` unit tests (identity stability across signature
  edits, overload determinism, format validation).
- `test_identity_stable_across_signature_edit` (adapter).
- `tests/tests/e2e_forensic_closure.rs` (4 tests + 2 impact tests):
  breaking edit -> `SignatureModified` + breaking contract + `verify`
  exit 2; body-only edit -> exit 0.
