import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import fs from 'node:fs';
import os from 'node:os';
import { createAutopsyMcpServer } from '../dist/server.js';
import { runAutopsy, resolveAutopsyBinary } from '../dist/cli.js';
import { evidenceEnvelope } from '../dist/evidence.js';

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

test('createAutopsyMcpServer registers all 9 read-only tools and resources', () => {
  const server = createAutopsyMcpServer();
  assert.ok(server, 'McpServer instance must be created');

  const tools = Object.keys(server._registeredTools);
  assert.ok(tools.includes('autopsy_status'), 'Must register autopsy_status');
  assert.ok(tools.includes('autopsy_impact'), 'Must register autopsy_impact');
  assert.ok(tools.includes('autopsy_diff'), 'Must register autopsy_diff');
  assert.ok(tools.includes('autopsy_verify'), 'Must register autopsy_verify');
  assert.ok(tools.includes('autopsy_explain'), 'Must register autopsy_explain');
  for (const tool of [
    'autopsy_temporal',
    'autopsy_test_selection',
    'autopsy_contract_diff',
    'autopsy_distributed_blast_radius',
  ]) {
    assert.ok(tools.includes(tool), `Must register ${tool}`);
  }
  assert.equal(tools.length, 9, 'Must register exactly 5 mandatory plus 4 Phase 4.2 tools');

  const resources = Object.keys(server._registeredResources);
  assert.ok(resources.includes('autopsy://graph'), 'Must register autopsy://graph resource');

  const templates = Object.keys(server._registeredResourceTemplates || {});
  assert.ok(templates.includes('autopsy_graph_symbols'), 'Must register paginated symbols template');
  assert.ok(templates.includes('autopsy_graph_edges'), 'Must register paginated edges template');
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
  const result = await runAutopsy('temporal', ['--max-commits', '100000'], { repoRoot });
  assert.equal(result.exitCode, 0, result.rawStderr);
  assert.ok(result.data.stats.total > 0, 'the workspace has history to replay');
  assert.ok(Array.isArray(result.data.journal), 'the journal projection is embedded verbatim');
  assert.ok(
    result.data.notes.some((note) => note.includes('not which symbols')),
    'the extractor granularity is stated in the machine-readable output'
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
