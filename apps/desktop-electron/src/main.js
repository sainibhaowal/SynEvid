const { app, BrowserWindow, ipcMain, dialog, session, Menu, Tray, nativeImage } = require('electron');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const readline = require('readline');
const http = require('http');
const crypto = require('crypto');
const { isTrustedIpcSender, isTrustedRendererUrl } = require('./security');
const { verifyRuntimeManifest } = require('./runtime-manifest');

// In virtualized / WSL2 / multi-GPU environments, avoid crashing GPU child process
// app.commandLine.appendSwitch('force_high_performance_gpu');

let mainWindow = null;
let daemonProcess = null;
let webProcess = null;
let frameMeter = {
  active: false,
  sampleFrames: 0,
  totalFrames: 0,
  sampleStartedAt: 0n,
  fps: null,
  markStartedAt: null,
  lastMarkMs: null,
};
const DAEMON_PORT = Number(process.env.SYNEVID_PORT || 3333);
const COCKPIT_URL = process.env.COCKPIT_URL || 'http://127.0.0.1:3000';
const LIVE_DEVELOPMENT = process.env.SYNEVID_DEV === '1' || process.env.SYNEVID_DEV === 'true';
const LOOPBACK_TOKEN = process.env.SYNEVID_LOOPBACK_TOKEN || crypto.randomBytes(32).toString('hex');
let shuttingDown = false;
let daemonRestartTimer = null;
let daemonRestartAttempts = 0;
let tray = null;
let mcpTokenFilePath = null;

function cockpitRendererUrl() {
  const url = new URL(COCKPIT_URL);
  url.searchParams.set('synevid_daemon', `http://127.0.0.1:${DAEMON_PORT}`);
  return url.toString();
}

// React's development build uses eval() only to reconstruct cross-environment
// call stacks. Keep that capability limited to the source-run development
// shell; packaged/production Electron stays on the strict CSP.
function buildContentSecurityPolicy(liveDevelopment) {
  const developmentEvaluation = liveDevelopment ? " 'unsafe-eval'" : '';
  return [
    "default-src 'self' data: blob: http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001 http://127.0.0.1:3333",
    `script-src 'self'${developmentEvaluation} http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001`,
    "style-src 'self' 'unsafe-inline' http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001",
    "img-src 'self' data: blob: http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001",
    "font-src 'self' data: http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001",
    'connect-src \'self\' http://localhost:* http://127.0.0.1:* ws://localhost:* ws://127.0.0.1:*',
  ].join('; ');
}

function packagedRuntimePath(...parts) {
  return path.join(process.resourcesPath, ...parts);
}

function sourceRepositoryRoot() {
  return path.resolve(__dirname, '../../..');
}

function daemonRepositoryRoot() {
  return app.isPackaged ? app.getPath('userData') : sourceRepositoryRoot();
}

function daemonExecutableCandidates() {
  const executableName = process.platform === 'win32' ? 'synevid-ui.exe' : 'synevid-ui';
  const repoRoot = sourceRepositoryRoot();
  const candidates = [];
  if (app.isPackaged) candidates.push(packagedRuntimePath('sidecars', executableName));
  candidates.push(
    path.join(repoRoot, 'target', 'release', executableName),
    path.join(repoRoot, 'target', 'debug', executableName),
  );
  return candidates;
}

function findPackagedCockpitServer(root) {
  const direct = path.join(root, 'server.js');
  if (fs.existsSync(direct)) return direct;
  if (!fs.existsSync(root)) return null;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const nested = findPackagedCockpitServer(path.join(root, entry.name));
    if (nested) return nested;
  }
  return null;
}

function showDaemonErrorBanner(message) {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  const payload = JSON.stringify({ code: 'ERR-DAEMON-301', message });
  const script = `(() => {
    const detail = ${payload};
    let banner = document.getElementById('synevid-daemon-error');
    if (!banner) {
      banner = document.createElement('div');
      banner.id = 'synevid-daemon-error';
      banner.style.cssText = 'position:fixed;left:16px;right:16px;top:16px;z-index:2147483647;padding:12px 16px;border:1px solid rgba(248,113,113,.7);border-radius:12px;background:#450a0a;color:#fecaca;font:12px ui-monospace,monospace;box-shadow:0 12px 30px rgba(0,0,0,.35)';
      document.body.appendChild(banner);
    }
    banner.textContent = detail.code + ': ' + detail.message + ' — retrying automatically';
  })()`;
  mainWindow.webContents.executeJavaScript(script).catch(() => {});
}

function clearDaemonErrorBanner() {
  if (!mainWindow || mainWindow.isDestroyed()) return;
  mainWindow.webContents.executeJavaScript("document.getElementById('synevid-daemon-error')?.remove()").catch(() => {});
}

function readFrameMeterStats() {
  if (!frameMeter.active || frameMeter.sampleStartedAt === 0n) {
    return {
      active: false,
      fps: null,
      totalFrames: frameMeter.totalFrames,
      presentationLatencyMs: frameMeter.lastMarkMs,
      source: 'Electron webContents presentation events',
    };
  }

  const elapsedMs = Number(process.hrtime.bigint() - frameMeter.sampleStartedAt) / 1_000_000;
  return {
    active: true,
    fps: frameMeter.fps,
    totalFrames: frameMeter.totalFrames,
    elapsedMs: Math.round(elapsedMs * 10) / 10,
    presentationLatencyMs: frameMeter.lastMarkMs,
    source: 'Electron webContents presentation events',
  };
}

