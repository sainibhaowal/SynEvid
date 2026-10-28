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
});
