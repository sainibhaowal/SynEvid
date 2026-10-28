import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import fs from 'node:fs';
import os from 'node:os';
import { createAutopsyMcpServer } from '../dist/server.js';
import {
  runAutopsy,
  resolveAutopsyBinary,
  pinningFlags,
  pinnedSnapshotId,
  resolveTimeoutMs,
  resolveMaxBufferBytes,
} from '../dist/cli.js';
import { evidenceEnvelope, refusedEnvelope, repairAdviceFromStderr } from '../dist/evidence.js';
import { resolveAgainstRepoRoot } from '../dist/server.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_API_ROOT = path.resolve(__dirname, '../../../benchmarks/corpus/repo-api');

// Pin tests to the current workspace build when present: resolver order
// otherwise prefers a possibly stale ~/.cargo/bin install, which would
// validate old behavior instead of the code under test.
const WORKSPACE_DEBUG_BIN = path.resolve(__dirname, '../../../target/debug/autopsy');
try {
  const fsSync = await import('node:fs');
  if (!process.env.AUTOPSY_BIN && fsSync.existsSync(WORKSPACE_DEBUG_BIN)) {
    process.env.AUTOPSY_BIN = WORKSPACE_DEBUG_BIN;
  }
} catch {
  // fall through to default resolution
}

test('CLI runner resolves valid autopsy binary', () => {
  const binary = resolveAutopsyBinary();
  assert.ok(binary, 'Autopsy binary path must not be empty');
});

test('CLI runner prefers the installer prefix ~/.local/bin', () => {
  const tmpHome = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-home-'));
  const localBin = path.join(tmpHome, '.local', 'bin', 'autopsy');
  const savedBin = process.env.AUTOPSY_BIN;
  const savedHome = process.env.HOME;
  try {
    fs.mkdirSync(path.dirname(localBin), { recursive: true });
    fs.writeFileSync(localBin, '#!/bin/sh\n');
    delete process.env.AUTOPSY_BIN;
    process.env.HOME = tmpHome;
    assert.equal(resolveAutopsyBinary(), localBin);
  } finally {
    if (savedBin === undefined) delete process.env.AUTOPSY_BIN;
    else process.env.AUTOPSY_BIN = savedBin;
    if (savedHome === undefined) delete process.env.HOME;
    else process.env.HOME = savedHome;
    fs.rmSync(tmpHome, { recursive: true, force: true });
  }
});

test('runAutopsy executes baseline command on repo-api deterministically', async () => {
  const result = await runAutopsy('baseline', [], { repoRoot: REPO_API_ROOT });
  assert.equal(result.success, true);
  assert.equal(result.exitCode, 0);
  assert.ok(result.data, 'Output data should not be null');
  assert.ok(result.data.result_digest, 'Output data must contain result_digest');
  assert.equal(typeof result.data.result_digest, 'string');
  assert.ok(result.data.snapshots && result.data.snapshots.length > 0, 'Must contain snapshots');
});

test('runAutopsy executes impact command on UserService', async () => {
  const result = await runAutopsy('impact', ['-s', 'UserService'], { repoRoot: REPO_API_ROOT });
  assert.equal(result.success, true);
  assert.equal(result.exitCode, 0);
  assert.ok(result.data.impact, 'Must contain impact structure');
  assert.ok(
    result.data.impact.impacted_entities.some((e) => e.includes('UserService')),
    'UserService must be in impacted entities'
  );
});

test('runAutopsy executes verify command on repo-api', async () => {
  const result = await runAutopsy('verify', [], { repoRoot: REPO_API_ROOT });
  assert.equal(result.success, true);
  assert.equal(result.exitCode, 0);
  assert.ok(result.data.coverage, 'Must contain coverage information');
  assert.equal(result.data.coverage.state, 'verified');
});

