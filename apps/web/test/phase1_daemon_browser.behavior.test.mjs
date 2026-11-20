import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const TEST_DIR = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(TEST_DIR, '..');
const REPO_ROOT = path.resolve(WEB_DIR, '../..');
const ELECTRON_PACKAGE = path.join(REPO_ROOT, 'apps', 'desktop-electron', 'node_modules', 'electron');
const ELECTRON_RUNNER = path.join(TEST_DIR, 'phase1_daemon_browser_runner.cjs');
const NEXT_BIN = path.join(WEB_DIR, 'node_modules', 'next', 'dist', 'bin', 'next');
const require = createRequire(import.meta.url);

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

function waitForHttp(url, timeoutMs = 60_000) {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + timeoutMs;
    const attempt = () => {
      const request = http.get(url, (response) => {
        response.resume();
        if (response.statusCode && response.statusCode < 500) {
          resolve();
          return;
        }
        retry();
      });
      request.on('error', retry);
    };
    const retry = () => {
      if (Date.now() >= deadline) {
        reject(new Error(`Timed out waiting for ${url}`));
        return;
      }
      setTimeout(attempt, 150);
    };
    attempt();
  });
}

function normalizePath(value) {
  const normalized = fs.realpathSync(value).replaceAll('\\', '/');
  return normalized.startsWith('//?/') ? normalized.slice(4) : normalized;
}

function daemonBinary() {
  const suffix = process.platform === 'win32' ? '.exe' : '';
  return path.join(REPO_ROOT, 'target', 'debug', `synevid-ui${suffix}`);
}

async function startDaemon(rootPath, port) {
  const child = spawn(daemonBinary(), [
    '--host', '127.0.0.1',
    '--port', String(port),
    '--repo-root', rootPath,
  ], {
    cwd: REPO_ROOT,
    stdio: 'ignore',
    windowsHide: true,
  });
  await waitForHttp(`http://127.0.0.1:${port}/api/v1/health`);
  return child;
}

function stopProcess(child) {
  if (!child || child.exitCode !== null) return;
  child.kill();
}

function runElectron(nextOrigin, daemonPort, rootPath, expectedSeq, publishEvent) {
  const electronExecutable = require(ELECTRON_PACKAGE);
  return new Promise((resolve, reject) => {
    const environment = {
      ...process.env,
      SYNEVID_BROWSER_NEXT_ORIGIN: nextOrigin,
      SYNEVID_BROWSER_DAEMON_PORT: String(daemonPort),
      SYNEVID_BROWSER_ROOT_PATH: rootPath,
      SYNEVID_BROWSER_EXPECTED_SEQ: String(expectedSeq),
      SYNEVID_BROWSER_PUBLISH_EVENT: publishEvent ? '1' : '0',
    };
    delete environment.ELECTRON_RUN_AS_NODE;
    const child = spawn(electronExecutable, [ELECTRON_RUNNER], {
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
      const line = stdout.split(/\r?\n/).find((entry) => entry.startsWith('SYNEVID_PHASE1_BROWSER_RESULT='));
      if (line) {
        resolve(JSON.parse(line.slice('SYNEVID_PHASE1_BROWSER_RESULT='.length)));
      } else {
        reject(new Error(`Electron phase1 browser runner exited ${code}: ${stderr || stdout}`));
      }
    });
  });
}

async function waitForPersistedEvent(port, seq) {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const response = await fetch(`http://127.0.0.1:${port}/api/v1/live/poll?since_seq=0`);
    const payload = await response.json();
    if (payload.events?.some((event) => event.seq_id === seq)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`event ${seq} was not visible in the daemon replay endpoint`);
}

test('Phase 1 real Electron browser workflow preserves repository identity and replay cursor across daemon restart', { timeout: 240_000 }, async () => {
  assert.ok(fs.existsSync(daemonBinary()), 'build synevid-ui before running the real browser workflow');
  assert.ok(require(ELECTRON_PACKAGE), 'source Electron runtime must be installed');
  assert.ok(fs.existsSync(NEXT_BIN), 'Next source runtime must be installed');

  // The production UI intentionally hides repositories whose display name is
  // SynEvid itself; keep this test repository clearly user-owned.
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'phase1-browser-real-'));
  fs.mkdirSync(path.join(root, 'src'));
  fs.writeFileSync(path.join(root, 'src', 'main.ts'), 'export function browserProof(value: number) { return value + 1; }\n');
  const canonicalRoot = normalizePath(root);
  const daemonPort = await reservePort();
  const nextPort = await reservePort();
  const next = spawn(process.execPath, [NEXT_BIN, 'dev', '-p', String(nextPort), '-H', '127.0.0.1'], {
    cwd: WEB_DIR,
    env: { ...process.env, NODE_ENV: 'development', NEXT_TELEMETRY_DISABLED: '1', SYNEVID_NEXT_DIST_DIR: `.next-phase1-${nextPort}` },
    stdio: 'ignore',
    windowsHide: true,
  });
  let daemon;
  let restarted;
  try {
    daemon = await startDaemon(root, daemonPort);
    await waitForHttp(`http://127.0.0.1:${nextPort}/`);
    const first = await runElectron(`http://127.0.0.1:${nextPort}/`, daemonPort, canonicalRoot, 0, true);
    assert.equal(first.canonicalPathVisible, true);
    assert.equal(first.replayedSequenceVisible, true);
    assert.equal(first.repositoryFilesVisible, true);
    assert.equal(first.graphUnavailable, false);
    assert.ok(first.publishedSeq > 0);
    await waitForPersistedEvent(daemonPort, first.publishedSeq);

    stopProcess(daemon);
    await new Promise((resolve) => setTimeout(resolve, 250));
    restarted = await startDaemon(root, daemonPort);
    const second = await runElectron(`http://127.0.0.1:${nextPort}/`, daemonPort, canonicalRoot, first.publishedSeq, false);
    assert.equal(second.canonicalPathVisible, true);
    assert.equal(second.replayedSequenceVisible, true);
    assert.equal(second.repositoryFilesVisible, true);
    assert.equal(second.graphUnavailable, false);
  } finally {
    stopProcess(daemon);
    stopProcess(restarted);
    stopProcess(next);
    fs.rmSync(path.join(WEB_DIR, `.next-phase1-${nextPort}`), { recursive: true, force: true });
    fs.rmSync(root, { recursive: true, force: true });
  }
});