function startFrameMeter() {
  const contents = mainWindow?.webContents;
  if (!contents || contents.isDestroyed()) {
    return readFrameMeterStats();
  }
  if (frameMeter.active) {
    return readFrameMeterStats();
  }

  const now = process.hrtime.bigint();
  frameMeter = {
    active: true,
    sampleFrames: 0,
    totalFrames: 0,
    sampleStartedAt: now,
    fps: null,
    markStartedAt: null,
    lastMarkMs: null,
  };
  contents.beginFrameSubscription(false, () => {
    if (!frameMeter.active) return;
    frameMeter.sampleFrames += 1;
    frameMeter.totalFrames += 1;
    if (frameMeter.markStartedAt !== null) {
      frameMeter.lastMarkMs = Number(process.hrtime.bigint() - frameMeter.markStartedAt) / 1_000_000;
      frameMeter.markStartedAt = null;
    }
    const sampleElapsedNs = process.hrtime.bigint() - frameMeter.sampleStartedAt;
    if (sampleElapsedNs >= 1_000_000_000n) {
      frameMeter.fps = (frameMeter.sampleFrames * 1_000_000_000) / Number(sampleElapsedNs);
      frameMeter.sampleFrames = 0;
      frameMeter.sampleStartedAt = process.hrtime.bigint();
    }
  });
  return readFrameMeterStats();
}

function stopFrameMeter() {
  const contents = mainWindow?.webContents;
  if (contents && !contents.isDestroyed() && frameMeter.active) {
    contents.endFrameSubscription();
  }
  frameMeter.active = false;
  frameMeter.sampleFrames = 0;
  frameMeter.sampleStartedAt = 0n;
  frameMeter.markStartedAt = null;
  return readFrameMeterStats();
}

function shutdownChildren() {
  if (shuttingDown) return;
  shuttingDown = true;
  if (daemonRestartTimer) {
    clearTimeout(daemonRestartTimer);
    daemonRestartTimer = null;
  }
  for (const child of [daemonProcess, webProcess]) {
    if (!child) continue;
    try {
      child.kill();
    } catch (error) {
      console.warn('[SynEvid Desktop] Child shutdown warning:', error.message);
    }
  }
  daemonProcess = null;
  webProcess = null;
  if (mcpTokenFilePath) {
    try {
      fs.rmSync(mcpTokenFilePath, { force: true });
    } catch (error) {
      console.warn('[SynEvid Desktop] MCP credential cleanup warning:', error.message);
    }
    mcpTokenFilePath = null;
  }
}

function writeMcpTokenFile() {
  const userDataDir = app.getPath('userData');
  fs.mkdirSync(userDataDir, { recursive: true });
  if (process.platform !== 'win32') fs.chmodSync(userDataDir, 0o700);
  mcpTokenFilePath = path.join(userDataDir, 'mcp-loopback-token');
  const existing = fs.lstatSync(mcpTokenFilePath, { throwIfNoEntry: false });
  if (existing && (!existing.isFile() || existing.isSymbolicLink())) {
    throw new Error('MCP credential path must be a regular file inside the private user-data directory');
  }
  if (existing) fs.rmSync(mcpTokenFilePath);
  fs.writeFileSync(mcpTokenFilePath, `${LOOPBACK_TOKEN}\n`, { encoding: 'utf8', mode: 0o600, flag: 'wx' });
  if (process.platform !== 'win32') fs.chmodSync(mcpTokenFilePath, 0o600);
  return mcpTokenFilePath;
}

function mcpBundlePath() {
  return app.isPackaged
    ? packagedRuntimePath('mcp-server', 'index.bundle.js')
    : path.join(sourceRepositoryRoot(), 'apps', 'mcp-server', 'dist', 'index.bundle.js');
}

function runPackagedMcpImpact(repoPath, symbol, sessionId) {
  return new Promise((resolve, reject) => {
    const cliPath = app.isPackaged
      ? packagedRuntimePath('sidecars', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy')
      : path.join(sourceRepositoryRoot(), 'target', 'debug', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy');
    const child = spawn(process.execPath, [mcpBundlePath(), '--repo', repoPath], {
      cwd: repoPath,
      env: {
        ...process.env,
        ELECTRON_RUN_AS_NODE: '1',
        SYNEVID_DAEMON_URL: `http://127.0.0.1:${DAEMON_PORT}`,
        SYNEVID_MCP_TOKEN_FILE: mcpTokenFilePath || writeMcpTokenFile(),
        AUTOPSY_BIN: cliPath,
      },
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    });
    const lines = readline.createInterface({ input: child.stdout });
    let stderr = '';
    let settled = false;
    const timeout = setTimeout(() => finish(new Error('Packaged MCP impact timed out')), 45_000);
    const finish = (error, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      lines.close();
      child.kill();
      if (error) reject(new Error(`${error.message}${stderr ? `; MCP stderr: ${stderr.slice(-2000)}` : ''}`));
      else resolve(value);
    };
    const send = (message) => child.stdin.write(`${JSON.stringify(message)}\n`);
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('error', (error) => finish(error));
    child.once('exit', (code) => {
      if (!settled) finish(new Error(`MCP process exited before response (code=${code})`));
    });
    lines.on('line', (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        finish(new Error('Packaged MCP emitted invalid JSON-RPC on stdout'));
        return;
      }
      if (message.id === 1) {
        if (message.error) {
          finish(new Error(`MCP initialization failed: ${JSON.stringify(message.error)}`));
          return;
        }
        send({ jsonrpc: '2.0', method: 'notifications/initialized' });
        send({
          jsonrpc: '2.0',
          id: 2,
          method: 'tools/call',
          params: {
            name: 'autopsy_impact',
            arguments: {
              symbol,
              repo_root: repoPath,
              direction: 'backward',
              detail: 'full',
              session_id: sessionId,
            },
          },
        });
      } else if (message.id === 2) {
        if (message.error || message.result?.isError) {
          finish(new Error(`MCP impact was refused: ${JSON.stringify(message.error || message.result)}`));
          return;
        }
        const text = message.result?.content?.find((item) => item.type === 'text')?.text;
        let envelope;
        try {
          envelope = JSON.parse(text || '');
        } catch {
          finish(new Error('MCP impact returned no parseable evidence envelope'));
          return;
        }
        if (envelope.actionable !== true || envelope.trajectory?.live_published !== true ||
            !Number.isInteger(envelope.trajectory?.live_seq) ||
            envelope.trajectory?.trajectory_persisted !== true ||
            !Number.isInteger(envelope.trajectory?.event_seq)) {
          finish(new Error(`MCP impact did not persist authenticated daemon evidence: ${JSON.stringify(envelope)}`));
          return;
        }
        finish(null, envelope);
      }
    });
    send({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        protocolVersion: '2025-06-18',
        capabilities: {},
        clientInfo: { name: 'synevid-packaged-smoke', version: '0.6.0' },
      },
    });
  });
}