test('createAutopsyMcpServer registers all 11 tools and resources', () => {
  const server = createAutopsyMcpServer();
  assert.ok(server, 'McpServer instance must be created');

  const tools = Object.keys(server._registeredTools);
  assert.ok(tools.includes('autopsy_status'), 'Must register autopsy_status');
  assert.ok(tools.includes('autopsy_impact'), 'Must register autopsy_impact');
  assert.ok(tools.includes('autopsy_diff'), 'Must register autopsy_diff');
  assert.ok(tools.includes('autopsy_verify'), 'Must register autopsy_verify');
  assert.ok(tools.includes('autopsy_explain'), 'Must register autopsy_explain');
  assert.ok(tools.includes('autopsy_find'), 'Must register autopsy_find (Tier 1.1 symbol resolution)');
  for (const tool of [
    'autopsy_temporal',
    'autopsy_test_selection',
    'autopsy_contract_diff',
    'autopsy_distributed_blast_radius',
    'autopsy_git_status',
    'autopsy_git_galaxy',
    'autopsy_git_diff',
    'autopsy_git_bisect',
    'autopsy_git_commit',
    'autopsy_git_undo',
  ]) {
    assert.ok(tools.includes(tool), `Must register ${tool}`);
  }
  assert.equal(tools.length, 17, '5 mandatory + 4 Phase 4.2 + find + session + 6 Phase 3 Git tools');

  const resources = Object.keys(server._registeredResources);
  const templates = Object.keys(server._registeredResourceTemplates || {});
  assert.ok(
    resources.includes('autopsy://graph') ||
      templates.includes('autopsy_graph'),
    'Must register the autopsy://graph resource (template or fixed URI)'
  );

  assert.ok(templates.includes('autopsy_graph_symbols'), 'Must register paginated symbols template');
  assert.ok(templates.includes('autopsy_graph_edges'), 'Must register paginated edges template');
  assert.ok(templates.includes('autopsy_graph_map'), 'Must register the bounded code-map template (Tier 1.3)');
  assert.ok(
    templates.includes('autopsy_workspace'),
    'Must register the Tier 2.3 workspace registry resource'
  );
  assert.ok(
    templates.includes('autopsy_runs_latest'),
    'Must register the Tier 3.7 latest forensic trajectory report resource'
  );
  assert.ok(
    resources.includes('autopsy://tools'),
    'Must register the Tier 3.1 toolspec resource as a fixed URI (autopsy://tools)'
  );
});

test('autopsy://tools resource serves the deterministic toolspec (Tier 3.1)', async () => {
  const server = createAutopsyMcpServer();
  const resource = server._registeredResources['autopsy://tools'];
  assert.ok(resource, 'the toolspec resource must be registered at its documented URI');
  const read = async () => resource.readCallback(new URL('autopsy://tools'));
  const first = await read();
  const second = await read();
  const text = first.contents[0].text;
  assert.equal(second.contents[0].text, text, 'two reads must be byte-identical');
  const spec = JSON.parse(text);
  assert.equal(spec.format_version, '0.0.1');
  assert.ok(Array.isArray(spec.tools) && spec.tools.length >= 22, 'every subcommand is listed');
  assert.ok(spec.exit_codes, 'the exit-code contract is present');
  assert.ok(spec.feedback, 'the feedback channel is documented');
  assert.ok(spec.envelope, 'the evidence envelope is documented');
});

// ---------------------------------------------------------------------------------------
// Tier 2 — adapter languages, repair advice, session receipts, workspace registry
// ---------------------------------------------------------------------------------------

test('repairAdviceFromStderr parses the engine error advice exactly', () => {
  const stderr = [
    'Error: Analysis error: snapshot \'x\' not stored; capture one with `autopsy baseline --persist`',
    'what_failed: analysis_snapshot_missing',
    'why: the referenced snapshot is not present in the storage engine',
    'next_action: capture one first: run `autopsy baseline --persist` and reuse its snapshot id',
  ].join('\n');
  const advice = repairAdviceFromStderr(stderr);
  assert.ok(advice, 'the markers must parse');
  assert.equal(advice.what_failed, 'analysis_snapshot_missing');
  assert.match(advice.next_action, /baseline --persist/);
});

test('repairAdviceFromStderr stays null when the engine produced evidence, not an error', () => {
  // An exit-2 policy failure with a report carries no advice markers.
  assert.equal(repairAdviceFromStderr('{"coverage": "verified"}\n'), null);
  assert.equal(repairAdviceFromStderr(''), null);
});

