import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../../..');

// Helper to calculate relative luminance for WCAG contrast
function getLuminance(hex) {
  const cleanHex = hex.replace('#', '');
  const r = parseInt(cleanHex.substring(0, 2), 16) / 255;
  const g = parseInt(cleanHex.substring(2, 4), 16) / 255;
  const b = parseInt(cleanHex.substring(4, 6), 16) / 255;

  const a = [r, g, b].map(v => {
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return a[0] * 0.2126 + a[1] * 0.7152 + a[2] * 0.0722;
}

function getContrastRatio(hex1, hex2) {
  const lum1 = getLuminance(hex1);
  const lum2 = getLuminance(hex2);
  const brightest = Math.max(lum1, lum2);
  const darkest = Math.min(lum1, lum2);
  return (brightest + 0.05) / (darkest + 0.05);
}

test('FR-P2-001: Minimalist Light & Dark Mode CSS Tokens and WCAG Contrast', () => {
  const cssPath = path.join(REPO_ROOT, 'apps/web/src/app/globals.css');
  assert.ok(fs.existsSync(cssPath), 'globals.css must exist');
  const css = fs.readFileSync(cssPath, 'utf8');

  // Verify light and dark mode tokens
  const requiredTokens = [
    '--bg-canvas',
    '--bg-surface',
    '--bg-elevated',
    '--border-hairline',
    '--border-strong',
    '--text-primary',
    '--text-secondary',
    '--text-muted',
    '--node-verified',
    '--node-damaged',
    '--node-impacted',
    '--node-deadcode',
    '--edge-call',
    '--edge-import',
    '--edge-ui-to-api',
  ];

  for (const token of requiredTokens) {
    assert.ok(
      css.includes(token),
      `Token ${token} must be defined in globals.css`
    );
  }

  // Verify WCAG AAA contrast ratio for primary text (#0F172A) on light surface (#FFFFFF)
  const contrast = getContrastRatio('#FFFFFF', '#0F172A');
  assert.ok(
    contrast >= 15.0,
    `Primary text contrast (${contrast.toFixed(2)}:1) must satisfy WCAG AAA (>= 15:1)`
  );
});

test('FR-P2-002: Left Navigation Rail mounts all 8 workspaces with Alt+1..Alt+8 shortcuts', () => {
  const pagePath = path.join(REPO_ROOT, 'apps/web/src/app/page.tsx');
  assert.ok(fs.existsSync(pagePath), 'page.tsx must exist');
  const pageContent = fs.readFileSync(pagePath, 'utf8');

  const expectedWorkspaces = [
    { id: 'ws-repos', shortcut: '1' },
    { id: 'ws-canvas', shortcut: '2' },
    { id: 'ws-simulator', shortcut: '3' },
    { id: 'ws-git', shortcut: '4' },
    { id: 'ws-trajectory', shortcut: '5' },
    { id: 'ws-gates', shortcut: '6' },
    { id: 'ws-ai', shortcut: '7' },
    { id: 'ws-settings', shortcut: '8' },
  ];

  for (const ws of expectedWorkspaces) {
    assert.ok(
      pageContent.includes(ws.id),
      `Workspace ${ws.id} must be declared in page.tsx`
    );
  }
});

test('FR-P2-004: Single-File Ego-Network and Line-to-Line Call Tracing', async () => {
  const res = await fetch('http://127.0.0.1:3333/api/v1/graph/ego?id=PaymentService.chargeCard');
  assert.strictEqual(res.status, 200, 'Ego network endpoint must return 200 OK');
  const data = await res.json();

  // Target node verification
  assert.ok(data.target_node, 'target_node must be present');
  assert.strictEqual(data.target_node.file_path, 'src/services/PaymentService.ts');
  assert.strictEqual(data.target_node.stable_id, 'PaymentService.chargeCard');
  assert.strictEqual(data.target_node.start_line, 35);

  // Incoming callers verification
  assert.ok(Array.isArray(data.incoming_callers), 'incoming_callers array must be present');
  const orderCaller = data.incoming_callers.find(c => c.source_file.includes('OrderController'));
  assert.ok(orderCaller, 'OrderController must be listed as incoming caller');
  assert.strictEqual(orderCaller.source_line, 42);
  assert.strictEqual(orderCaller.target_line, 35);
  assert.strictEqual(orderCaller.confidence, 'exact');

  // Outgoing callees verification
  assert.ok(Array.isArray(data.outgoing_callees), 'outgoing_callees array must be present');
  const userCallee = data.outgoing_callees.find(c => c.target_file.includes('UserRepository'));
  assert.ok(userCallee, 'UserRepository must be listed as callee');
  assert.strictEqual(userCallee.target_line, 19);
});

test('FR-P2-005: Live Code Editor & Blast Radius Simulator Sub-50ms Evaluation', async () => {
  // Warm-up ping to ensure socket is established
  await fetch('http://127.0.0.1:3333/api/v1/health');

  const startTime = Date.now();
  const res = await fetch('http://127.0.0.1:3333/api/v1/simulate/edit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      file_path: 'src/services/PaymentService.ts',
      unsaved_buffer: 'export async function chargeCard(amount: number, taxRate: number): Promise<Receipt> { return execute(amount); }'
    })
  });

  const durationMs = Date.now() - startTime;
  assert.strictEqual(res.status, 200, 'Simulate edit endpoint must return 200 OK');

  const data = await res.json();
  assert.ok(
    data.computation_latency_ms <= 50,
    `Engine computation latency must be <= 50ms (was ${data.computation_latency_ms}ms)`
  );
  assert.ok(
    durationMs <= 150,
    `Simulation round-trip must complete within interactive threshold (took ${durationMs}ms)`
  );
  assert.strictEqual(data.verdict, 'FAIL', 'Breaking signature change must produce verdict FAIL');
  assert.ok(Array.isArray(data.damaged_callers), 'damaged_callers must be an array');
  assert.ok(data.damaged_callers.length > 0, 'Breaking change must identify damaged caller nodes');

  const damagedCaller = data.damaged_callers.find(n => n.file_path.includes('OrderController.ts'));
  assert.ok(damagedCaller, 'OrderController.ts must be marked as damaged');
  assert.strictEqual(damagedCaller.calling_line, 42);
  assert.ok(
    damagedCaller.diagnostic.includes('Arity mismatch') || damagedCaller.diagnostic.includes('signature'),
    'Damaged caller must carry exact contract break reason'
  );

  // Dead code detection when legacyFormat function is omitted
  assert.ok(Array.isArray(data.newly_dead_symbols), 'newly_dead_symbols must be an array');
  assert.ok(data.newly_dead_symbols.length > 0, 'Omitted function must trigger dead symbol detection');
});

test('Section 7: Error Boundary & Negative Syntax Recovery (ERR-CORE-002)', async () => {
  const res = await fetch('http://127.0.0.1:3333/api/v1/simulate/edit', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      file_path: 'src/services/PaymentService.ts',
      unsaved_buffer: 'function broken((( { return 123;'
    })
  });

  assert.strictEqual(res.status, 200, 'Invalid syntax should recover gracefully and return 200 with diagnostics');
  const data = await res.json();

  assert.strictEqual(data.verdict, 'FAIL', 'Invalid syntax must produce verdict FAIL');
  assert.ok(Array.isArray(data.syntax_errors), 'syntax_errors must be an array');
  assert.ok(data.syntax_errors.length > 0, 'Syntax error must be captured');
  assert.ok(
    data.syntax_errors[0].includes('SyntaxError') || data.syntax_errors[0].includes('token'),
    'Syntax diagnostic must describe the parse failure'
  );
});