function markNextPresentedFrame() {
  if (!frameMeter.active) return readFrameMeterStats();
  frameMeter.markStartedAt = process.hrtime.bigint();
  frameMeter.lastMarkMs = null;
  return readFrameMeterStats();
}

const SPLASH_HTML = `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>SynEvid Desktop Shell</title>
  <style>
    body {
      margin: 0;
      background: #090A0F;
      color: #F8FAFC;
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      height: 100vh;
      user-select: none;
      overflow: hidden;
    }
    .brand {
      display: flex;
      align-items: center;
      gap: 12px;
      margin-bottom: 24px;
    }
    .brand-title {
      font-size: 26px;
      font-weight: 800;
      letter-spacing: -0.5px;
      color: #F8FAFC;
    }
    .badge {
      font-size: 11px;
      font-family: monospace;
      padding: 3px 8px;
      border-radius: 8px;
      background: rgba(0, 240, 255, 0.15);
      border: 1px solid rgba(0, 240, 255, 0.3);
      color: #00F0FF;
      font-weight: 700;
    }
    .status-box {
      background: rgba(15, 23, 42, 0.85);
      border: 1px solid rgba(255, 255, 255, 0.1);
      border-radius: 16px;
      padding: 28px 36px;
      width: 400px;
      text-align: center;
      box-shadow: 0 8px 32px rgba(0, 0, 0, 0.5);
    }
    .spinner {
      width: 40px;
      height: 40px;
      border: 3px solid rgba(0, 240, 255, 0.15);
      border-top-color: #00F0FF;
      border-radius: 50%;
      animation: spin 0.8s linear infinite;
      margin: 0 auto 16px auto;
    }
    @keyframes spin {
      to { transform: rotate(360deg); }
    }
    .status-text {
      font-size: 13px;
      color: #94A3B8;
      font-family: monospace;
      line-height: 1.6;
    }
    .highlight {
      color: #38BDF8;
      font-weight: 600;
    }
  </style>
</head>
<body>
  <div class="brand">
    <span class="brand-title">SYNEVID</span>
      <span class="badge">v0.6.0</span>
  </div>
  <div class="status-box">
    <div class="spinner"></div>
    <div class="status-text">
      Initializing SynEvid Standalone Shell...<br>
      <span class="highlight">Connecting Reactive Daemon &amp; Cockpit</span>
    </div>
  </div>
</body>
</html>`;

// Check health of any HTTP service
function checkServiceHealth(targetUrl, timeoutMs = 1500, headers = {}) {
  return new Promise((resolve) => {
    try {
      const u = new URL(targetUrl);
      const req = http.get(
        {
          hostname: u.hostname === 'localhost' ? '127.0.0.1' : u.hostname,
          port: u.port,
          path: u.pathname,
          headers,
          timeout: timeoutMs,
        },
        (res) => {
          res.resume();
          resolve(res.statusCode >= 200 && res.statusCode < 400);
        }
      );
      req.on('error', () => resolve(false));
      req.on('timeout', () => {
        req.destroy();
        resolve(false);
      });
    } catch {
      resolve(false);
    }
  });
}

function requestJson(targetUrl, { method = 'GET', token, body, timeoutMs = 30_000 } = {}) {
  return new Promise((resolve, reject) => {
    const target = new URL(targetUrl);
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body));
    const headers = {};
    if (token) headers.Authorization = `Bearer ${token}`;
    if (payload) {
      headers['Content-Type'] = 'application/json';
      headers['Content-Length'] = String(payload.length);
    }
    const req = http.request({
      hostname: target.hostname,
      port: target.port,
      path: `${target.pathname}${target.search}`,
      method,
      headers,
      timeout: timeoutMs,
    }, (res) => {
      let responseBody = '';
      res.setEncoding('utf8');
      res.on('data', (chunk) => { responseBody += chunk; });
      res.on('end', () => {
        let json;
        try {
          json = JSON.parse(responseBody);
        } catch (error) {
          reject(new Error(`Expected JSON from ${target.pathname}: ${error.message}`));
          return;
        }
        resolve({ statusCode: res.statusCode || 0, json });
      });
    });
    req.on('error', reject);
    req.on('timeout', () => req.destroy(new Error(`Request timed out: ${target.pathname}`)));
    if (payload) req.write(payload);
    req.end();
  });
}

