import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const TEST_DIR = path.dirname(fileURLToPath(import.meta.url));
const WEB_DIR = path.resolve(TEST_DIR, '..');
const NEXT_BIN = path.join(WEB_DIR, 'node_modules', 'next', 'dist', 'bin', 'next');

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

async function fetchWhenReady(url, child) {
  const deadline = Date.now() + 60_000;
  let lastError = 'Next.js server did not become ready';
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Next.js exited early (${child.exitCode}): ${lastError}`);
    try {
      const response = await fetch(url);
      if (response.status === 200) return response;
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`Timed out waiting for Next.js: ${lastError}`);
}

test('Phase 5 CSP nonce in the response is present on every rendered inline script', { timeout: 90_000 }, async () => {
  assert.ok(fs.existsSync(NEXT_BIN), 'Next.js source runtime must be installed');
  const port = await reservePort();
  const distDir = `.next-phase5-csp-${port}`;
  const child = spawn(process.execPath, [NEXT_BIN, 'dev', '-p', String(port), '-H', '127.0.0.1'], {
    cwd: WEB_DIR,
    env: {
      ...process.env,
      NODE_ENV: 'development',
      NEXT_TELEMETRY_DISABLED: '1',
      SYNEVID_NEXT_DIST_DIR: distDir,
    },
    stdio: 'ignore',
    windowsHide: true,
  });
  try {
    const response = await fetchWhenReady(`http://127.0.0.1:${port}/`, child);
    const csp = response.headers.get('content-security-policy') || '';
    const scriptDirective = csp.split(';').map((part) => part.trim()).find((part) => part.startsWith('script-src ')) || '';
    const nonce = scriptDirective.match(/'nonce-([^']+)'/)?.[1];
    assert.ok(nonce, 'script-src must carry a per-response nonce');
    assert.match(scriptDirective, /'strict-dynamic'/);
    assert.doesNotMatch(scriptDirective, /'unsafe-inline'/);
    assert.doesNotMatch(csp, /https?:\/\/(?!localhost|127\.0\.0\.1)/);

    const html = await response.text();
    const scripts = [...html.matchAll(/<script\b([^>]*)>/gi)];
    assert.ok(scripts.length > 0, 'rendered page must contain framework/application scripts');
    for (const [, attributes] of scripts) {
      assert.equal(attributes.match(/\bnonce="([^"]+)"/)?.[1], nonce, 'each rendered script must match the response CSP nonce');
    }
  } finally {
    if (child.exitCode === null) child.kill();
    await new Promise((resolve) => {
      if (child.exitCode !== null) resolve();
      else child.once('exit', resolve);
    });
    fs.rmSync(path.join(WEB_DIR, distDir), { recursive: true, force: true, maxRetries: 8, retryDelay: 250 });
  }
});