test('evidenceEnvelope attaches engine repair advice on failure and nothing on pass', async () => {
  const empty = await runAutopsy('find', ['   '], {
    repoRoot: path.resolve(__dirname, '../../../benchmarks/corpus/repo-api'),
  });
  assert.equal(empty.exitCode, 3, 'a blank pattern is a caller error at exit 3');
  const envelope = evidenceEnvelope('find', empty);
  assert.ok(envelope.repair, 'a failure with engine markers must carry repair advice');
  assert.equal(envelope.repair.what_failed, 'analysis_empty_input');
  assert.ok(envelope.repair.why);
  assert.ok(envelope.repair.next_action);

  const refused = refusedEnvelope('explain', 'missing target', 'pass a target');
  assert.equal(refused.exit_code, 3);
  assert.equal(refused.repair.next_action, 'pass a target');
  assert.equal(refused.repair.what_failed, 'input_refused');
});

test('resolveAgainstRepoRoot resolves relative paths deterministically', () => {
  assert.equal(
    resolveAgainstRepoRoot('specs/checkout.json', '/work/app'),
    '/work/app/specs/checkout.json'
  );
  assert.equal(
    resolveAgainstRepoRoot('/abs/trace.json', '/work/app'),
    '/abs/trace.json',
    'absolute paths stay untouched'
  );
});

test('pinningFlags covers the Tier 2.4 browse family with --snapshot-id', () => {
  for (const subcommand of ['symbols', 'edges', 'find', 'map']) {
    assert.deepEqual(pinningFlags(subcommand, 'abc123'), [
      '--snapshot-id',
      'abc123',
    ], `${subcommand} must pin`);
  }
  assert.deepEqual(pinningFlags('session', 'abc123'), [], 'sessions do not take pin flags');
});

test('browse pinning reads a stored snapshot end to end', async () => {
  const repoRoot = REPO_API_ROOT;
  const stored = await runAutopsy('baseline', ['--persist'], { repoRoot });
  assert.equal(stored.exitCode, 0, 'baseline must persist');
  const snapshotId = stored.data?.snapshots?.[0]?.snapshot_id;
  assert.ok(snapshotId, 'a stored snapshot id must exist');

  const pinned = await runAutopsy('symbols', ['--snapshot-id', snapshotId], { repoRoot });
  assert.equal(pinned.exitCode, 0, 'pinned browse must succeed');
  assert.ok(Array.isArray(pinned.data.symbols));

  const badPin = await runAutopsy('symbols', ['--snapshot-id', 'not-a-snapshot'], { repoRoot });
  assert.equal(badPin.exitCode, 3, 'an unknown pin is an analysis error, not a silent reparse');
  const envelope = evidenceEnvelope('symbols', badPin);
  assert.ok(envelope.repair, 'the pinned miss carries repair advice');
  assert.match(envelope.repair.next_action, /baseline --persist/);
});

test('autopsy_session runs the full repair-loop receipt lifecycle', async () => {
  const sessionRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-session-'));
  try {
    const start = await runAutopsy('session', ['start'], { repoRoot: sessionRoot });
    assert.equal(start.exitCode, 0, 'session start');
    const sessionId = start.data.session_id;
    assert.ok(sessionId, 'the session id is the receipt anchor');

    const record = await runAutopsy(
      'session',
      [
        'record',
        '--session-id', sessionId,
        '--exit-code', '2',
        '--note', 'breaking change flagged',
        '--',
        'verify',
        '--strict',
      ],
      { repoRoot: sessionRoot }
    );
    assert.equal(record.exitCode, 0, 'recording is a store write, not a verdict');

    const close = await runAutopsy('session', ['close', '--session-id', sessionId], {
      repoRoot: sessionRoot,
    });
    assert.equal(close.exitCode, 2, 'the close mirrors the failing evidence exit code');
    assert.equal(close.data.verdict, 'fail');
    assert.equal(close.data.evidence_step, 'verify');
    assert.equal(close.data.evidence_exit_code, 2);
    assert.equal(close.data.steps, 1);

    const reclosed = await runAutopsy('session', ['close', '--session-id', sessionId], {
      repoRoot: sessionRoot,
    });
    assert.equal(reclosed.exitCode, 3, 'a closed receipt is final');
    assert.ok(reclosed.data?.error || reclosed.rawStderr, 'the second close refuses honestly');
  } finally {
    fs.rmSync(sessionRoot, { recursive: true, force: true });
  }
});

