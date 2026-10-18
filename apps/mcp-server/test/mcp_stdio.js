// Minimal MCP stdio client for protocol-level end-to-end tests (roadmap §6:
// "exercises it through the MCP server, not just the CLI").
//
// The MCP stdio transport is newline-delimited JSON-RPC 2.0 — one JSON object per
// line, no Content-Length headers. This client speaks exactly that and spawns the
// real `dist/index.js` server, the same process an agent host starts, so the tests
// assert the wire protocol rather than calling tool handlers in-process.

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
export const SERVER_ENTRY = path.resolve(HERE, '..', 'dist', 'index.js');
export const WORKSPACE_ROOT = path.resolve(HERE, '..', '..', '..');
export const REPO_API_ROOT = path.join(WORKSPACE_ROOT, 'benchmarks', 'corpus', 'repo-api');

/**
 * Pins the spawned server to the workspace build when present: the resolver
 * otherwise prefers a possibly stale `~/.cargo/bin` install, which would
 * validate old behavior instead of the code under test.
 */
function pinnedBinary() {
  const candidate = path.join(WORKSPACE_ROOT, 'target', 'debug', 'autopsy');
  return fs.existsSync(candidate) ? candidate : null;
}

/** A throwaway copy of the corpus fixture, so receipts never touch committed ground truth. */
export function freshWorkspaceCopy(prefix) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), prefix));
  fs.cpSync(REPO_API_ROOT, dir, { recursive: true });
  // Strip any stale session store: the store drives the deterministic session
  // counter, so a leftover file would shift ids on a dirty developer machine.
  fs.rmSync(path.join(dir, '.autopsy', 'sessions.json'), { force: true });
  return dir;
}

/**
 * Spawns the MCP server and completes the protocol handshake.
 * Returns `{request, notify, stderr, close}`.
 */
export async function startMcpServer({ cwd = REPO_API_ROOT } = {}) {
  const binary = pinnedBinary();
  const env = { ...process.env, RUST_BACKTRACE: '0' };
  if (binary) env.AUTOPSY_BIN = binary;

  const child = spawn(process.execPath, [SERVER_ENTRY], {
    cwd,
    env,
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  let stderr = '';
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (chunk) => {
    stderr += chunk;
  });

  let buffer = '';
  let exited = false;
  const pending = new Map();
  let nextId = 1;

  child.on('exit', (code) => {
    exited = true;
    for (const { reject } of pending.values()) {
      reject(new Error(`MCP server exited with code ${code}: ${stderr}`));
    }
    pending.clear();
  });

  child.stdout.setEncoding('utf8');
  child.stdout.on('data', (chunk) => {
    buffer += chunk;
    for (;;) {
      const newline = buffer.indexOf('\n');
      if (newline < 0) break;
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        continue; // not a JSON-RPC frame; the server logs only to stderr anyway
      }
      if (message.id !== undefined && pending.has(message.id)) {
        const { resolve, reject } = pending.get(message.id);
        pending.delete(message.id);
        if (message.error) {
          reject(new Error(`MCP error response: ${JSON.stringify(message.error)}`));
        } else {
          resolve(message.result);
        }
      }
    }
  });

  function send(message) {
    child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  function request(method, params = {}) {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      send({ jsonrpc: '2.0', id, method, params });
    });
  }

  function notify(method, params = {}) {
    send({ jsonrpc: '2.0', method, params });
  }

  await request('initialize', {
    protocolVersion: '2025-06-18',
    capabilities: {},
    clientInfo: { name: 'synevid-e2e', version: '1.0.0' },
  });
  notify('notifications/initialized');

  return {
    request,
    notify,
    get stderr() {
      return stderr;
    },
    async close() {
      if (exited) return;
      child.stdin.end();
      await new Promise((resolve) => {
        const timer = setTimeout(() => {
          child.kill('SIGKILL');
          resolve();
        }, 3000);
        child.once('exit', () => {
          clearTimeout(timer);
          resolve();
        });
      });
    },
  };
}

/**
 * Calls one MCP tool and decodes its answer.
 * Every Synevid tool returns one text content block holding the evidence
 * envelope; `envelope` is that parsed JSON (null when the block is not JSON).
 */
export async function callTool(client, name, args = {}) {
  const result = await client.request('tools/call', { name, arguments: args });
  const text = result?.content?.[0]?.text ?? '';
  let envelope = null;
  try {
    envelope = JSON.parse(text);
  } catch {
    // leave envelope null; callers assert on it for well-formed answers
  }
  return { result, text, envelope, isError: result?.isError === true };
}
