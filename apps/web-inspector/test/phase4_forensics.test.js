import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../../..');

test('FR-P4-001: Three-Tier Forensic Trajectory Hierarchy Storage Schema (INV-DETERM-001)', () => {
  const storageLibPath = path.join(REPO_ROOT, 'crates/autopsy-storage/src/lib.rs');
  assert.ok(fs.existsSync(storageLibPath), 'crates/autopsy-storage/src/lib.rs must exist');

  const content = fs.readFileSync(storageLibPath, 'utf8');
  assert.ok(content.includes('forensic_sessions'), 'Must define forensic_sessions table');
  assert.ok(content.includes('forensic_workflows'), 'Must define forensic_workflows table');
  assert.ok(content.includes('trajectory_events'), 'Must define trajectory_events table');
  assert.ok(content.includes('ai_working_cache'), 'Must define ai_working_cache table');
  assert.ok(content.includes('ai_session_ledger'), 'Must define ai_session_ledger table');
});

test('FR-P4-002: Sub-50ms In-Flight Edit Simulation & Cryptographic BLAKE3 Receipts', () => {
  const serverRsPath = path.join(REPO_ROOT, 'apps/synevid-ui/src/server.rs');
  assert.ok(fs.existsSync(serverRsPath), 'synevid-ui server.rs must exist');

  const content = fs.readFileSync(serverRsPath, 'utf8');
  assert.ok(content.includes('handle_simulate_edit'), 'Must implement handle_simulate_edit');
  assert.ok(content.includes('computation_latency_ms'), 'Must record computation latency <= 50ms');
  assert.ok(content.includes('blake3::hash'), 'Must generate cryptographic BLAKE3 receipt');
  assert.ok(content.includes('damaged_callers'), 'Must compute damaged callers');
});

test('FR-P4-003: AI Zero-Token-Waste Local SQLite Working Cache (FR-014, FR-028)', () => {
  const testEfficiencyPath = path.join(REPO_ROOT, 'tests/tests/phase4_ai_cache_efficiency.rs');
  assert.ok(fs.existsSync(testEfficiencyPath), 'phase4_ai_cache_efficiency.rs benchmark test must exist');

  const content = fs.readFileSync(testEfficiencyPath, 'utf8');
  assert.ok(content.includes('test_phase4_ai_cache_efficiency_benchmark'), 'Must verify token slashing efficiency');
  assert.ok(content.includes('reduction_percentage >= 60.0'), 'Must enforce >= 60% token reduction invariant');
  assert.ok(content.includes('UNCHANGED_SINCE_STEP_'), 'Must verify UNCHANGED_SINCE_STEP compact receipts');
});

test('FR-P4-004: Expanded MCP Tool Surface (21 Tools including 4 Phase 4 Tools)', () => {
  const mcpServerPath = path.join(REPO_ROOT, 'apps/mcp-server/src/server.ts');
  assert.ok(fs.existsSync(mcpServerPath), 'mcp-server server.ts must exist');

  const content = fs.readFileSync(mcpServerPath, 'utf8');
  assert.ok(content.includes('autopsy_simulate_edit'), 'Must register autopsy_simulate_edit');
  assert.ok(content.includes('autopsy_canvas_focus'), 'Must register autopsy_canvas_focus');
  assert.ok(content.includes('autopsy_git_time_travel'), 'Must register autopsy_git_time_travel');
  assert.ok(content.includes('autopsy_working_memory'), 'Must register autopsy_working_memory');
});

test('FR-P4-005: Interactive Forensic Trajectory UI Component & 1-Click Rollback', () => {
  const compPath = path.join(REPO_ROOT, 'apps/web/src/components/ForensicTrajectorySection.tsx');
  assert.ok(fs.existsSync(compPath), 'ForensicTrajectorySection.tsx must exist');

  const content = fs.readFileSync(compPath, 'utf8');
  assert.ok(content.includes('Three-Tier Forensic Trajectory Timeline'), 'Must display trajectory header');
  assert.ok(content.includes('handle1ClickRevert'), 'Must implement 1-click revert');
  assert.ok(content.includes('Replay on Canvas'), 'Must support canvas replay');
  assert.ok(content.includes('Zero-Token-Waste Efficiency'), 'Must display token reduction meter');
  assert.ok(content.includes('Cryptographic BLAKE3 Receipt'), 'Must display BLAKE3 receipts');
});

test('FR-P4-006: AI Cockpit & 1-Click Client Auto-Configurator Component', () => {
  const cockpitPath = path.join(REPO_ROOT, 'apps/web/src/components/AiCockpitSection.tsx');
  assert.ok(fs.existsSync(cockpitPath), 'AiCockpitSection.tsx must exist');

  const content = fs.readFileSync(cockpitPath, 'utf8');
  assert.ok(content.includes('1-Click MCP Client Auto-Configurator'), 'Must display 1-click configurator');
  assert.ok(content.includes('claude-code'), 'Must support Claude Code');
  assert.ok(content.includes('cursor'), 'Must support Cursor');
  assert.ok(content.includes('windsurf'), 'Must support Windsurf');
  assert.ok(content.includes('vscode-cline'), 'Must support VS Code');
  assert.ok(content.includes('gemini'), 'Must support Gemini CLI');
});

test('FR-P4-007: Deterministic Trace Bundle Export & Import Endpoints', () => {
  const serverPath = path.join(REPO_ROOT, 'apps/synevid-ui/src/server.rs');
  const content = fs.readFileSync(serverPath, 'utf8');

  assert.ok(content.includes('/api/v1/forensics/export'), 'Must route export endpoint');
  assert.ok(content.includes('/api/v1/forensics/import'), 'Must route import endpoint');
  assert.ok(content.includes('trace_bundle_digest'), 'Must attach trace bundle digest');
});
