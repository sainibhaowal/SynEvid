import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAutopsyMcpServer } from '../dist/server.js';
import { runAutopsy, resolveAutopsyBinary } from '../dist/cli.js';

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

test('createAutopsyMcpServer registers all 5 mandatory read-only tools and resources', () => {
  const server = createAutopsyMcpServer();
  assert.ok(server, 'McpServer instance must be created');

  const tools = Object.keys(server._registeredTools);
  assert.ok(tools.includes('autopsy_status'), 'Must register autopsy_status');
  assert.ok(tools.includes('autopsy_impact'), 'Must register autopsy_impact');
  assert.ok(tools.includes('autopsy_diff'), 'Must register autopsy_diff');
  assert.ok(tools.includes('autopsy_verify'), 'Must register autopsy_verify');
  assert.ok(tools.includes('autopsy_explain'), 'Must register autopsy_explain');
  assert.equal(tools.length, 5, 'Must register exactly 5 mandatory tools');

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
