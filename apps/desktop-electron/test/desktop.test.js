const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const PKG_ROOT = path.resolve(__dirname, '..');

test('FR-P5-001: Electron Desktop App Package Configuration', () => {
  const pkgPath = path.join(PKG_ROOT, 'package.json');
  assert.ok(fs.existsSync(pkgPath), 'package.json must exist');

  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  assert.equal(pkg.main, 'src/main.js');
  assert.ok(pkg.devDependencies.electron, 'Must specify electron dependency');
  assert.equal(pkg.scripts.dev, 'node scripts/dev.js');
  assert.equal(pkg.scripts.start, 'node scripts/dev.js');
  assert.ok(pkg.engines.node.startsWith('>=20'), 'Must declare the supported Node.js floor');
  assert.ok(pkg.engines.npm.startsWith('>=10'), 'Must declare the supported npm floor');
});

test('FR-P5-001: Main Process & Hardened Security Architecture (INV-SANDBOX-004)', () => {
  const mainPath = path.join(PKG_ROOT, 'src/main.js');
  assert.ok(fs.existsSync(mainPath), 'main.js must exist');

  const content = fs.readFileSync(mainPath, 'utf8');
  assert.ok(content.includes('contextIsolation: true'), 'Must enable contextIsolation');
  assert.ok(content.includes('nodeIntegration: false'), 'Must disable nodeIntegration');
  assert.ok(content.includes('sandbox: true'), 'Must enable sandbox');
  assert.ok(content.includes('Content-Security-Policy'), 'Must install strict local loopback CSP');
  assert.ok(content.includes('dialog:openDirectory'), 'Must register native folder picker IPC');
  assert.ok(content.includes('ensureSidecarDaemon'), 'Must implement daemon watchdog');
  assert.ok(content.includes("path.join(repoRoot, 'target', 'release', executableName)"), 'Must prefer the release daemon when available');
  assert.ok(content.includes("path.join(repoRoot, 'target', 'debug', executableName)"), 'Must retain a debug daemon fallback');
  assert.ok(content.includes('checkApplicationHealth'), 'Must wait for both local services before loading the cockpit');
  assert.ok(content.includes('beginFrameSubscription'), 'Must expose native presentation-event measurement');
  assert.ok(content.includes('presentationLatencyMs'), 'Must report compositor-boundary timing');
  assert.ok(content.includes("windowsHide: true"), 'WSL child processes must not open a terminal over the cockpit');
});

test('FR-P5-001: Hardened Preload Bridge Integrity', () => {
  const preloadPath = path.join(PKG_ROOT, 'src/preload.js');
  assert.ok(fs.existsSync(preloadPath), 'preload.js must exist');

  const content = fs.readFileSync(preloadPath, 'utf8');
  assert.ok(content.includes('contextBridge.exposeInMainWorld'), 'Must use contextBridge');
  assert.ok(content.includes('openDirectory'), 'Must expose openDirectory');
  assert.ok(content.includes('minimizeWindow'), 'Must expose minimizeWindow');
  assert.ok(content.includes('maximizeWindow'), 'Must expose maximizeWindow');
  assert.ok(content.includes('closeWindow'), 'Must expose closeWindow');
  assert.ok(content.includes('startFrameMeter'), 'Must expose the native frame meter');
  assert.ok(content.includes('markNextPresentedFrame'), 'Must expose presentation timing marks');
  assert.ok(content.includes("sendSync('frame-meter:mark-sync')"), 'Theme marks must be registered before the DOM mutation');
});

test('FR-P5-001: Cockpit startup stays recoverable until the server is ready', () => {
  const mainPath = path.join(PKG_ROOT, 'src/main.js');
  const content = fs.readFileSync(mainPath, 'utf8');

  assert.ok(content.includes('while (mainWindow === shellWindow && !shellWindow.isDestroyed())'));
  assert.ok(content.includes('Cockpit is still starting; retrying automatically'));
  assert.ok(content.includes('LIVE_DEVELOPMENT'));
  assert.ok(content.includes("LIVE_DEVELOPMENT || !productionBuildExists ? 'dev' : 'start'"));
  assert.ok(content.includes('buildContentSecurityPolicy'));
  assert.ok(content.includes("liveDevelopment ? \" 'unsafe-eval'\" : ''"));
  assert.ok(!content.includes('mainWindow.loadURL(COCKPIT_URL).catch(() => {});'));
});

test('FR-P5-001: Docked DevTools toggle is available from the shell', () => {
  const mainPath = path.join(PKG_ROOT, 'src/main.js');
  const content = fs.readFileSync(mainPath, 'utf8');

  assert.ok(content.includes('devTools: true'));
  assert.ok(content.includes("shellWebContents.on('before-input-event'"));
  assert.ok(content.includes("input.key === 'F12'"));
  assert.ok(content.includes("String(input.key).toLowerCase() === 'i'"));
  assert.ok(content.includes("shellWebContents.openDevTools({ mode: 'right' })"));
  assert.ok(content.includes('shellWebContents.closeDevTools()'));
});

test('FR-P2-003: Windows Electron prefers the discrete GPU for the large graph canvas', () => {
  const mainPath = path.join(PKG_ROOT, 'src/main.js');
  const content = fs.readFileSync(mainPath, 'utf8');
  const gpuSwitch = content.indexOf("app.commandLine.appendSwitch('force_high_performance_gpu')");
  const appReady = content.indexOf('app.whenReady()');

  assert.ok(gpuSwitch >= 0, 'Hybrid-GPU Windows systems should select the discrete adapter');
  assert.ok(gpuSwitch < appReady, 'Chromium GPU selection must be set before Electron becomes ready');
  assert.ok(content.includes("if (process.platform === 'win32')"));
});

test('FR-P5-001: Desktop launchers clear Electron Node-only mode', () => {
  const repoRoot = path.resolve(PKG_ROOT, '../..');
  const powershellLauncher = fs.readFileSync(path.join(repoRoot, 'scripts/run-desktop.ps1'), 'utf8');
  const batchLauncher = fs.readFileSync(path.join(repoRoot, 'scripts/run-desktop.bat'), 'utf8');

  assert.ok(powershellLauncher.includes("$env:ELECTRON_RUN_AS_NODE = ''"));
  assert.ok(batchLauncher.includes('set "ELECTRON_RUN_AS_NODE="'));
  assert.ok(!powershellLauncher.includes('Start-Process -FilePath "wsl.exe"'));
  assert.ok(!batchLauncher.includes('start /b "" wsl.exe'));

  const devLauncher = fs.readFileSync(path.join(PKG_ROOT, 'scripts/dev.js'), 'utf8');
  assert.ok(devLauncher.includes("env.COCKPIT_URL = 'http://127.0.0.1:3001'"));
  assert.ok(devLauncher.includes('source development independent from a stale production cockpit'));
});

test('Cockpit quick actions have working export and verification handlers', () => {
  const repoRoot = path.resolve(PKG_ROOT, '../..');
  const pagePath = path.join(repoRoot, 'apps/web/src/app/page.tsx');
  const content = fs.readFileSync(pagePath, 'utf8');

  assert.ok(content.includes('onClick={handleOpenVerificationGates}'));
  assert.ok(content.includes('await api.getInvariants()'));
  assert.ok(content.includes('onClick={handleExportReport}'));
  assert.ok(content.includes('URL.createObjectURL'));
  assert.ok(content.includes("link.download = 'synevid-report-'"));
});