async function verifyVisibleAnalyticsReceipt(expected) {
  if (!mainWindow || mainWindow.isDestroyed()) throw new Error('Packaged smoke window is unavailable');
  await mainWindow.loadURL(cockpitRendererUrl());
  const deadline = Date.now() + 30_000;
  let lastState = 'cockpit did not render the analytics panel';
  while (Date.now() < deadline) {
    if (!mainWindow || mainWindow.isDestroyed()) throw new Error('Packaged smoke window closed before receipt render');
    const state = await mainWindow.webContents.executeJavaScript(`(() => {
      const panel = document.querySelector('section[aria-label="Analytics reports"]');
      if (!panel) {
        const repositoriesWorkspace = [...document.querySelectorAll('button')]
          .find((button) => button.title.startsWith('Repositories ('));
        if (repositoriesWorkspace && !repositoriesWorkspace.className.includes('bg-cyan-500/15')) {
          repositoriesWorkspace.click();
          return { ready: false, reason: 'opening repositories workspace' };
        }
        return { ready: false, reason: 'analytics panel missing from repositories workspace' };
      }
      const jsonButton = [...panel.querySelectorAll('button')].find((button) => button.textContent.trim().toLowerCase() === 'json');
      if (!jsonButton) return { ready: false, reason: 'JSON report control missing' };
      if (!panel.querySelector('pre')) jsonButton.click();
      const content = panel.querySelector('pre')?.textContent || '';
      if (!content) {
        const reportError = panel.querySelector('[class*="text-rose"]')?.textContent?.trim();
        return { ready: false, reason: reportError || 'report content not rendered', panelText: panel.innerText.slice(-1200) };
      }
      return { ready: true, content };
    })()`, true).catch((error) => ({ ready: false, reason: error.message }));
    if (state.ready) {
      let rendered;
      try {
        rendered = JSON.parse(state.content);
      } catch {
        throw new Error('Visible analytics report was not valid JSON');
      }
      const visibleDigest = rendered.result_digest || rendered.digest;
      if (rendered.repository?.repo_id !== expected.repositoryId ||
          rendered.snapshot?.snapshot_id !== expected.snapshotId ||
          visibleDigest !== expected.digest) {
        const differingFields = Object.keys(expected.report || {}).filter((key) =>
          JSON.stringify(rendered[key]) !== JSON.stringify(expected.report[key]));
        throw new Error(`Visible analytics receipt does not match the daemon result: ${JSON.stringify({
          repository_id: rendered.repository?.repo_id,
          snapshot_id: rendered.snapshot?.snapshot_id,
          digest: visibleDigest,
          expected_repository_id: expected.repositoryId,
          expected_snapshot_id: expected.snapshotId,
          expected_digest: expected.digest,
          differing_report_fields: Object.fromEntries(differingFields.map((key) => [key, {
            visible: JSON.stringify(rendered[key]).slice(0, 800),
            expected: JSON.stringify(expected.report[key]).slice(0, 800),
          }])),
        })}`);
      }
      return;
    }
    lastState = state.reason || lastState;
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`Visible analytics receipt did not become ready: ${lastState}`);
}

