import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../../..');

test('FR-P3-001: Pure-Rust Git Object Reader Zero Sub-Process Invariant (INV-SANDBOX-003)', () => {
  const gitEngineDir = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src');
  assert.ok(fs.existsSync(gitEngineDir), 'crates/autopsy-git-engine/src must exist');

  const files = fs.readdirSync(gitEngineDir).filter((f) => f.endsWith('.rs'));
  assert.ok(files.length >= 5, 'Must contain at least 5 rust modules');

  for (const f of files) {
    const content = fs.readFileSync(path.join(gitEngineDir, f), 'utf8');
    // Ensure zero subprocess spawning (Command::new, std::process::Command)
    assert.ok(
      !content.includes('std::process::Command'),
      `Forbidden std::process::Command found in ${f}`
    );
    assert.ok(
      !content.includes('Command::new'),
      `Forbidden Command::new found in ${f}`
    );
  }
});

test('FR-P3-002: Virtual Historical Snapshot & Read-Only Invariant (FR-026)', () => {
  const virtualTreePath = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src/virtual_tree.rs');
  assert.ok(fs.existsSync(virtualTreePath), 'virtual_tree.rs must exist');

  const content = fs.readFileSync(virtualTreePath, 'utf8');
  assert.ok(content.includes('VirtualTreeBuilder'), 'Must define VirtualTreeBuilder');
  assert.ok(content.includes('build_frame'), 'Must define build_frame method');
  assert.ok(content.includes('flatten_tree'), 'Must flatten tree directly in memory from packfiles');
});

test('FR-P3-003: Git Time-Travel Galaxy UI Component & Scrubbing Slider', () => {
  const componentPath = path.join(REPO_ROOT, 'apps/web/src/components/GitGalaxySection.tsx');
  assert.ok(fs.existsSync(componentPath), 'GitGalaxySection.tsx must exist');

  const content = fs.readFileSync(componentPath, 'utf8');
  assert.ok(content.includes('export function GitGalaxySection'), 'Must export GitGalaxySection');
  assert.ok(content.includes('Time-Travel Timeline Playhead'), 'Must render timeline scrubbing playhead');
  assert.ok(content.includes('handleSliderChange'), 'Must support slider scrubbing');
  assert.ok(content.includes('Range Mode (A ↔ B)'), 'Must support range comparison mode');
  assert.ok(content.includes('Undo (Cmd+Z)'), 'Must provide one-click undo button');
});

test('FR-P3-004: Semantic Any-to-Any Differ & Drift Report Contract', () => {
  const differPath = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src/differ.rs');
  assert.ok(fs.existsSync(differPath), 'differ.rs must exist');

  const content = fs.readFileSync(differPath, 'utf8');
  assert.ok(content.includes('GitDiffer'), 'Must define GitDiffer');
  assert.ok(content.includes('diff_commits'), 'Must implement diff_commits method');
  assert.ok(content.includes('SemanticDiffReport'), 'Must produce SemanticDiffReport');
  assert.ok(content.includes('blast_radius_score'), 'Must compute blast radius score');
});

test('FR-P3-005: Guarded Mutation Manager & Undo Safety Checkpoints', () => {
  const mutationPath = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src/mutation.rs');
  assert.ok(fs.existsSync(mutationPath), 'mutation.rs must exist');

  const content = fs.readFileSync(mutationPath, 'utf8');
  assert.ok(content.includes('GitMutationManager'), 'Must define GitMutationManager');
  assert.ok(content.includes('create_undo_checkpoint'), 'Must snapshot undo checkpoints under refs/autopsy/undo');
  assert.ok(content.includes('Synevid-Snapshot:'), 'Must append deterministic Synevid trailers');
  assert.ok(content.includes('Synevid-Verdict:'), 'Must append deterministic verdict trailer');
  assert.ok(content.includes('pub fn undo'), 'Must implement atomic rollback via undo');
});

test('FR-P3-006: Web Cockpit Workspace Wiring (Alt+4 / ws-git)', () => {
  const pagePath = path.join(REPO_ROOT, 'apps/web/src/app/page.tsx');
  assert.ok(fs.existsSync(pagePath), 'page.tsx must exist');

  const content = fs.readFileSync(pagePath, 'utf8');
  assert.ok(content.includes('GitGalaxySection'), 'page.tsx must import and mount GitGalaxySection');
  assert.ok(content.includes("activeWorkspace === 'ws-git'"), 'Must handle ws-git workspace');
});

