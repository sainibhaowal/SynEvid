import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const TEST_DIR = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(TEST_DIR, '..');
const REPO_ROOT = path.resolve(WEB_DIR, '../..');
const ELECTRON_PACKAGE = path.join(REPO_ROOT, 'apps', 'desktop-electron', 'node_modules', 'electron');
const ELECTRON_RUNNER = path.join(TEST_DIR, 'phase3_browser_runner.cjs');
const NEXT_BIN = path.join(WEB_DIR, 'node_modules', 'next', 'dist', 'bin', 'next');
const require = createRequire(import.meta.url);

const OID_A = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const OID_B = 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb';

function reservePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
  });
}

function sendJson(response, value, status = 200) {
  const body = JSON.stringify(value);
  response.writeHead(status, {
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Headers': 'content-type',
    'Access-Control-Allow-Methods': 'GET,POST,OPTIONS',
    'Content-Type': 'application/json',
    'Content-Length': Buffer.byteLength(body),
  });
  response.end(body);
}

function frame(oid, summary) {
  return {
    oid,
    short_oid: oid.slice(0, 7),
    summary,
    timestamp: 1704067200,
    dimensions: '3D',
    node_count: 1,
    edge_count: 0,
    nodes: [{
      id: `file:${oid}`,
      x: 0,
      y: 0,
      z: 0,
      cluster_id: 0,
      diff_status: 'unchanged',
      file_path: 'src/app.ts',
      kind: 'file',
      start_line: 1,
      end_line: 1,
    }],
    edges: [],
    added_files: [],
    modified_files: [],
    deleted_files: [],
    coverage_state: 'Verified',
    result_digest: `blake3:${oid.slice(0, 16)}`,
  };
}

async function startFixtureApi(port) {
  const calls = [];
  const server = http.createServer((request, response) => {
    const url = new URL(request.url || '/', `http://127.0.0.1:${port}`);
    calls.push({ method: request.method, path: `${url.pathname}${url.search}` });

    if (request.method === 'OPTIONS') {
      response.writeHead(204, {
        'Access-Control-Allow-Origin': '*',
        'Access-Control-Allow-Headers': 'content-type',
        'Access-Control-Allow-Methods': 'GET,POST,OPTIONS',
      });
      response.end();
      return;
    }

    if (url.pathname === '/api/v1/git/galaxy') {
      sendJson(response, {
        repo_id: 'browser-fixture',
        head_oid: OID_B,
        active_branch: 'main',
        total_commits: 2,
        commits: [
          {
            oid: OID_B,
            short_oid: OID_B.slice(0, 7),
            parents: [OID_A],
            author_name: 'Fixture',
            author_email: 'fixture@synevid.test',
            timestamp: 1704067200,
            summary: 'head change',
            lane: 0,
            is_head: true,
            branches: ['main'],
            tags: [],
            symbols_modified: 1,
            blast_radius_score: 1,
            heuristic_verdict: 'PASS',
            semantic_verdict: 'UNKNOWN',
            coverage_state: 'unknown',
            evidence_kind: 'file_heuristic',
            verdict: 'UNKNOWN',
          },
          {
            oid: OID_A,
            short_oid: OID_A.slice(0, 7),
            parents: [],
            author_name: 'Fixture',
            author_email: 'fixture@synevid.test',
            timestamp: 1704067100,
            summary: 'baseline',
            lane: 0,
            is_head: false,
            branches: [],
            tags: ['v1.0.0'],
            symbols_modified: 0,
            blast_radius_score: 0,
            heuristic_verdict: 'PASS',
            semantic_verdict: 'UNKNOWN',
            coverage_state: 'unknown',
            evidence_kind: 'file_heuristic',
            verdict: 'UNKNOWN',
          },
        ],
        branches: [{ name: 'main', target_oid: OID_B, is_active: true }],
        tags: [{ name: 'v1.0.0', target_oid: OID_A }],
      });
      return;
    }

    if (url.pathname.startsWith('/api/v1/git/time-travel/')) {
      const oid = url.pathname.split('/').at(-1);
      // Keep the first frame pending long enough for the test to exercise the
      // Galaxy controls before the normal navigation-to-canvas callback runs.
      setTimeout(() => sendJson(response, oid === OID_A ? frame(OID_A, 'baseline') : frame(OID_B, 'head change')), 8000);
      return;
    }

    if (url.pathname === `/api/v1/git/diff/${OID_A}/${OID_B}`) {
      sendJson(response, {
        base_oid: OID_A,
        head_oid: OID_B,
        added_files: [],
        modified_files: ['src/app.ts'],
        deleted_files: [],
        total_changed_files: 1,
        blast_radius_score: 1,
        verdict: 'PASS',
        drifted_contracts: [],
        coverage_state: 'Verified',
      });
      return;
    }

    if (url.pathname === '/api/v1/git/status') {
      sendJson(response, {
        branch: 'main',
        head_oid: OID_B,
        staged: [],
        unstaged: [],
        untracked: [],
        deleted: [],
        ignored: [],
        is_clean: true,
        total_uncommitted: 0,
      });
      return;
    }

    if (url.pathname === '/api/status') {
      sendJson(response, {
        snapshot_id: 'browser-fixture',
        repo_root: 'browser-fixture',
        repo_name: 'browser-fixture',
        active_repo_id: 'browser-fixture',
        coverage_state: 'Verified',
        files_scanned: 1,
        symbol_count: 1,
        edge_count: 0,
        version: 'v0.4.0',
      });
      return;
    }

    if (url.pathname === '/api/v1/repos/list') {
      sendJson(response, []);
      return;
    }

    if (url.pathname === '/api/v1/capabilities') {
      sendJson(response, {});
      return;
    }

    if (url.pathname === '/__calls') {
      sendJson(response, calls);
      return;
    }

    sendJson(response, {});
  });
  await new Promise((resolve) => server.listen(port, '127.0.0.1', resolve));
  return { server, calls };
}

