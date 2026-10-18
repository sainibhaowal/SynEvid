// End-to-end repair-loop session test through the MCP server (roadmap §6).
//
// The TypeScript-side counterpart of the gate's proposed `e2e_agent_session.rs`:
// spawns `dist/index.js` over stdio and drives `autopsy_session` the way an agent
// does — start, record each evidence step, close — then asserts the verdicts
// mirror the recorded engine exit codes and that receipts are reproducible
// (BLAKE3 over normalized root + counter; no wall-clock anywhere).

import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { callTool, freshWorkspaceCopy, startMcpServer } from './mcp_stdio.js';

const TIMESTAMP = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/;

test('a failing repair loop closes at fail and mirrors exit 2 through MCP', async (t) => {
  const workspace = freshWorkspaceCopy('synevid-mcp-session-fail-');
  const client = await startMcpServer({ cwd: workspace });
  t.after(async () => {
    await client.close();
    fs.rmSync(workspace, { recursive: true, force: true });
  });

  // Open the session.
  const start = await callTool(client, 'autopsy_session', { action: 'start', repo_root: workspace });
  assert.equal(start.envelope.command, 'session');
  assert.equal(start.envelope.exit_code, 0);
  assert.equal(start.envelope.report.status, 'open');
  const sessionId = start.envelope.report.session_id;
  assert.match(sessionId, /^[0-9a-f]{32}$/, 'deterministic 32-hex session id');

  // Record the failing evidence step: a strict verify that the policy rejected.
  const recorded = await callTool(client, 'autopsy_session', {
    action: 'record',
    session_id: sessionId,
    invoked: ['verify', '--strict'],
    exit_code: 2,
    repo_root: workspace,
  });
  assert.equal(recorded.envelope.report.recorded, true);
  assert.equal(recorded.envelope.report.step_index, 0);

  // Close: the verdict is derived from the recorded step, never invented.
  const closed = await callTool(client, 'autopsy_session', {
    action: 'close',
    session_id: sessionId,
    repo_root: workspace,
  });
  assert.equal(closed.envelope.report.verdict, 'fail');
  assert.equal(closed.envelope.report.evidence_exit_code, 2);
  assert.equal(closed.envelope.exit_code, 2, 'the envelope mirrors the verdict');
  assert.equal(closed.envelope.actionable, false);

  // Show: the receipt is byte-identical across reads and carries no wall-clock.
  const show1 = await callTool(client, 'autopsy_session', {
    action: 'show',
    session_id: sessionId,
    repo_root: workspace,
  });
  const show2 = await callTool(client, 'autopsy_session', {
    action: 'show',
    session_id: sessionId,
    repo_root: workspace,
  });
  assert.equal(show1.text, show2.text, 'receipts are byte-identical across reads');
  assert.equal(show1.envelope.report.receipt.verdict, 'fail');
  assert.ok(!TIMESTAMP.test(show1.text), 'no wall-clock timestamps in the receipt');

  // A closed receipt is final: recording another step is refused with advice.
  const late = await callTool(client, 'autopsy_session', {
    action: 'record',
    session_id: sessionId,
    invoked: ['verify'],
    exit_code: 0,
    repo_root: workspace,
  });
  assert.equal(late.envelope.exit_code, 3, 'a closed receipt cannot be reopened');
  assert.ok(late.envelope.repair, 'the refusal carries the repair triple');
});

test('a passing loop closes at pass and the store is byte-reproducible', async (t) => {
  const workspace = freshWorkspaceCopy('synevid-mcp-session-pass-');
  const client = await startMcpServer({ cwd: workspace });
  t.after(async () => {
    await client.close();
    fs.rmSync(workspace, { recursive: true, force: true });
  });

  const runLoop = async () => {
    const start = await callTool(client, 'autopsy_session', { action: 'start', repo_root: workspace });
    assert.equal(start.envelope.exit_code, 0);
    const sessionId = start.envelope.report.session_id;

    const recorded = await callTool(client, 'autopsy_session', {
      action: 'record',
      session_id: sessionId,
      invoked: ['verify'],
      exit_code: 0,
      repo_root: workspace,
    });
    assert.equal(recorded.envelope.report.recorded, true);

    const closed = await callTool(client, 'autopsy_session', {
      action: 'close',
      session_id: sessionId,
      repo_root: workspace,
    });
    assert.equal(closed.envelope.report.verdict, 'pass');
    assert.equal(closed.envelope.exit_code, 0);
    assert.equal(closed.envelope.actionable, true);
    return sessionId;
  };

  const storePath = path.join(workspace, '.autopsy', 'sessions.json');

  const firstId = await runLoop();
  const firstBytes = fs.readFileSync(storePath, 'utf8');

  // Wipe the store and replay the identical loop: the id and the stored bytes
  // must reproduce exactly — determinism is the point of the receipt.
  fs.rmSync(storePath);
  const secondId = await runLoop();
  const secondBytes = fs.readFileSync(storePath, 'utf8');

  assert.equal(secondId, firstId, 'the session id is reproducible across identical stores');
  assert.equal(secondBytes, firstBytes, 'the store bytes are reproducible across identical runs');
  assert.ok(!TIMESTAMP.test(secondBytes), 'no wall-clock timestamps in the store');
});
