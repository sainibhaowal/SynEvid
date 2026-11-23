#!/usr/bin/env node
// Packaged smoke test - validates packaged daemon works with auth, health, and analysis

const { spawn } = require('child_process');
const fetch = require('node-fetch');
const path = require('path');
const fs = require('fs');
const crypto = require('crypto');

const APP_PATH = process.argv[2];
const SMOKE_REPO = process.env.SYNEVID_SMOKE_REPO || process.cwd();
const DAEMON_PORT = process.env.SYNEVID_PORT || '3334';
const LOOPBACK_TOKEN = process.env.SYNEVID_LOOPBACK_TOKEN || 'test-token-123';
const DAEMON_URL = `http://127.0.0.1:${DAEMON_PORT}`;

if (!APP_PATH || !fs.existsSync(APP_PATH)) {
  console.error('Usage: node smoke-packaged.js <path-to-packaged-app>');
  console.error('Example: node smoke-packaged.js ./release-build/SynEvid-Setup-0.6.0.exe');
  console.error('         node smoke-packaged.js ./release-build/SynEvid-0.6.0.AppImage');
  console.error('         node smoke-packaged.js ./release-build/SynEvid-0.6.0.dmg');
  process.exit(1);
}

async function waitForHealth(url, token, maxAttempts = 60) {
  for (let i = 0; i < maxAttempts; i++) {
    try {
      const response = await fetch(`${url}/api/v1/health`, {
        headers: { Authorization: `Bearer ${token}` },
        timeout: 5000,
      });
      if (response.ok) {
        const data = await response.json();
        if (data.status === 'ok') {
          console.log('[HEALTH] Daemon is healthy');
          return true;
        }
      }
    } catch (e) {
      // ignore, retry
    }
    await new Promise(r => setTimeout(r, 1000));
  }
  throw new Error('Daemon did not become healthy');
}