async function waitForHttp(url, timeoutMs = 60_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.status < 500) return;
    } catch {
      // Next is still compiling; retry below.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

function runElectron(nextOrigin, fixturePort) {
  const electronExecutable = require(ELECTRON_PACKAGE);
  return new Promise((resolve, reject) => {
    const environment = {
      ...process.env,
      SYNEVID_BROWSER_NEXT_ORIGIN: nextOrigin,
      SYNEVID_BROWSER_FIXTURE_PORT: String(fixturePort),
    };
    delete environment.ELECTRON_RUN_AS_NODE;
    const electronArgs = process.platform === 'linux' && process.env.CI
      ? ['--no-sandbox', ELECTRON_RUNNER]
      : [ELECTRON_RUNNER];
    const child = spawn(electronExecutable, electronArgs, {
      cwd: REPO_ROOT,
      env: environment,
      stdio: ['ignore', 'pipe', 'pipe'],
      windowsHide: true,
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk.toString(); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
    child.on('error', reject);
    child.on('exit', (code) => {
      const line = stdout.split(/\r?\n/).find((entry) => entry.startsWith('SYNEVID_BROWSER_RESULT='));
      if (line) {
        resolve(JSON.parse(line.slice('SYNEVID_BROWSER_RESULT='.length)));
      } else {
        reject(new Error(`Electron browser runner exited ${code}: ${stderr || stdout}`));
      }
    });
  });
}

test('Phase 3 browser behavior: slider and range comparison call real API routes', { timeout: 180_000 }, async () => {
  assert.ok(require(ELECTRON_PACKAGE), 'source Electron runtime must be installed');
  assert.ok(NEXT_BIN, 'Next source runtime must be present');

  const nextPort = await reservePort();
  const fixturePort = await reservePort();
  const fixture = await startFixtureApi(fixturePort);
  const next = spawn(process.execPath, [NEXT_BIN, 'dev', '-p', String(nextPort), '-H', '127.0.0.1'], {
    cwd: WEB_DIR,
    env: {
      ...process.env,
      NODE_ENV: 'development',
      NEXT_TELEMETRY_DISABLED: '1',
      SYNEVID_NEXT_DIST_DIR: `.next-browser-${nextPort}`,
    },
    stdio: 'ignore',
    windowsHide: true,
  });

  try {
    await waitForHttp(`http://127.0.0.1:${nextPort}/`);
    const result = await runElectron(`http://127.0.0.1:${nextPort}/`, fixturePort);
    assert.equal(result.sliderSelectedBaseline, true);
    assert.equal(result.rangeComparisonLoaded, true);
    assert.ok(result.calls.some((call) => call.path.startsWith('/api/v1/git/time-travel/')));
    assert.ok(result.calls.some((call) => call.path === `/api/v1/git/time-travel/${OID_A}?dim=3d`));
    assert.ok(result.calls.some((call) => call.path === `/api/v1/git/diff/${OID_A}/${OID_B}`));
  } finally {
    fixture.server.close();
    next.kill();
    fs.rmSync(path.join(WEB_DIR, `.next-browser-${nextPort}`), { recursive: true, force: true });
  }
});
