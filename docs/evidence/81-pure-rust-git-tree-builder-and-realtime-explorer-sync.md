# Evidence Receipt 81: Pure-Rust Hierarchical Git Tree Builder & Real-Time Explorer Sync

## Executive Summary
This document records the implementation and verification of the pure-Rust hierarchical Git tree constructor in `crates/autopsy-git-engine` and the real-time Explorer Git status auto-refresh loop in `apps/web`.

## Root Cause Addressed
1. **Tree Absorption on Commit**: Prior to this update, `GitMutationManager::commit` generated valid Git commit objects pointing to the parent commit's existing tree hash. As a consequence, staged file modifications on disk were not incorporated into a newly computed tree object. The working tree scanner (`GitStatusScanner`) therefore continued to observe differences between the disk files and HEAD.
2. **Real-Time Status Propagation**: The VS Code-grade File Explorer (`RepoTreeExplorer.tsx`) required automatic synchronization with the working tree state so that changes from staging, committing, and checkout are immediately visible without manual user interaction.

## Technical Implementation Details
1. **Pure-Rust Tree Constructor (`crates/autopsy-git-engine/src/mutation.rs`)**:
   - Implemented `GitTreeNode` (`Blob(mode, ObjectId)` and `Tree(BTreeMap<String, GitTreeNode>)`).
   - Recursively traverses staged file paths and existing parent tree entries to build a nested directory hierarchy.
   - Writes loose Git blob objects for all staged files using `write_loose_object(&git_dir, "blob", &bytes)`.
   - Serializes Git tree entries using canonical octal modes (`40000` for directories, `100644` for files) and sorts entries using Git's trailing-slash convention.
   - Produces a valid, compressed loose tree object in `.git/objects/xx/yyyy...`.
   - Tested in `test_isolated_working_tree_status_and_diff`: verifies that once staged files are committed, `scan_status` reports zero staged/untracked files and the committed files match HEAD.
2. **Automatic View Switching on Mutation (`apps/web/src/components/GitGalaxySection.tsx`)**:
   - When a commit is created, the UI now automatically calls `setActiveTab('dag')` to transition the user immediately to the Galaxy DAG view where the newly created commit node and release tag badges are rendered.
3. **2500ms Real-Time Explorer Sync Loop (`apps/web/src/components/RepoTreeExplorer.tsx`)**:
   - Installed a 2500ms background polling interval for `fetchGitStatus` alongside dependency triggers.
   - Automatically synchronizes `M` (Modified), `U` (Untracked), `A` (Staged), and `D` (Deleted) badges in real time.

## Verification Telemetry
- `cargo test -p autopsy-git-engine`: 4/4 passed (all tests isolated in `temp_dir()`).
- `cargo test -p synevid-ui`: 4/4 passed.
- `apps/web-inspector`: 30/30 passed.
- `apps/web`: Next.js 16.4.2 production build succeeded with 0 errors.
