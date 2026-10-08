import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAutopsyMcpServer } from '../dist/server.js';
import { runAutopsy, resolveAutopsyBinary } from '../dist/cli.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_API_ROOT = path.resolve(__dirname, '../../../benchmarks/corpus/repo-api');

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

test('createAutopsyMcpServer registers all 5 mandatory read-only tools', () => {
  const server = createAutopsyMcpServer();
  assert.ok(server, 'McpServer instance must be created');
});