test('autopsy_session schema exposes the lifecycle actions honestly', () => {
  const server = createAutopsyMcpServer();
  const tool = server._registeredTools.autopsy_session;
  assert.ok(tool, 'the session tool must be registered');
  const shape = tool.inputSchema.shape;
  assert.ok(shape.action, 'the action enum must be in the schema');
  const values =
    shape.action.options ??
    shape.action._def?.values ??
    shape.action._def?.checks?.[0]?.values;
  assert.deepEqual([...values], ['start', 'record', 'close', 'report', 'show', 'list']);
});

// ---------------------------------------------------------------------------------------
// Tier 1 — agent-experience surface (find, map, envelope, pinning, compaction)
// ---------------------------------------------------------------------------------------

test('runAutopsy find resolves a partial name the way an agent asks', async () => {
  const result = await runAutopsy('find', ['userservice', '--name-only'], { repoRoot: REPO_API_ROOT });
  assert.equal(result.success, true);
  assert.equal(result.exitCode, 0);
  assert.equal(result.data.total >= 1, true, 'the corpus UserService must resolve');
  assert.equal(
    result.data.symbols.some((sym) => /userservice/i.test(sym.qualified_name)),
    true,
    'a case-insensitive partial name resolves the symbol'
  );
  const hit = result.data.symbols[0];
  assert.ok(hit.stable_id, 'every hit carries the exact identity impact -s needs');
  assert.ok(hit.qualified_name);
});

test('runAutopsy find is deterministic and paginates', async () => {
  const first = await runAutopsy('find', ['a', '--limit', '2', '--offset', '0'], { repoRoot: REPO_API_ROOT });
  const second = await runAutopsy('find', ['a', '--limit', '2', '--offset', '2'], { repoRoot: REPO_API_ROOT });
  assert.equal(first.data.total, second.data.total, 'total is the population, not the page');
  const ids1 = new Set(first.data.symbols.map((sym) => sym.stable_id));
  for (const sym of second.data.symbols) {
    assert.ok(!ids1.has(sym.stable_id), 'find pages must not overlap');
  }
  const rerun = await runAutopsy('find', ['a', '--limit', '2', '--offset', '0'], { repoRoot: REPO_API_ROOT });
  assert.deepEqual(rerun.data.symbols, first.data.symbols, 'byte-identical rerun');
});

test('runAutopsy find refuses an empty pattern with exit 3, not an empty success', async () => {
  const result = await runAutopsy('find', ['   '], { repoRoot: REPO_API_ROOT });
  assert.equal(result.exitCode, 3, 'a blank pattern is a caller bug');
  assert.equal(result.success, false);
  const envelope = evidenceEnvelope('find', result);
  assert.equal(envelope.actionable, false, 'the refusal is stated in the envelope');
});

test('runAutopsy map returns a bounded, deterministic whole-graph projection', async () => {
  const first = await runAutopsy('map', ['--top', '3'], { repoRoot: REPO_API_ROOT });
  assert.equal(first.success, true);
  assert.equal(first.data.symbols_total > 0, true, 'the corpus has symbols to map');
  assert.ok(first.data.kind_counts, 'kind counts are the map');
  assert.ok(first.data.package_counts, 'package counts are the map');
  assert.equal(first.data.top_symbols.length <= 3, true, 'top is bounded');
  const top = first.data.top_symbols;
  assert.equal(
    top.every((n, i) => i === 0 || top[i - 1].degree >= n.degree),
    true,
    'the top list ranks by degree descending'
  );
  const second = await runAutopsy('map', ['--top', '3'], { repoRoot: REPO_API_ROOT });
  assert.deepEqual(second.data, first.data, 'byte-identical rerun');
});

test('runAutopsy map scope narrows counts and states the scope', async () => {
  const every = await runAutopsy('map', [], { repoRoot: REPO_API_ROOT });
  const scoped = await runAutopsy('map', ['--under', 'src/components'], { repoRoot: REPO_API_ROOT });
  assert.equal(scoped.data.under, 'src/components', 'a scoped map states its scope');
  assert.equal(
    scoped.data.symbols_total <= every.data.symbols_total,
    true,
    'a scoped population is a subset'
  );
});

test('mandatory tools now speak the one envelope: exit code and actionable travel with every answer', async () => {
  const impact = await runAutopsy('impact', ['-s', 'UserService'], { repoRoot: REPO_API_ROOT });
  const envel = evidenceEnvelope('impact', impact);
  assert.equal(envel.exit_code, 0);
  assert.equal(envel.actionable, true);
  assert.ok(envel.report.impact, 'the report rides inside the envelope');
});

