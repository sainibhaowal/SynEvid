# ADR-0010: Phase 3 Read-Only Analysis and Explicit Git Mutations

**Status:** Accepted
**Date:** 2026-11-01
**Decision owners:** Synevid core and desktop/web maintainers

## Context

Phase 3 needs two different capabilities. Historical analysis must be safe to
run while a repository is open in an editor, while Git controls such as stage,
commit, checkout, revert, branch, tag, merge, and undo intentionally change
repository state. Treating both capabilities as one mode would violate the
read-only analyzer boundary and make a canvas interaction an implicit source
or history mutation.

## Decision

Synevid has two explicit modes:

1. **Analysis mode is read-only.** Galaxy/history, packed-object reading,
   virtual snapshots, time-travel frames, semantic diffs, blame, status,
   bisect evaluation, and canvas projection may read the working tree and
   `.git`, but must not write source files, `.git/index`, refs, objects,
   timestamps, or sidecar state. Historical snapshots are built in memory or
   isolated temporary analysis storage and never checked out.
2. **Mutation mode is explicit and user initiated.** Only the named POST Git
   control endpoints call `GitMutationManager`:
   `/api/v1/git/stage`, `/unstage`, `/commit`, `/branch`, `/tag`, `/merge`,
   `/checkout`, `/revert`, and `/undo`. Each operation is guarded by the
   clean-worktree/conflict rules, writes real Git state, and returns a recovery
   receipt where applicable.
3. **GET requests and canvas frame changes never mutate.** Selecting a commit,
   scrubbing the timeline, enabling range comparison, or viewing a historical
   frame changes only application state and the in-memory graph data contract.
   It does not change the 3D renderer, animation loop, camera, styling, source
   files, or Git history.
4. **The production core remains subprocess-free.** Real Git repositories are
   used as fixtures by tests where needed, but the native object reader and
   mutation engine do not invoke the Git executable.

## Consequences

- The UI labels Git-writing actions as controls and reports their receipts;
  analysis controls remain safe to use without confirmation of a mutation.
- Branch, checkout, revert, and merge operations may change the working tree
  only after the explicit mutation action succeeds. Undo checkpoints provide
  recovery for the supported mutations.
- Browser tests must verify both sides: timeline/range interactions produce
  read-only API requests, while mutation requests use POST and are isolated
  from canvas rendering code.
- This ADR resolves the Phase 3 requirements conflict: FR-026 governs
  analysis; explicit Git controls are a separate, intentionally authorized
  mutation surface.

## Verification

- `tests/tests/phase3_real_git_repository.rs` exercises a real packed,
  branching, tagged, merged repository, checks mtime-preserving historical
  analysis, and then runs the explicit mutation flow.
- `apps/web/test/phase3_browser.behavior.test.mjs` drives the source-run
  Electron page and verifies slider and range-comparison requests against a
  local fixture API.
- `apps/web-inspector/test/phase3_git_galaxy.test.js` keeps the architectural
  boundary and canvas-preservation regressions visible in the zero-build suite.
