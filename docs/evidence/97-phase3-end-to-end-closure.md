# Production Evidence Receipt 97: Phase 3 End-to-End Correction

**Date:** 2026-11-01  
**Scope:** Phase 3 (v0.4.0) functional closure audit and correction of evidence 79/80.

> Measurement correction (2026-11-03): the reader now uses bounded parallel
> in-memory prefetching for large packs and loose-object directories. The
> indexed packed 5,000-commit acceptance path measured 140–161 ms in the latest
> optimized Windows runs, while the same 5,000 loose-object fixture measured
> 2.53–3.08 s. The packed path is close to the target but still has scheduler
> variance; the loose-object cold-start budget remains open.

## Implemented and verified

- Native Git index v2/v3 reader/writer with checksum validation, real staged blob objects, file staging, selected unified-hunk staging, unstage, and deterministic index ordering.
- Checkout and branch switching update HEAD, branch refs, working-tree files, and .git/index; dirty worktrees are refused with ERR-GIT-204.
- Revert applies an inverse tree patch, creates a real revert commit, updates the working tree/index, records deterministic trailers, and refuses conflicting changes with ERR-GIT-208.
- Undo checkpoints restore the previous commit, symbolic HEAD, index snapshot, and created/deleted branch or tag ref state.
- Semantic historical analysis uses the language adapters in an isolated temporary tree. Historical frames expose symbol nodes, call/reference edges, added/modified/deleted ghost status, coverage, and BLAKE3 result digests.
- Any-to-any diffs now expose symbol deltas, contract deltas, edge deltas, direct symbols, impacted boundary, coverage, verdict, and a deterministic BLAKE3 digest.
- Bisect evaluates semantic contract/coverage transitions through a first-parent range and binary-searches the first failing commit. Commit messages are not used as a failure predicate.
- Line-level blame reports line text, introducing commit, author, sequence, timestamp, blast-radius count, coverage, and a result digest.
- The historical frame is wired into the existing GraphUniverse canvas data contract. The renderer, animation loop, camera behavior, and visual styling were not redesigned.
- PRD path forms are supported: /api/v1/git/time-travel/:oid and /api/v1/git/diff/:base_oid/:head_oid; the blame endpoint is available at /api/v1/git/blame.
- Successful guarded mutations emit forensic trajectory receipts through the existing storage/live-event path.
- Added a real Git fixture that creates branches, a non-fast-forward merge, a tag, packed refs, and packed delta objects; the native reader consumes the resulting repository without invoking Git.
- The real fixture dirties a tracked TypeScript file under Windows `core.autocrlf=true`; native status/staging now compare Git-clean LF bytes, so unchanged CRLF checkouts are not falsely blocked as dirty.
- Virtual historical-frame analysis verifies both working bytes and file mtime remain unchanged before the explicit mutation flow begins.
- Added ADR-0010 to formalize read-only analysis versus the named, explicit POST mutation endpoints.
- Added a source Electron browser test that clicks the real Git Galaxy workspace, enables range comparison, scrubs the real range input to the baseline commit, and verifies the corresponding API requests.

## Verification executed

- cargo test -p autopsy-git-engine --lib -- --nocapture: 7/7 passing, including real-index checkout/revert, semantic diff, non-destructive frame, blame, hunk staging, semantic bisect, and the 5,000-commit regression benchmark.
- cargo test -p autopsy-tests --test phase3_native_git_end_to_end -- --nocapture: passing.
- cargo test -p synevid-ui test_phase3_git_galaxy_and_time_travel_endpoints -- --nocapture: passing in both library and binary test targets.
- apps/web npm test: 19/19 passing, including a live source-Electron browser behavior test for range comparison and timeline scrubbing, historical-frame canvas wiring, PRD routes, honest fallbacks, and blame-to-Galaxy navigation.
- apps/web npm exec tsc -- --noEmit: passing.
- apps/web-inspector `node --test test/phase3_git_galaxy.test.js`: 10/10 Phase 3 contract checks passing.
- Focused clippy for `autopsy-temporal`, `autopsy-git-engine`, and `synevid-ui`: passing with `-D warnings`.
- cargo test -p autopsy-tests --test phase3_real_git_repository -- --nocapture: passing against a real packed repository with packed refs, merge parents, a delta chain, CRLF checkout normalization, mtime-preserving historical analysis, and the full guarded mutation/recovery flow.
- Release performance receipts: `cargo test -p autopsy-git-engine test_phase3_galaxy_5000_commit_acceptance_budget --release -- --nocapture --test-threads=1` measured 140–161 ms for the indexed packed history in the latest runs; the first packed historical frame measured 12.09 ms in the real-repository integration test and 18.14 ms in the semantic unit flow.

## Remaining literal acceptance gap

The functional work is wired end-to-end, but Phase 3 is not honestly marked
100% complete because the PRD does not qualify its 5,000-commit budget by
object storage format and the loose-object cold-start path remains above it:

- The 5,000 loose-object history read measured about 2.5–3.4 seconds on this
  Windows filesystem, above the PRD’s <150 ms target. It remains fully
  supported and has a 30-second regression ceiling.
- The indexed packed history path now passes the literal `<150 ms` assertion
  across the latest three isolated release runs. First historical-frame builds meet the
  <100 ms target in both the native packed integration fixture and the semantic
  unit fixture. The daemon also retains a bounded frame cache for repeat
  requests.

Therefore evidence 79/80 must not be read as current proof of literal Phase 3
completion. No canvas animation or UI design change is required for the
remaining loose-object performance work.

## Follow-up measurement (2026-11-03)

Packed prefetch now visits pack entries in physical offset order before the
existing bounded parallel decode. Three isolated release runs measured 145.03
ms, 136.89 ms, and 147.40 ms; additional runs reached 172.27 ms under normal
Windows scheduling. The 5,000-commit test therefore remains functionally
green but is not a stable proof of the literal `<150 ms` budget. The latest
real-repository historical frame measured 7.13 ms. No canvas implementation was
changed.

## Indexed prefetch closure (2026-11-03)

The packed warm-up now calls the reader with each indexed object's known pack
offset, avoiding a second OID lookup and pack scan. The acceptance assertion is
again the literal `<150 ms` budget. Three isolated release runs passed at
63.42 ms, 78.39 ms, and 88.94 ms. Packed 5,000-commit loading is therefore
verified; the loose-object cold-start path remains the only storage-format
performance limitation.

The refreshed release run also measured 86.48 ms on the same strict packed
acceptance test. The real packed-repository integration and first historical
frame checks remained green. No canvas implementation was changed.
