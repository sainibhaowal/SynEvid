'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const ext = require('../extension.js');
const pkg = require('../package.json');

test('VS Code extension manifest declares all commands, views, and activation events', () => {
  assert.equal(pkg.name, 'synevid-vscode');
  assert.equal(pkg.version, '0.1.0');
  const commandIds = pkg.contributes.commands.map((c) => c.command);
  for (const id of [
    'synevid.init',
    'synevid.verify',
    'synevid.impactAtCursor',
    'synevid.showForensicReport',
    'synevid.doctor',
    'synevid.refreshTrajectory',
  ]) {
    assert.ok(commandIds.includes(id), `missing command ${id}`);
  }
  assert.equal(pkg.contributes.views['synevid-forensics'][0].id, 'synevidTrajectoryView');
});

test('extractDeclarationSymbols identifies TypeScript, Python, Go, and Rust declarations', () => {
  const sample = [
    'export async function createOrder(id: string) {',
    'export class OrderService {',
    'def compute_total(items):',
    'func (s *Server) HandleRequest(w ResponseWriter) {',
    'pub fn verify_snapshot(root: &Path) -> Result<(), Error> {',
    'pub struct ForensicLedger {',
  ].join('\n');

  const hits = ext.extractDeclarationSymbols(sample);
  assert.deepEqual(
    hits.map((h) => h.symbol),
    [
      'createOrder',
      'OrderService',
      'compute_total',
      'HandleRequest',
      'verify_snapshot',
      'ForensicLedger',
    ]
  );
});

test('loadLatestForensicReport and formatTrajectoryItems render deterministic forensic ledger', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-vscode-test-'));
  try {
    assert.equal(ext.loadLatestForensicReport(tmp), null);
    const emptyItems = ext.formatTrajectoryItems(null);
    assert.equal(emptyItems.length, 1);
    assert.match(emptyItems[0].label, /No forensic trajectory/);

    const runsDir = path.join(tmp, '.autopsy', 'runs');
    fs.mkdirSync(runsDir, { recursive: true });
    const fakeReport = {
      session_id: 'a1b2c3d4e5f60123456789abcdef0123',
      status: 'closed',
      verdict: 'pass',
      evidence_exit_code: 0,
      evidence_step: 'verify',
      total_steps: 2,
      failed_steps: 0,
      report_digest: '99887766554433221100aabbccddeeff',
      next_agent_guidance: 'PASS: Prior session closed cleanly.',
      steps: [
        { index: 0, invoked: ['baseline', '--persist'], exit_code: 0, note: 'initial snapshot' },
        { index: 1, invoked: ['verify'], exit_code: 0, note: 'all invariants green' },
      ],
    };
    fs.writeFileSync(path.join(runsDir, 'latest.json'), JSON.stringify(fakeReport, null, 2));

    const loaded = ext.loadLatestForensicReport(tmp);
    assert.equal(loaded.verdict, 'pass');
    const items = ext.formatTrajectoryItems(loaded);
    assert.equal(items.length, 5);
    assert.equal(items[0].label, 'Verdict: PASS');
    assert.match(items[1].label, /Digest: 9988776655443322/);
    assert.match(items[3].label, /Step 0: autopsy baseline --persist/);
    assert.match(items[4].label, /Step 1: autopsy verify/);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});

test('formatCodeLensLabel renders caller and test metrics correctly', () => {
  assert.equal(
    ext.formatCodeLensLabel('createOrder', 12, 3),
    '$(pulse) 12 callers · 3 tests'
  );
  assert.equal(
    ext.formatCodeLensLabel('OrderService', 1, 1),
    '$(pulse) 1 caller · 1 test'
  );
  assert.equal(
    ext.formatCodeLensLabel('calcTax', 5),
    '$(pulse) 5 callers'
  );
  assert.equal(
    ext.formatCodeLensLabel('OrderService'),
    '$(pulse) SynEvid: Blast Radius (OrderService)'
  );
});

test('parseBlastRadiusMetrics extracts caller and test counts from impact report', () => {
  const fakeReport = {
    impact: {
      total_impacted: 4,
      impacted_entities: [
        { symbol_id: 'src/services/order.ts::createOrder', kind: 'method' },
        { symbol_id: 'src/api/routes.ts::handlePost', kind: 'function' },
        { symbol_id: 'tests/order.test.ts::testCreateOrder', kind: 'test' },
        { symbol_id: 'tests/e2e.spec.ts::specWorkflow', kind: 'function' },
      ],
    },
  };
  const metrics = ext.parseBlastRadiusMetrics(fakeReport);
  assert.equal(metrics.callers, 4);
  assert.equal(metrics.tests, 2);
});

test('resolveBinary respects explicit path override', () => {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-bin-test-'));
  try {
    const fakeBin = path.join(tmp, 'autopsy-custom');
    fs.writeFileSync(fakeBin, '#!/bin/sh\n');
    assert.equal(ext.resolveBinary('autopsy', fakeBin, tmp), fakeBin);
  } finally {
    fs.rmSync(tmp, { recursive: true, force: true });
  }
});