test('pinningFlags map each subcommand to its stored-snapshot flag', () => {
  assert.deepEqual(pinningFlags('impact', 'snap-1'), ['--snapshot-id', 'snap-1']);
  assert.deepEqual(pinningFlags('diff', 'snap-1'), ['--before', 'snap-1']);
  assert.deepEqual(pinningFlags('verify', 'snap-1'), ['--baseline-id', 'snap-1']);
  assert.deepEqual(pinningFlags('baseline', 'snap-1'), ['--incremental', '--baseline-id', 'snap-1']);
  assert.deepEqual(pinningFlags('symbols', 'snap-1'), ['--snapshot-id', 'snap-1']);
  assert.deepEqual(pinningFlags('edges', 'snap-1'), ['--snapshot-id', 'snap-1']);
  assert.deepEqual(pinningFlags('find', 'snap-1'), ['--snapshot-id', 'snap-1']);
  assert.deepEqual(pinningFlags('map', 'snap-1'), ['--snapshot-id', 'snap-1']);
  assert.deepEqual(pinningFlags('session', 'snap-1'), [], 'bookkeeping takes no pin');
});

test('pinnedSnapshotId: explicit argument wins over AUTOPSY_SNAPSHOT', () => {
  const previous = process.env.AUTOPSY_SNAPSHOT;
  process.env.AUTOPSY_SNAPSHOT = 'env-snapshot';
  assert.equal(pinnedSnapshotId(), 'env-snapshot');
  assert.equal(pinnedSnapshotId('explicit'), 'explicit', 'the tool argument wins');
  assert.equal(
    pinnedSnapshotId('  '),
    'env-snapshot',
    'a blank argument is not an explicit choice: the session pin still applies'
  );
  delete process.env.AUTOPSY_SNAPSHOT;
  assert.equal(pinnedSnapshotId('  '), undefined, 'with no pin anywhere, there is no pin');
  assert.equal(pinnedSnapshotId('  '), pinnedSnapshotId(), 'blank and absent resolve identically');
  if (previous === undefined) {
    delete process.env.AUTOPSY_SNAPSHOT;
  } else {
    process.env.AUTOPSY_SNAPSHOT = previous;
  }
});

test('resolveTimeoutMs honors AUTOPSY_TIMEOUT_MS and keeps 30s default', () => {
  const previous = process.env.AUTOPSY_TIMEOUT_MS;
  delete process.env.AUTOPSY_TIMEOUT_MS;
  assert.equal(resolveTimeoutMs(), 30_000);
  assert.equal(resolveTimeoutMs(45_000), 45_000, 'an explicit tool timeout wins');
  process.env.AUTOPSY_TIMEOUT_MS = '120000';
  assert.equal(resolveTimeoutMs(), 120_000);
  assert.equal(resolveTimeoutMs(45_000), 45_000);
  process.env.AUTOPSY_TIMEOUT_MS = 'not-a-number';
  assert.equal(resolveTimeoutMs(), 30_000, 'a garbage env value is ignored, not trusted');
  process.env.AUTOPSY_TIMEOUT_MS = '0';
  assert.equal(resolveTimeoutMs(), 30_000, 'a zero timeout is no timeout');
  if (previous === undefined) {
    delete process.env.AUTOPSY_TIMEOUT_MS;
  } else {
    process.env.AUTOPSY_TIMEOUT_MS = previous;
  }
});

