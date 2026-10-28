// End-to-end agent-loop test through the MCP server (roadmap §6).
//
// This file is the TypeScript-side counterpart of the gate's proposed
// `e2e_agent_find_and_envelope.rs`: because the MCP server *is* a TypeScript
// process, the only way to exercise "through the MCP server" literally is to
// speak its wire protocol. These tests spawn `dist/index.js` over stdio and
// drive the loop an agent drives: discover tools -> resolve a partial name ->
// bounded impact -> passing verify, with every answer inside the one envelope.

import test from 'node:test';
import assert from 'node:assert/strict';
import { callTool, startMcpServer, REPO_API_ROOT } from './mcp_stdio.js';

const SEED_STABLE_ID = 'src/services/userService.ts#method:UserService.createUser';

test('an agent reaches an actionable verify in exactly 3 MCP tool calls', async (t) => {
  const client = await startMcpServer();
  t.after(() => client.close());

  // Tool discovery: the surface an agent sees before doing anything.
  const listed = await client.request('tools/list');
  assert.equal(listed.tools.length, 21, '5 mandatory + 4 Phase-4.2 + find + session + 6 Phase-3 Git + 4 Phase-4');
  const names = listed.tools.map((tool) => tool.name);
  for (const expected of [
    'autopsy_status',
    'autopsy_impact',
    'autopsy_diff',
    'autopsy_verify',
    'autopsy_explain',
    'autopsy_find',
    'autopsy_session',
  ]) {
    assert.ok(names.includes(expected), `tools/list must include ${expected}`);
  }

  let toolCalls = 0;

  // Call 1 — resolve a partial, human-typed name to a stable id.
  toolCalls += 1;
  const find = await callTool(client, 'autopsy_find', {
    pattern: 'UserService.createUser',
    repo_root: REPO_API_ROOT,
  });
  assert.equal(find.isError, false, find.text);
  assert.equal(find.envelope.command, 'find');
  assert.equal(find.envelope.exit_code, 0);
  assert.equal(find.envelope.actionable, true);
  assert.equal(find.envelope.report.symbols[0].stable_id, SEED_STABLE_ID);

  // Call 2 — bounded blast radius from the resolved id.
  toolCalls += 1;
  const impact = await callTool(client, 'autopsy_impact', {
    symbol: SEED_STABLE_ID,
    repo_root: REPO_API_ROOT,
  });
  assert.equal(impact.envelope.command, 'impact');
  assert.equal(impact.envelope.exit_code, 0);
  assert.equal(impact.envelope.actionable, true);
  assert.ok(
    impact.envelope.report.impact.impacted_entities.length >= 1,
    'the seed itself is part of its blast radius'
  );

  // Call 3 — the passing verification gate.
  toolCalls += 1;
  const verify = await callTool(client, 'autopsy_verify', { repo_root: REPO_API_ROOT });
  assert.equal(verify.envelope.command, 'verify');
  assert.equal(verify.envelope.exit_code, 0);
  assert.equal(verify.envelope.actionable, true);
  assert.equal(verify.envelope.report.coverage.state, 'verified');

  assert.equal(toolCalls, 3, 'find -> impact -> verify is the whole loop');
});

test('refusals stay inside the envelope with repair advice; no bare error JSON', async (t) => {
  const client = await startMcpServer();
  t.after(() => client.close());

  // Schema-level refusal: an empty pattern never reaches the engine; the tool
  // answers with a named validation message rather than bare error JSON.
  const invalid = await callTool(client, 'autopsy_find', { pattern: '', repo_root: REPO_API_ROOT });
  assert.equal(invalid.isError, true, 'schema violations are flagged on the tool result');
  assert.match(invalid.text, /autopsy_find/, 'the refusal names the tool');
  assert.match(invalid.text, /pattern/, 'the refusal names the offending argument');
  assert.throws(() => JSON.parse(invalid.text), 'validation answers are prose, not raw error objects');

  // Engine-level refusal: a ghost snapshot pin is enveloped at exit 3 with advice.
  const refused = await callTool(client, 'autopsy_find', {
    pattern: 'UserService',
    repo_root: REPO_API_ROOT,
    snapshot_id: 'ghost-snapshot-00000000',
  });
  assert.equal(refused.isError, false, 'a refusal is an evidence answer, not a transport error');
  assert.equal(refused.envelope.command, 'find');
  assert.equal(refused.envelope.exit_code, 3);
  assert.equal(refused.envelope.actionable, false);
  assert.ok(refused.envelope.repair, 'the refusal carries the repair triple');
  assert.equal(typeof refused.envelope.repair.what_failed, 'string');
  assert.equal(typeof refused.envelope.repair.why, 'string');
  assert.equal(typeof refused.envelope.repair.next_action, 'string');
  assert.equal(refused.envelope.error, undefined, 'never bare error JSON');

  // A ghost snapshot pin on impact is also an enveloped exit-3 refusal with advice.
  const ghost = await callTool(client, 'autopsy_impact', {
    symbol: SEED_STABLE_ID,
    repo_root: REPO_API_ROOT,
    snapshot_id: 'ghost-snapshot-00000000',
  });
  assert.equal(ghost.envelope.command, 'impact');
  assert.equal(ghost.envelope.exit_code, 3);
  assert.ok(ghost.envelope.repair, 'ghost-pin refusal carries advice');
  assert.equal(ghost.envelope.error, undefined);
});

test('autopsy://tools resource serves the full deterministic toolspec', async (t) => {
  const client = await startMcpServer();
  t.after(() => client.close());

  const read = await client.request('resources/read', { uri: 'autopsy://tools' });
  const spec = JSON.parse(read.contents[0].text);
  assert.equal(spec.format_version, '0.0.1');
  assert.equal(spec.tools.length, 26, 'every subcommand is listed (incl. init, ui, tui, watch)');
  assert.ok(spec.exit_codes, 'the exit-code contract travels with the spec');
  assert.ok(spec.feedback, 'the feedback channel travels with the spec');
});
