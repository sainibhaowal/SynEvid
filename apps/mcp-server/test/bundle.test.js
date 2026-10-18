// Release-artifact test: `dist/index.bundle.js` is the single-file MCP server
// that ships inside the platform archives (see .github/workflows/post-merge.yml
// and scripts/install.sh). Users install that file directly — no npm, no tsc —
// so it must be verified as-is, not just the TypeScript sources.

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { callTool, startMcpServer, REPO_API_ROOT } from './mcp_stdio.js';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BUNDLE_ENTRY = path.resolve(HERE, '..', 'dist', 'index.bundle.js');

test('the shipped bundle (dist/index.bundle.js) serves the full tool surface', async (t) => {
  assert.ok(
    fs.existsSync(BUNDLE_ENTRY),
    `bundle missing: ${BUNDLE_ENTRY} (run: npm run build:bundle)`,
  );
  const client = await startMcpServer({ entry: BUNDLE_ENTRY, cwd: REPO_API_ROOT });
  t.after(() => client.close());

  const listed = await client.request('tools/list');
  assert.equal(listed.tools.length, 11, 'every tool must ship in the release bundle');

  // Exercise real tool calls through the bundle: the point of the packaged
  // artifact is that an installed agent host reaches the engine end-to-end.
  const find = await callTool(client, 'autopsy_find', {
    pattern: 'UserService.createUser',
    repo_root: REPO_API_ROOT,
  });
  assert.equal(find.isError, false, find.text);
  assert.equal(find.envelope.command, 'find');
  assert.equal(find.envelope.exit_code, 0);
  assert.equal(
    find.envelope.report.symbols[0].stable_id,
    'src/services/userService.ts#method:UserService.createUser',
  );

  // And the digest-bearing path must survive bundling untouched.
  const verify = await callTool(client, 'autopsy_verify', { repo_root: REPO_API_ROOT });
  assert.equal(verify.envelope.exit_code, 0);
  assert.ok(verify.envelope.report.result_digest, 'digest-bearing answers must survive bundling');
});