test('resolveMaxBufferBytes honors AUTOPSY_MAX_BUFFER_BYTES and keeps 20MB default', () => {
  const previous = process.env.AUTOPSY_MAX_BUFFER_BYTES;
  delete process.env.AUTOPSY_MAX_BUFFER_BYTES;
  assert.equal(resolveMaxBufferBytes(), 20 * 1024 * 1024);
  assert.equal(resolveMaxBufferBytes(1024), 1024, 'an explicit maxBuffer wins');
  process.env.AUTOPSY_MAX_BUFFER_BYTES = '1048576';
  assert.equal(resolveMaxBufferBytes(), 1048576);
  process.env.AUTOPSY_MAX_BUFFER_BYTES = 'not-a-number';
  assert.equal(resolveMaxBufferBytes(), 20 * 1024 * 1024, 'a garbage env value is ignored, not trusted');
  process.env.AUTOPSY_MAX_BUFFER_BYTES = '0';
  assert.equal(resolveMaxBufferBytes(), 20 * 1024 * 1024, 'a zero maxBuffer is no maxBuffer');
  if (previous === undefined) {
    delete process.env.AUTOPSY_MAX_BUFFER_BYTES;
  } else {
    process.env.AUTOPSY_MAX_BUFFER_BYTES = previous;
  }
});
test('snapshot pinning reuses a stored baseline: N impact calls, one parse', async () => {
  // Build the stored state once.
  const baseline = await runAutopsy('baseline', [], { repoRoot: REPO_API_ROOT });
  assert.equal(baseline.exitCode, 0);
  const snapshotId = baseline.data.snapshots[0].snapshot_id;

  const unpinned = await runAutopsy('impact', ['-s', 'UserService'], { repoRoot: REPO_API_ROOT });
  const pinned = await runAutopsy('impact', ['-s', 'UserService'], {
    repoRoot: REPO_API_ROOT,
    snapshotId,
  });
  assert.equal(pinned.exitCode, 0);
  assert.deepEqual(
    pinned.data.impact,
    unpinned.data.impact,
    'a pinned analysis must be the byte-identical answer of the worktree analysis at the same revision'
  );

  // Unknown snapshot IDs refuse loudly rather than silently re-parsing the worktree.
  const ghost = await runAutopsy('impact', ['-s', 'UserService'], {
    repoRoot: REPO_API_ROOT,
    snapshotId: '00000000-ghost',
  });
  assert.equal(ghost.exitCode, 3, 'unresolvable pin is an analysis error, not one that lies');
  assert.equal(ghost.success, false);
});

test('compact keeps counts and verdicts exact while naming the residual', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'autopsy-mcp-compact-'));
  const head = 'openapi: 3.2.1\ninfo:\n  title: shop\n  version: 1.0.0\npaths:\n';
  const routes = ['  /a:\n', '  /b:\n', '  /c:\n', '  /d:\n', '  /e:\n']
    .map((p) => p + "    get:\n      responses:\n        '200':\n          description: ok\n")
    .join('');
  // Removing /a /b /c /d /e gives five breaking changes — comfortably above top_k 2.
  const from = writeFile('from.yaml', head + routes, dir);
  const to = writeFile('to.yaml', head, dir);
  const full = await runAutopsy('contract-dist', [from, to, '--protocol', 'openapi'], { repoRoot: dir });
  assert.equal(full.exitCode, 2);
  const compact = await runAutopsy(
    'contract-dist',
    [from, to, '--protocol', 'openapi', '--compact', '--top-k', '2'],
    { repoRoot: dir }
  );
  assert.equal(compact.exitCode, 2, 'compaction must not move the verdict');
  assert.equal(compact.data.verdict, 'breaking');
  assert.equal(compact.data.compact, true, 'a compacted view is labelled');
  const primary = Array.isArray(compact.data.changes) ? 'changes' : null;
  assert.notEqual(primary, null, 'the change list is the primary list of the report');
  if (primary) {
    assert.equal(compact.data[primary].length, 2, 'the slice honors top_k');
    assert.equal(compact.data[`${primary}_omitted`], 3, 'the residual is named: 5 changes - 2 shown');
    assert.equal(Array.isArray(full.data.changes) && full.data.changes.length, 5);
  }
});

test('runAutopsy paginates symbols deterministically', async () => {
  const page1 = await runAutopsy('symbols', ['--limit', '2', '--offset', '0'], { repoRoot: REPO_API_ROOT });
  assert.equal(page1.success, true);
  assert.equal(page1.exitCode, 0);
  assert.ok(Array.isArray(page1.data.symbols), 'Must contain symbols array');
  assert.ok(page1.data.total >= page1.data.symbols.length, 'Total must bound page');
  assert.equal(page1.data.limit, 2);
  assert.equal(page1.data.offset, 0);

  const page2 = await runAutopsy('symbols', ['--limit', '2', '--offset', '2'], { repoRoot: REPO_API_ROOT });
  const ids1 = new Set(page1.data.symbols.map((s) => s.stable_id));
  for (const s of page2.data.symbols) {
    assert.ok(!ids1.has(s.stable_id), 'Pages must not overlap');
  }
  // Deterministic rerun: identical bytes, identical order.
  const rerun = await runAutopsy('symbols', ['--limit', '2', '--offset', '0'], { repoRoot: REPO_API_ROOT });
  assert.deepEqual(rerun.data.symbols, page1.data.symbols);
});