async function runPackagedSmokeWorkflow() {
  const runtimeIntegrity = app.isPackaged
    ? verifyRuntimeManifest(process.resourcesPath)
    : null;
  const repoPath = process.env.SYNEVID_SMOKE_REPO;
  if (!repoPath || !fs.statSync(repoPath, { throwIfNoEntry: false })?.isDirectory()) {
    throw new Error('SYNEVID_SMOKE_REPO must point to a real smoke repository');
  }
  const daemonUrl = `http://127.0.0.1:${DAEMON_PORT}`;
  const unauthenticated = await requestJson(`${daemonUrl}/api/v1/health`);
  const invalidToken = await requestJson(`${daemonUrl}/api/v1/health`, { token: 'invalid-smoke-token' });
  if (unauthenticated.statusCode !== 401 || invalidToken.statusCode !== 401) {
    throw new Error(`Daemon auth rejection mismatch: missing=${unauthenticated.statusCode}, invalid=${invalidToken.statusCode}; expected 401 for both`);
  }
  const connected = await requestJson(`${daemonUrl}/api/v1/repos/connect`, {
    method: 'POST',
    token: LOOPBACK_TOKEN,
    body: { root_path: repoPath, display_name: 'Phase 5 packaged smoke fixture' },
  });
  const scan = connected.json.scan;
  if (connected.statusCode < 200 || connected.statusCode >= 300 ||
      connected.json.status !== 'connected' || !connected.json.repo_id ||
      !scan?.snapshot_id || scan.files_scanned < 1) {
    throw new Error(`Packaged repository connect/index failed: ${JSON.stringify(connected.json)}`);
  }

  const reportResponse = await requestJson(`${daemonUrl}/api/v1/analytics/report?format=json`, {
    token: LOOPBACK_TOKEN,
  });
  const reportEnvelope = reportResponse.json;
  const report = reportEnvelope.report;
  if (reportResponse.statusCode < 200 || reportResponse.statusCode >= 300 ||
      reportEnvelope.error_code || !reportEnvelope.digest?.startsWith('blake3:') ||
      !report || report.result_digest !== reportEnvelope.digest ||
      reportEnvelope.evidence_receipt?.digest !== reportEnvelope.blake3_digest ||
      report?.repository?.repo_id !== connected.json.repo_id ||
      report?.snapshot?.snapshot_id !== scan.snapshot_id ||
      !Number.isInteger(report?.structural_health?.total_files) ||
      report.structural_health.total_files < 1) {
    throw new Error(`Packaged analysis/report verification failed: ${JSON.stringify(reportEnvelope)}`);
  }
  const graphResponse = await requestJson(`${daemonUrl}/api/v1/repos/export`, {
    token: LOOPBACK_TOKEN,
  });
  const exportedNodes = graphResponse.json?.export?.nodes;
  const impactSeed = Array.isArray(exportedNodes)
    ? exportedNodes.find((node) =>
      node.file_path === 'src/index.ts' &&
      node.node_kind === 'function' &&
      typeof node.stable_id === 'string' &&
      node.stable_id.length > 0)
    : null;
  if (graphResponse.statusCode < 200 || graphResponse.statusCode >= 300 || !impactSeed) {
    throw new Error(`Packaged smoke could not resolve a function seed from the indexed graph: ${JSON.stringify({
      status: graphResponse.statusCode,
      graph_error: graphResponse.json?.error,
      node_count: Array.isArray(exportedNodes) ? exportedNodes.length : null,
    })}`);
  }
  const sessionStart = await requestJson(`${daemonUrl}/api/v1/forensics/sessions/start`, {
    method: 'POST',
    token: LOOPBACK_TOKEN,
    body: {
      repo_id: connected.json.repo_id,
      baseline_snap_id: scan.snapshot_id,
      actor_kind: 'HUMAN',
      actor_name: 'Phase 5 packaged smoke',
    },
  });
  const sessionId = sessionStart.json?.session_id;
  if (sessionStart.statusCode < 200 || sessionStart.statusCode >= 300 ||
      sessionStart.json?.status !== 'ACTIVE' || typeof sessionId !== 'string' || !sessionId) {
    throw new Error(`Packaged smoke could not start a persisted forensic session: ${JSON.stringify(sessionStart.json)}`);
  }
  const mcpImpact = await runPackagedMcpImpact(repoPath, impactSeed.stable_id, sessionId);
  const mcpConfigResponse = await requestJson(`${daemonUrl}/api/v1/ai/config-snippet?client=cursor`, {
    token: LOOPBACK_TOKEN,
  });
  const configuredMcp = mcpConfigResponse.json?.config?.mcpServers?.synevid;
  if (mcpConfigResponse.statusCode < 200 || mcpConfigResponse.statusCode >= 300 ||
      configuredMcp?.command !== process.execPath ||
      configuredMcp?.args?.[0] !== mcpBundlePath() ||
      configuredMcp?.env?.SYNEVID_DAEMON_URL !== daemonUrl ||
      configuredMcp?.env?.SYNEVID_MCP_TOKEN_FILE !== mcpTokenFilePath ||
      configuredMcp?.env?.AUTOPSY_BIN !== (app.isPackaged
        ? packagedRuntimePath('sidecars', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy')
        : path.join(sourceRepositoryRoot(), 'target', 'debug', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy')) ||
      configuredMcp?.env?.SYNEVID_LOOPBACK_TOKEN) {
    throw new Error(`Packaged MCP configuration is incomplete or exposes its bearer token: ${JSON.stringify(mcpConfigResponse.json)}`);
  }
  // MCP has just persisted an AI-analysis event, which is part of the report's
  // real session ledger. Fetch the final report after that write so the UI and
  // daemon are compared against one identical source state, not a stale digest.
  const finalReportResponse = await requestJson(`${daemonUrl}/api/v1/analytics/report?format=json`, {
    token: LOOPBACK_TOKEN,
  });
  const finalReportEnvelope = finalReportResponse.json;
  if (finalReportResponse.statusCode < 200 || finalReportResponse.statusCode >= 300 ||
      !finalReportEnvelope?.digest?.startsWith('blake3:') ||
      finalReportEnvelope.report?.repository?.repo_id !== connected.json.repo_id ||
      finalReportEnvelope.report?.snapshot?.snapshot_id !== scan.snapshot_id) {
    throw new Error(`Final packaged analytics report did not match the connected snapshot: ${JSON.stringify(finalReportEnvelope)}`);
  }
  await verifyVisibleAnalyticsReceipt({
    repositoryId: connected.json.repo_id,
    snapshotId: scan.snapshot_id,
    digest: finalReportEnvelope.digest,
    report: finalReportEnvelope.report,
  });
  console.log('PACKAGED_SMOKE_UI_RECEIPT_PASS', {
    repository_id: connected.json.repo_id,
    snapshot_id: scan.snapshot_id,
    report_digest: finalReportEnvelope.digest,
  });
  console.log('PACKAGED_SMOKE_MCP_PASS', {
    live_event_seq: mcpImpact.trajectory.live_seq,
    trajectory_event_seq: mcpImpact.trajectory.event_seq,
    report_digest: finalReportEnvelope.digest,
    bearer_secret_embedded: false,
  });
  console.log('PACKAGED_SMOKE_ANALYSIS_PASS', {
    repository_id: connected.json.repo_id,
    snapshot_id: scan.snapshot_id,
    files_scanned: scan.files_scanned,
    report_digest: finalReportEnvelope.digest,
    runtime_files_verified: runtimeIntegrity?.files_verified ?? null,
    missing_token_status: unauthenticated.statusCode,
    invalid_token_status: invalidToken.statusCode,
  });
  return {
    schema_version: 1,
    status: 'PASS',
    repository_id: connected.json.repo_id,
    snapshot_id: scan.snapshot_id,
    files_scanned: scan.files_scanned,
    report_digest: finalReportEnvelope.digest,
    mcp_live_event_seq: mcpImpact.trajectory.live_seq,
    mcp_trajectory_event_seq: mcpImpact.trajectory.event_seq,
    mcp_trajectory_persisted: mcpImpact.trajectory.trajectory_persisted,
    ui_receipt_verified: true,
    runtime_files_verified: runtimeIntegrity?.files_verified ?? null,
    missing_token_status: unauthenticated.statusCode,
    invalid_token_status: invalidToken.statusCode,
  };
}

async function checkApplicationHealth() {
  const [daemonHealthy, cockpitHealthy] = await Promise.all([
    checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`, 1500, {
      Authorization: `Bearer ${LOOPBACK_TOKEN}`,
    }),
    checkServiceHealth(COCKPIT_URL),
  ]);
  return daemonHealthy && cockpitHealthy;
}

async function waitForApplicationHealth(maxAttempts = 60) {
  for (let attempt = 0; attempt < maxAttempts; attempt += 1) {
    if (await checkApplicationHealth()) return true;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return false;
}

const SPLASH_DATA_URL = 'data:text/html;charset=utf-8,' + encodeURIComponent(SPLASH_HTML);

// Ensure sidecar daemon is running natively on Windows
async function ensureSidecarDaemon() {
  const isHealthy = await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`, 1500, {
    Authorization: `Bearer ${LOOPBACK_TOKEN}`,
  });
  if (isHealthy) {
    console.log(`[SynEvid Desktop] Reusing active synevid-ui daemon on port ${DAEMON_PORT}`);
    return;
  }

  const repoRoot = sourceRepositoryRoot();
  const existingBinary = daemonExecutableCandidates().find((candidate) => fs.existsSync(candidate));
  console.log(`[SynEvid Desktop] Spawning synevid-ui sidecar for ${daemonRepositoryRoot()}`);

  try {
    // The live Electron shell must exercise the current Rust source. A
    // packaged/release binary can remain available for production startup, but
    // development must use Cargo's incremental source runtime so daemon edits
    // are not hidden by a stale release executable.
    if (LIVE_DEVELOPMENT) {
      const cargoCmd = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
      console.log('[SynEvid Desktop] Live development: starting synevid-ui from source via Cargo.');
      daemonProcess = spawn(cargoCmd, ['run', '-p', 'synevid-ui', '--', '--host', '127.0.0.1', '--port', String(DAEMON_PORT), '--repo-root', daemonRepositoryRoot()], {
        cwd: repoRoot,
        stdio: 'ignore',
        detached: false,
        windowsHide: true,
        env: {
          ...process.env,
          SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN,
          SYNEVID_MCP_TOKEN_FILE: mcpTokenFilePath || writeMcpTokenFile(),
          SYNEVID_DAEMON_URL: `http://127.0.0.1:${DAEMON_PORT}`,
          SYNEVID_MCP_BUNDLE_PATH: mcpBundlePath(),
          SYNEVID_MCP_COMMAND: process.execPath,
          SYNEVID_MCP_ELECTRON_NODE: '1',
          AUTOPSY_BIN: app.isPackaged
            ? packagedRuntimePath('sidecars', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy')
            : path.join(sourceRepositoryRoot(), 'target', 'debug', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy'),
        },
      });
    } else if (existingBinary) {
      daemonProcess = spawn(existingBinary, ['--host', '127.0.0.1', '--port', String(DAEMON_PORT), '--repo-root', daemonRepositoryRoot()], {
        cwd: daemonRepositoryRoot(),
        stdio: 'ignore',
        detached: false,
        windowsHide: true,
        env: {
          ...process.env,
          SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN,
          SYNEVID_MCP_TOKEN_FILE: mcpTokenFilePath || writeMcpTokenFile(),
          SYNEVID_DAEMON_URL: `http://127.0.0.1:${DAEMON_PORT}`,
          SYNEVID_MCP_BUNDLE_PATH: mcpBundlePath(),
          SYNEVID_MCP_COMMAND: process.execPath,
          SYNEVID_MCP_ELECTRON_NODE: '1',
          AUTOPSY_BIN: app.isPackaged
            ? packagedRuntimePath('sidecars', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy')
            : path.join(sourceRepositoryRoot(), 'target', 'debug', process.platform === 'win32' ? 'autopsy.exe' : 'autopsy'),
        },
      });
    } else {
      const cargoCmd = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
      daemonProcess = spawn(cargoCmd, ['run', '-p', 'synevid-ui', '--', '--host', '127.0.0.1', '--port', String(DAEMON_PORT), '--repo-root', daemonRepositoryRoot()], {
        cwd: repoRoot,
        stdio: 'ignore',
        detached: false,
        windowsHide: true,
        env: { ...process.env, SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN },
      });
    }

    daemonProcess.on('error', (err) => {
      console.warn('[SynEvid Desktop] Sidecar daemon spawn warning:', err.message);
      showDaemonErrorBanner(err.message);
    });
    daemonProcess.on('exit', (code, signal) => {
      if (shuttingDown) return;
      daemonProcess = null;
      daemonRestartAttempts += 1;
      const detail = `sidecar exited (code=${code ?? 'null'}, signal=${signal ?? 'none'})`;
      showDaemonErrorBanner(detail);
      const delay = Math.min(2000, 100 * daemonRestartAttempts);
      daemonRestartTimer = setTimeout(async () => {
        daemonRestartTimer = null;
        await ensureSidecarDaemon();
      }, delay);
    });
    daemonRestartAttempts = 0;
    clearDaemonErrorBanner();
  } catch (err) {
    console.warn('[SynEvid Desktop] Sidecar spawn exception:', err.message);
    showDaemonErrorBanner(err.message);
  }
}

// Ensure Next.js Cockpit is running natively on Windows
async function ensureWebCockpit() {
  const isHealthy = await checkServiceHealth(COCKPIT_URL);
  if (isHealthy) {
    console.log(`[SynEvid Desktop] Reusing active Cockpit on ${COCKPIT_URL}`);
    return;
  }

  const repoRoot = sourceRepositoryRoot();
  const webDir = path.join(repoRoot, 'apps/web');
  const packagedCockpit = app.isPackaged ? packagedRuntimePath('cockpit') : null;
  const packagedServer = packagedCockpit ? findPackagedCockpitServer(packagedCockpit) : null;
  const productionBuildExists = app.isPackaged
    ? Boolean(packagedServer)
    : fs.existsSync(path.join(webDir, '.next', 'BUILD_ID'));
  const nextMode = LIVE_DEVELOPMENT || !productionBuildExists ? 'dev' : 'start';
  const cockpitPort = new URL(COCKPIT_URL).port || '3000';
  console.log('[SynEvid Desktop] Spawning Next.js ' + nextMode + ' server on port ' + cockpitPort + '...');

  try {
    if (app.isPackaged && packagedServer) {
      // Electron's Windows Node-mode argument parser can split a script path
      // at spaces in an installed workspace path. Pass the path through the
      // environment and require it from a constant expression instead.
      webProcess = spawn(process.execPath, ['-e', "require(process.env.SYNEVID_COCKPIT_SERVER)"], {
        cwd: path.dirname(packagedServer),
        stdio: process.env.SYNEVID_SMOKE === '1' ? ['ignore', 'pipe', 'pipe'] : 'ignore',
        detached: false,
        windowsHide: true,
        env: {
          ...process.env,
          ELECTRON_RUN_AS_NODE: '1',
          SYNEVID_COCKPIT_SERVER: packagedServer,
          NODE_ENV: 'production',
          PORT: String(new URL(COCKPIT_URL).port || 3000),
          HOSTNAME: '127.0.0.1',
          NODE_PATH: [packagedRuntimePath('deps'), process.env.NODE_PATH].filter(Boolean).join(path.delimiter),
          SYNEVID_DAEMON_URL: `http://127.0.0.1:${DAEMON_PORT}`,
        },
      });
      if (webProcess.stdout) {
        webProcess.stdout.on('data', (chunk) => {
          if (process.env.SYNEVID_SMOKE === '1') {
            console.log(`[SynEvid Desktop] Cockpit stdout: ${chunk.toString().trim()}`);
          }
        });
      }
      if (webProcess.stderr) {
        webProcess.stderr.on('data', (chunk) => {
          if (process.env.SYNEVID_SMOKE === '1') {
            console.error(`[SynEvid Desktop] Cockpit stderr: ${chunk.toString().trim()}`);
          }
        });
      }
    } else {
      const nextBin = path.join(webDir, 'node_modules', 'next', 'dist', 'bin', 'next');
      if (process.platform === 'win32' && fs.existsSync(nextBin)) {
      webProcess = spawn(
        'node.exe',
        [nextBin, nextMode, '-p', cockpitPort, '-H', '127.0.0.1'],
        {
          cwd: webDir,
          stdio: 'ignore',
          detached: false,
          windowsHide: true,
        }
      );
      } else if (process.platform === 'win32') {
      webProcess = spawn(
        'cmd.exe',
        ['/d', '/s', '/c', `npx next ${nextMode} -p ${cockpitPort} -H 127.0.0.1`],
        {
          cwd: webDir,
          stdio: 'ignore',
          detached: false,
          windowsHide: true,
        }
      );
      } else {
      webProcess = spawn('npx', ['next', nextMode, '-p', cockpitPort, '-H', '0.0.0.0'], {
        cwd: webDir,
        stdio: 'ignore',
        detached: false,
      });
      }
    }

    webProcess.on('error', (err) => {
      console.warn('[SynEvid Desktop] Web Cockpit spawn warning:', err.message);
    });
  } catch (err) {
    console.warn('[SynEvid Desktop] Web Cockpit spawn exception:', err.message);
  }
}

function createWindow() {
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    const hasServerCsp = Object.keys(details.responseHeaders || {})
      .some((name) => name.toLowerCase() === 'content-security-policy');
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        ...(hasServerCsp ? {} : {
          'Content-Security-Policy': [buildContentSecurityPolicy(LIVE_DEVELOPMENT)],
        }),
      },
    });
  });

  mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1024,
    minHeight: 700,
    backgroundColor: '#090A0F',
    title: 'SynEvid — Code Autopsy Visual Cockpit',
    autoHideMenuBar: true,
    // Keep the canvas renderer untouched while using native overlay controls
    // for a compact frameless shell on desktop platforms.
    titleBarStyle: process.platform === 'darwin' ? 'hiddenInset' : 'hidden',
    titleBarOverlay: process.platform === 'darwin'
      ? undefined
      : { color: '#090A0F', symbolColor: '#94A3B8', height: 34 },
    show: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      devTools: true,
    },
  });

  mainWindow.once('ready-to-show', () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.show();
      mainWindow.focus();
    }
  });

  mainWindow.webContents.on('did-finish-load', () => {
    console.log(`[SynEvid Desktop] Loaded URL: ${mainWindow.webContents.getURL()}`);
  });

  mainWindow.webContents.on('console-message', (event, level, message) => {
    if (level >= 2) {
      console.log(`[SynEvid Renderer] ${message}`);
    }
  });

  const shellWindow = mainWindow;
  const shellWebContents = shellWindow.webContents;
  shellWebContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  shellWebContents.on('will-navigate', (event, url) => {
    if (!isTrustedRendererUrl(url, COCKPIT_URL)) event.preventDefault();
  });
  shellWebContents.on('will-attach-webview', (event) => event.preventDefault());
  shellWebContents.on('before-input-event', (event, input) => {
    const isToggleShortcut =
      input.type === 'keyDown' &&
      (input.key === 'F12' ||
        ((input.control || input.meta) && input.shift && String(input.key).toLowerCase() === 'i'));
    if (!isToggleShortcut) return;

    event.preventDefault();
    if (shellWebContents.isDevToolsOpened()) {
      shellWebContents.closeDevTools();
    } else {
      shellWebContents.openDevTools({ mode: 'right' });
    }
  });

  const startCockpit = async () => {
    let attempts = 0;
    let ready = await checkApplicationHealth();
    if (!shellWindow || shellWindow.isDestroyed()) return;
    if (!ready) {
      await shellWindow.loadURL(SPLASH_DATA_URL).catch(() => {});
    }

    while (mainWindow === shellWindow && !shellWindow.isDestroyed()) {
      if (ready) {
        try {
          await shellWindow.loadURL(cockpitRendererUrl());
          return;
        } catch (err) {
          console.warn('Cockpit load failed; returning to splash:', err.message);
          ready = false;
          await shellWindow.loadURL(SPLASH_DATA_URL).catch(() => {});
        }
      }

      attempts += 1;
      if (attempts === 60) {
        shellWindow.webContents.executeJavaScript(
          "document.querySelector('.highlight').textContent = 'Cockpit is still starting; retrying automatically'"
        ).catch(() => {});
      }
      await new Promise((resolve) => setTimeout(resolve, attempts < 60 ? 500 : 1500));
      ready = await checkApplicationHealth();
    }
  };

  // The packaged smoke contract checks the standalone cockpit over HTTP below.
  // Do not race that health check with a BrowserWindow navigation that is
  // deliberately torn down immediately after the check passes. Normal source
  // and packaged runtime launches retain the regular splash-to-cockpit flow.
  if (process.env.SYNEVID_SMOKE !== '1') {
    startCockpit().catch((err) => {
      console.warn('Cockpit startup failed:', err.message);
    });
  }

  mainWindow.on('closed', () => {
    mainWindow = null;
  });
}