test('FR-P3-007: Autopsy Visual Bisect Engine & Breakpoint Pinpointing', () => {
  const mutationPath = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src/mutation.rs');
  const componentPath = path.join(REPO_ROOT, 'apps/web/src/components/GitGalaxySection.tsx');

  const rustContent = fs.readFileSync(mutationPath, 'utf8');
  assert.ok(rustContent.includes('pub fn bisect'), 'Must implement pub fn bisect');
  assert.ok(rustContent.includes('BisectReport'), 'Must produce BisectReport');

  const uiContent = fs.readFileSync(componentPath, 'utf8');
  assert.ok(uiContent.includes('Autopsy Visual Bisect'), 'Must render Autopsy Visual Bisect tab');
  assert.ok(uiContent.includes('handleRunBisect'), 'Must support interactive bisect execution');
  assert.ok(uiContent.includes('First Breaking Commit:'), 'Must highlight the first breaking commit');
});

test('FR-P3-008: Real-Time Working Tree Status & VS Code File Decorations', () => {
  const statusPath = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src/status.rs');
  const explorerPath = path.join(REPO_ROOT, 'apps/web/src/components/RepoTreeExplorer.tsx');

  assert.ok(fs.existsSync(statusPath), 'status.rs must exist');
  const statusContent = fs.readFileSync(statusPath, 'utf8');
  assert.ok(statusContent.includes('GitStatusScanner'), 'Must define GitStatusScanner');
  assert.ok(statusContent.includes('diff_file'), 'Must implement diff_file method');

  const explorerContent = fs.readFileSync(explorerPath, 'utf8');
  assert.ok(explorerContent.includes('gitStatus'), 'TreeNode must support gitStatus decoration');
  assert.ok(explorerContent.includes('node.gitStatus === \'modified\''), 'Must style modified files');
  assert.ok(explorerContent.includes('node.gitStatus === \'untracked\''), 'Must style untracked files');
  assert.ok(explorerContent.includes('node.gitStatus === \'staged\''), 'Must style staged files');
  assert.ok(explorerContent.includes('node.gitStatus === \'ignored\''), 'Must style ignored files');
});

test('FR-P3-009: Guarded Branch Merge, Semantic Tagging & Checkout/Detach Studio', () => {
  const mutationPath = path.join(REPO_ROOT, 'crates/autopsy-git-engine/src/mutation.rs');
  const componentPath = path.join(REPO_ROOT, 'apps/web/src/components/GitGalaxySection.tsx');

  const rustContent = fs.readFileSync(mutationPath, 'utf8');
  assert.ok(rustContent.includes('pub fn merge'), 'Must implement pub fn merge');
  assert.ok(rustContent.includes('pub fn tag'), 'Must implement pub fn tag');
  assert.ok(rustContent.includes('pub fn checkout'), 'Must implement pub fn checkout');
  assert.ok(rustContent.includes('pub fn revert'), 'Must implement pub fn revert');

  const uiContent = fs.readFileSync(componentPath, 'utf8');
  assert.ok(uiContent.includes('handleMergeBranch'), 'Must support branch merge in UI');
  assert.ok(uiContent.includes('handleCreateTag'), 'Must support semantic tag creation in UI');
  assert.ok(uiContent.includes('handleCheckoutCommit'), 'Must support checkout/detach in UI');
  assert.ok(uiContent.includes('handleRevertCommit'), 'Must support commit revert in UI');
});

test('FR-P3-010: Interactive Pure-Rust Terminal Workbench with Live Command REPL', () => {
  const terminalPath = path.join(REPO_ROOT, 'apps/web/src/components/TerminalWorkbench.tsx');
  const serverPath = path.join(REPO_ROOT, 'apps/synevid-ui/src/server.rs');

  assert.ok(fs.existsSync(terminalPath), 'TerminalWorkbench.tsx must exist');
  const terminalContent = fs.readFileSync(terminalPath, 'utf8');
  assert.ok(terminalContent.includes('● Interactive CLI'), 'Must offer Interactive CLI tab');
  assert.ok(terminalContent.includes('api.execTerminal'), 'Must wire to execTerminal API');
  assert.ok(terminalContent.includes('Quick Run:'), 'Must provide quick run presets');

  const serverContent = fs.readFileSync(serverPath, 'utf8');
  assert.ok(serverContent.includes('/api/v1/terminal/exec'), 'Must route /api/v1/terminal/exec');
  assert.ok(serverContent.includes('handle_terminal_exec'), 'Must implement handle_terminal_exec');
});