test('runAutopsy browses 1-hop edges for a symbol', async () => {
  const syms = await runAutopsy('symbols', ['--limit', '1', '--kind', 'method'], { repoRoot: REPO_API_ROOT });
  assert.ok(syms.data.symbols.length > 0, 'Corpus must contain a method symbol');
  const seed = syms.data.symbols[0].stable_id;
  const result = await runAutopsy('edges', ['--source', seed, '--limit', '50'], { repoRoot: REPO_API_ROOT });
  assert.equal(result.success, true);
  assert.equal(result.exitCode, 0);
  assert.ok(Array.isArray(result.data.edges), 'Must contain edges array');
  assert.equal(result.data.source, seed);
});

test('runAutopsy rejects unknown edge-browse symbols explicitly', async () => {
  const result = await runAutopsy('edges', ['--source', 'no-such-symbol'], { repoRoot: REPO_API_ROOT });
  assert.equal(result.success, false, 'Unknown symbols must fail, never return empty success');
});

// ---------------------------------------------------------------------------------------
// Phase 4.2 evidence tools (Step 45)
//
// The interesting assertions are the exit codes. An engine that refuses to hand back an
// actionable answer says so with `4`, and a tool layer that turned that into a transport
// error would hide the refusal behind a stack trace.
// ---------------------------------------------------------------------------------------

function writeFile(relative, content, root) {
  const full = path.join(root, relative);
  fs.mkdirSync(path.dirname(full), { recursive: true });
  fs.writeFileSync(full, content);
  return full;
}

/** A two-component catalog whose relationship fields use **bare names**, as real ones do. */
function catalogFixture() {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'autopsy-mcp-catalog-'));
  writeFile(
    'components/checkout/catalog-info.yaml',
    'apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: checkout\nspec:\n  type: service\n  lifecycle: production\n  owner: team-x\n',
    root
  );
  writeFile(
    'components/cart/catalog-info.yaml',
    'apiVersion: backstage.io/v1alpha1\nkind: Component\nmetadata:\n  name: cart\nspec:\n  type: service\n  lifecycle: production\n  owner: team-x\n',
    root
  );
  return root;
}

test('evidenceEnvelope keeps the exit code, because 4 is a refusal and not a failure', () => {
  const refused = evidenceEnvelope('temporal', {
    success: true,
    exitCode: 4,
    data: { stats: { total: 3 } },
    rawStdout: '',
    rawStderr: '',
  });
  assert.equal(refused.exit_code, 4);
  assert.equal(refused.actionable, false, 'a refusal must not look actionable');
  assert.deepEqual(refused.report, { stats: { total: 3 } }, 'the evidence is still returned');

  const ok = evidenceEnvelope('cochange', {
    success: true,
    exitCode: 0,
    data: null,
    rawStdout: '',
    rawStderr: '',
  });
  assert.equal(ok.actionable, true);

  const policy = evidenceEnvelope('contract-dist', {
    success: true,
    exitCode: 2,
    data: { verdict: 'breaking' },
    rawStdout: '',
    rawStderr: '',
  });
  assert.equal(policy.actionable, false, 'a breaking change is a policy gate, not a success');
});

test('runAutopsy blast-radius joins a bare owner name to the team group', async () => {
  const root = catalogFixture();
  const result = await runAutopsy('blast-radius', ['component:default/checkout', '--root', root], {
    repoRoot: root,
  });
  assert.equal(result.exitCode, 0);
  assert.equal(result.success, true);
  const radius = result.data.blast_radius;
  assert.deepEqual(radius.owners, ['group:default/team-x'], 'a bare owner name is a group');
  assert.equal(radius.owner_is_team, true);
  assert.ok(
    radius.co_owned_components.includes('component:default/cart'),
    'the same team owns cart, and that join is the point of the tool'
  );
  assert.ok(
    result.data.catalog.entities.length >= 2,
    'the catalog the radius was computed from travels with it'
  );
});