// IPC Handlers
ipcMain.handle('dialog:openDirectory', async (event) => {
  if (!isTrustedIpcSender(event, COCKPIT_URL)) return null;
  if (!mainWindow) return null;
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openDirectory'],
    title: 'Select Repository Root for SynEvid Autopsy',
  });
  if (result.canceled || result.filePaths.length === 0) {
    return null;
  }
  return result.filePaths[0];
});

ipcMain.handle('daemon:status', async (event) => {
  if (!isTrustedIpcSender(event, COCKPIT_URL)) return { error_code: 'ERR-IPC-403' };
  const daemonHealthy = await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`, 1500, {
    Authorization: `Bearer ${LOOPBACK_TOKEN}`,
  });
  const webHealthy = await checkServiceHealth(COCKPIT_URL);
  return {
    port: DAEMON_PORT,
    daemonOnline: daemonHealthy,
    cockpitOnline: webHealthy,
    cockpitUrl: COCKPIT_URL,
  };
});

ipcMain.handle('daemon:token', (event) => (
  isTrustedIpcSender(event, COCKPIT_URL) ? LOOPBACK_TOKEN : null
));
ipcMain.on('daemon:token-sync', (event) => {
  event.returnValue = isTrustedIpcSender(event, COCKPIT_URL) ? LOOPBACK_TOKEN : null;
});

ipcMain.handle('frame-meter:start', (event) => (isTrustedIpcSender(event, COCKPIT_URL) ? startFrameMeter() : { error_code: 'ERR-IPC-403' }));
ipcMain.handle('frame-meter:stats', (event) => (isTrustedIpcSender(event, COCKPIT_URL) ? readFrameMeterStats() : { error_code: 'ERR-IPC-403' }));
ipcMain.handle('frame-meter:stop', (event) => (isTrustedIpcSender(event, COCKPIT_URL) ? stopFrameMeter() : { error_code: 'ERR-IPC-403' }));
ipcMain.handle('frame-meter:mark', (event) => (isTrustedIpcSender(event, COCKPIT_URL) ? markNextPresentedFrame() : { error_code: 'ERR-IPC-403' }));
ipcMain.on('frame-meter:mark-sync', (event) => {
  event.returnValue = isTrustedIpcSender(event, COCKPIT_URL) ? markNextPresentedFrame() : { error_code: 'ERR-IPC-403' };
});

ipcMain.on('window:minimize', (event) => {
  if (!isTrustedIpcSender(event, COCKPIT_URL)) return;
  if (mainWindow) mainWindow.minimize();
});

ipcMain.on('window:maximize', (event) => {
  if (!isTrustedIpcSender(event, COCKPIT_URL)) return;
  if (mainWindow) {
    if (mainWindow.isMaximized()) {
      mainWindow.unmaximize();
    } else {
      mainWindow.maximize();
    }
  }
});

ipcMain.on('window:close', (event) => {
  if (!isTrustedIpcSender(event, COCKPIT_URL)) return;
  if (mainWindow) mainWindow.close();
});

// Single instance: concurrent instances share userData and fight over the disk
// cache ("Unable to move the cache: Access is denied").
const gotSingleInstanceLock = app.requestSingleInstanceLock();
if (!gotSingleInstanceLock) {
  app.quit();
} else {
  app.on('second-instance', () => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
  });
}

app.whenReady().then(async () => {
  if (!gotSingleInstanceLock) return;
  writeMcpTokenFile();
  createWindow();
  Menu.setApplicationMenu(Menu.buildFromTemplate([
    { label: 'SynEvid', submenu: [{ role: 'about' }, { type: 'separator' }, { role: 'quit' }] },
    { label: 'View', submenu: [{ role: 'reload' }, { role: 'toggleDevTools' }, { role: 'resetZoom' }] },
  ]));
  try {
    tray = new Tray(nativeImage.createEmpty());
    tray.setToolTip('SynEvid');
    tray.setContextMenu(Menu.buildFromTemplate([
      { label: 'Show SynEvid', click: () => mainWindow?.show() },
      { label: 'Quit', click: () => app.quit() },
    ]));
  } catch (error) {
    console.warn('[SynEvid Desktop] Tray unavailable:', error.message);
  }
  await ensureSidecarDaemon();
  await ensureWebCockpit();

  if (process.env.SYNEVID_SMOKE === '1') {
    try {
      const smokeReady = await waitForApplicationHealth();
      if (!smokeReady) throw new Error('Packaged daemon or cockpit did not become healthy');
      const smokeReceipt = await runPackagedSmokeWorkflow();
      const smokeReceiptPath = path.join(process.env.SYNEVID_SMOKE_REPO, '.synevid-smoke-receipt.json');
      fs.writeFileSync(smokeReceiptPath, JSON.stringify(smokeReceipt), { encoding: 'utf8', flag: 'wx', mode: 0o600 });
      console.log('PACKAGED_SMOKE_COMPONENTS', { daemon: true, cockpit: true });
      console.log('PACKAGED_SMOKE_PASS');
      setTimeout(() => {
        shutdownChildren();
        app.exit(0);
      }, 50);
    } catch (error) {
      console.error('PACKAGED_SMOKE_FAIL', error.message);
      showDaemonErrorBanner(error.message);
      shutdownChildren();
      setTimeout(() => app.exit(1), 250);
    }
  }

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    shutdownChildren();
    app.quit();
  }
});

app.on('before-quit', shutdownChildren);