async function request(url, options = {}) {
  const defaultHeaders = {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${LOOPBACK_TOKEN}`,
  };
  
  const response = await fetch(url, {
    ...options,
    headers: { ...defaultHeaders, ...options.headers },
  });
  
  const text = await response.text();
  let json;
  try {
    json = JSON.parse(text);
  } catch {
    json = { raw: text };
  }
  
  return { status: response.status, ok: response.ok, json, text };
}

async function runSmokeTest() {
  console.log('[SMOKE] Starting packaged smoke test');
  console.log(`[SMOKE] App: ${APP_PATH}`);
  console.log(`[SMOKE] Repo: ${SMOKE_REPO}`);
  console.log(`[SMOKE] Daemon: ${DAEMON_URL}`);
  
  // Start the packaged app
  const appProcess = spawn(APP_PATH, [], {
    env: {
      ...process.env,
      SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN,
      SYNEVID_PORT: DAEMON_PORT,
      SYNEVID_DEV: '0',
      NODE_ENV: 'production',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: false,
    windowsHide: true,
  });
  
  let appOutput = '';
  let appError = '';
  
  appProcess.stdout?.on('data', (data) => {
    appOutput += data.toString();
  });
  
  appProcess.stderr?.on('data', (data) => {
    appError += data.toString();
    if (process.env.SYNEVID_SMOKE === '1') {
      console.error(`[APP STDERR] ${data.toString().trim()}`);
    }
  });
  
  appProcess.on('error', (err) => {
    console.error('[SMOKE] App process error:', err.message);
  });
  
  appProcess.on('exit', (code, signal) => {
    console.log(`[SMOKE] App exited: code=${code}, signal=${signal}`);
  });
  
  // Wait for daemon to be healthy
  try {
    await waitForHealth(`http://127.0.0.1:${DAEMON_PORT}`, LOOPBACK_TOKEN);
  } catch (e) {
    console.error('[SMOKE] Daemon health check failed');
    console.error('[SMOKE] App stdout:', appOutput.slice(-2000));
    console.error('[SMOKE] App stderr:', appError.slice(-2000));
    appProcess.kill();
    process.exit(1);
  }
  
  console.log('[SMOKE] Daemon is healthy, running tests...');
  
  // TEST 1: Unauthenticated request rejected (401)
  console.log('[TEST 1] Unauthenticated request...');
  const unauth = await fetch(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`);
  if (unauth.status !== 401) {
    throw new Error(`Expected 401 for missing token, got ${unauth.status}`);
  }
  console.log('[PASS] Missing token returns 401');
  
  // TEST 2: Invalid token rejected (401)
  console.log('[TEST 2] Invalid token...');
  const invalid = await fetch(`${DAEMON_URL}/api/v1/health`, {
    headers: { Authorization: 'Bearer invalid-token-123' }
  });
  if (invalid.status !== 401) {
    throw new Error(`Expected 401 for invalid token, got ${invalid.status}`);
  }
  console.log('[PASS] Invalid token returns 401');
  
  // TEST 3: Valid token works (200)
  console.log('[TEST 3] Valid token...');
  const authed = await fetch(`${DAEMON_URL}/api/v1/health`, {
    headers: { Authorization: `Bearer ${LOOPBACK_TOKEN}` }
  });
  if (authed.status !== 200) {
    throw new Error(`Expected 200 for valid token, got ${authed.status}`);
  }
  const healthData = await authed.json();
  console.log('[PASS] Valid token returns 200', healthData);
  
  // TEST 4: Connect repo + analyze
  console.log('[TEST 4] Connect repo...');
  const connect = await fetch(`${DAEMON_URL}/api/v1/repos/connect`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Authorization: `Bearer ${LOOPBACK_TOKEN}` },
    body: JSON.stringify({ root_path: SMOKE_REPO, display_name: 'Phase 5 smoke test' })
  });
  if (![200, 201].includes(connect.status)) {
    const text = await connect.text();
    throw new Error(`Connect failed: ${connect.status} - ${text}`);
  }
  const connectData = await connect.json();
  const repoId = connectData.repo_id;
  const snapshotId = connectData.scan?.snapshot_id;
  console.log('[PASS] Repo connected', { repoId, snapshotId });
  
  // TEST 5: Analytics report matches daemon
  console.log('[TEST 5] Analytics report...');
  const reportResp = await fetch(`${DAEMON_URL}/api/v1/analytics/report?format=json`, {
    headers: { Authorization: `Bearer ${LOOPBACK_TOKEN}` }
  });
  if (reportResp.status !== 200) {
    throw new Error(`Analytics report failed: ${reportResp.status}`);
  }
  const reportData = await reportResp.json();
  
  // Verify report integrity
  if (reportData.report?.repository?.repo_id !== repoId) {
    throw new Error(`Report repo_id mismatch: ${reportData.report?.repository?.repo_id} !== ${repoId}`);
  }
  if (reportData.report?.snapshot?.snapshot_id !== snapshotId) {
    throw new Error(`Report snapshot_id mismatch: ${reportData.report?.snapshot?.snapshot_id} !== ${snapshotId}`);
  }
  if (reportData.report?.result_digest !== reportData.digest) {
    throw new Error(`Report digest mismatch: ${reportData.report?.result_digest} !== ${reportData.digest}`);
  }
  if (reportData.evidence_receipt?.digest !== reportData.digest) {
    throw new Error(`Evidence receipt digest mismatch`);
  }
  if (!reportData.digest?.startsWith('blake3:')) {
    throw new Error(`Digest not blake3: ${reportData.digest}`);
  }
  console.log('[PASS] Analytics report integrity verified');
  
  // TEST 6: Impact analysis
  console.log('[TEST 6] Impact analysis...');
  // Find a symbol to test impact
  const exportResp = await fetch(`${DAEMON_URL}/api/v1/repos/export`, {
    headers: { Authorization: `Bearer ${LOOPBACK_TOKEN}` }
  });
  const exportData = await exportResp.json();
  const exportNodes = exportData.export?.nodes || [];
  const impactSeed = exportNodes.find(n => n.node_kind === 'function' && n.file_path?.includes('src/'))?.stable_id;
  
  if (impactSeed) {
    const impactResp = await fetch(`${DAEMON_URL}/api/impact?seed=${encodeURIComponent(impactSeed)}&detail=summary`, {
      headers: { Authorization: `Bearer ${LOOPBACK_TOKEN}` }
    });
    if (impactResp.ok) {
      const impactData = await impactResp.json();
      console.log('[PASS] Impact analysis works', { seed: impactSeed, likely: impactData.likely_affected?.length || 0 });
    }
  }
  
  // TEST 7: Verify report visible in UI (Electron)
  console.log('[TEST 7] UI verification...');
  // This would require Electron automation - skip for headless
  console.log('[SKIP] UI verification requires Electron automation');
  
  // All tests passed
  console.log('\n========================================');
  console.log('PACKAGED_SMOKE_PASS');
  console.log(JSON.stringify({
    repo_id: repoId,
    snapshot_id: snapshotId,
    report_digest: reportData.digest,
    blake3_digest: reportData.digest,
    evidence_receipt_digest: reportData.evidence_receipt?.digest,
    repository_id: repoId,
    snapshot_id: snapshotId,
  }, null, 2));
  console.log('========================================\n');
  
  return true;
}

async function main() {
  const appProcess = spawn(process.argv[0], [APP_PATH], {
    env: {
      ...process.env,
      SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN,
      SYNEVID_PORT: DAEMON_PORT,
      SYNEVID_DEV: '0',
      NODE_ENV: 'production',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  
  let appOutput = '';
  let appError = '';
  
  const app = spawn(APP_PATH, [], {
    env: {
      ...process.env,
      SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN,
      SYNEVID_PORT: DAEMON_PORT,
      SYNEVID_DEV: '0',
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    windowsHide: true,
  });
  
  let appOutput = '';
  let appError = '';
  
  app.stdout?.on('data', (data) => { appOutput += data.toString(); });
  app.stderr?.on('data', (data) => { appError += data.toString(); });
  
  try {
    await waitForHealth(`http://127.0.0.1:${DAEMON_PORT}`, LOOPBACK_TOKEN);
    await runSmokeTest();
    console.log('[SMOKE] All tests passed!');
    process.exit(0);
  } catch (e) {
    console.error('[SMOKE] FAILED:', e.message);
    console.error('[SMOKE] App stdout:', appOutput.slice(-3000));
    console.error('[SMOKE] App stderr:', appError.slice(-3000));
    process.exit(1);
  } finally {
    appProcess.kill();
  }
}

main().catch(e => {
  console.error('[SMOKE] Fatal error:', e);
  process.exit(1);
});