test('runAutopsy treats exit 4 as evidence rather than a transport error', async () => {
  const root = catalogFixture();
  const result = await runAutopsy('blast-radius', ['component:default/ghost', '--root', root], {
    repoRoot: root,
  });
  assert.equal(result.exitCode, 4, 'an absent component is a refusal to answer');
  assert.equal(result.success, true, 'the analysis did not fail — it declined to be actionable');
  assert.equal(result.data.blast_radius.found, false);
  assert.equal(result.data.blast_radius.coverage, 'unknown');
});

test('runAutopsy contract-dist exits 2 on a breaking change and still returns the diff', async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'autopsy-mcp-contract-'));
  const head = 'openapi: 3.2.1\ninfo:\n  title: shop\n  version: 1.0.0\npaths:\n';
  const pay = "  /pay:\n    get:\n      responses:\n        '200':\n          description: ok\n";
  const refund = "  /refund:\n    get:\n      responses:\n        '200':\n          description: ok\n";
  const from = writeFile('from.yaml', head + pay + refund, dir);
  const to = writeFile('to.yaml', head + pay, dir);

  const result = await runAutopsy('contract-dist', [from, to, '--protocol', 'openapi'], {
    repoRoot: dir,
  });
  assert.equal(result.exitCode, 2, 'a removed endpoint is a policy failure');
  assert.equal(result.success, true, 'and it is still a successful analysis');
  assert.equal(result.data.verdict, 'breaking');
});

test('runAutopsy temporal replays the history of this workspace', async () => {
  const repoRoot = path.resolve(__dirname, '../../..');
  const result = await runAutopsy('temporal', ['--max-commits', '100000'], { repoRoot, timeoutMs: 90000 });
  assert.equal(result.exitCode, 0, result.rawStderr);
  assert.ok(result.data.stats.total > 0, 'the workspace has history to replay');
  assert.ok(Array.isArray(result.data.journal), 'the journal projection is embedded verbatim');
  assert.ok(
    result.data.notes.some((note) => note.includes('not which symbols')),
    'the extractor granularity is stated in the machine-readable output'
  );
});

test('runAutopsy lineage traces file identity between two revisions', async () => {
  const repoRoot = path.resolve(__dirname, '../../..');
  const result = await runAutopsy('lineage', ['--from', 'HEAD~2', '--to', 'HEAD'], { repoRoot });
  assert.equal(result.exitCode, 0, result.rawStderr);
  assert.ok(
    ['verified', 'partial'].includes(result.data.coverage),
    'coverage is stated, never guessed'
  );
  assert.match(result.data.from.resolved, /^[0-9a-f]{40}$/);
  assert.match(result.data.to.resolved, /^[0-9a-f]{40}$/);
  assert.ok(Array.isArray(result.data.events), 'events are the answer');
  assert.equal(typeof result.data.summary.total, 'number');
  assert.equal(result.data.summary.total, result.data.events.length);
  for (const event of result.data.events) {
    if (event.coverage === 'partial') {
      assert.ok(event.ambiguity, 'a partial event must say why it could not verify');
    }
  }
});

test('the lineage bound is a stated lower bound, and strict refuses it', async () => {
  const repoRoot = path.resolve(__dirname, '../../..');
  const result = await runAutopsy(
    'lineage',
    ['--from', 'HEAD~2', '--to', 'HEAD', '--max-files', '1', '--strict'],
    { repoRoot }
  );
  assert.equal(result.exitCode, 4, 'a bounded identity answer is not actionable under strict');
  assert.equal(result.success, true, 'a refusal is still successful transport');
  assert.equal(evidenceEnvelope('lineage', result).actionable, false);
  assert.equal(result.data.coverage, 'partial');
  assert.ok(
    result.data.notes.some((note) => note.includes('lower bound')),
    'the machine-readable output must say the event set is a lower bound'
  );
});

test('runAutopsy test-select reports a partial selection rather than a guess', async () => {
  const result = await runAutopsy('test-select', ['--changed', 'src/user.ts'], {
    repoRoot: REPO_API_ROOT,
  });
  assert.equal(result.exitCode, 0);
  assert.equal(result.data.coverage, 'partial', 'a file-level change set cannot be verified');
  assert.ok(result.data.digest, 'the plan is digested');
  assert.equal(result.data.fallback.kind, 'none');
});
