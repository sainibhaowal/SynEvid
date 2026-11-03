// Live development launcher for the native Electron application.
// It runs Electron from source; rebuilding is reserved for the separate dist script.
const { spawn, spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const desktopDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopDir, '../..');
const webDir = path.join(repoRoot, 'apps', 'web');
const webPackage = path.join(webDir, 'package.json');
const nextBin = path.join(webDir, 'node_modules', 'next', 'dist', 'bin', 'next');

function fail(message) {
  console.error(`[SynEvid Dev] ${message}`);
  process.exit(1);
}

function readVersion(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8', windowsHide: true });
  if (result.error || result.status !== 0) return null;
  return (result.stdout || '').trim();
}

const nodeMajor = Number(process.versions.node.split('.')[0]);
if (!Number.isFinite(nodeMajor) || nodeMajor < 20) {
  fail(`Node.js 20 or newer is required; found ${process.versions.node}.`);
}

const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const npmExecutable =
  process.platform === 'win32'
    ? process.env.npm_execpath || path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js')
    : npmCommand;
const npmVersion =
  process.platform === 'win32'
    ? readVersion(process.execPath, [npmExecutable, '--version'])
    : readVersion(npmExecutable, ['--version']);
if (!npmVersion) {
  fail('npm was not found. Install Node.js 20+ with npm, then run this command again.');
}

if (!fs.existsSync(webPackage)) {
  fail(`Web package metadata is missing: ${webPackage}`);
}

if (!fs.existsSync(nextBin)) {
  fail(
    `Web dependencies are incomplete: ${nextBin} is missing. ` +
      'Run "npm ci --ignore-scripts" from apps/web once, then rerun npm run dev.',
  );
}

const env = { ...process.env };
// This variable makes Electron run as Node and leaves ipcMain undefined.
delete env.ELECTRON_RUN_AS_NODE;
env.SYNEVID_DEV = '1';
env.NODE_ENV = 'development';
// Keep source development independent from a stale production cockpit that
// may already occupy port 3000. The Electron shell still owns the same live
// source runtime; this only gives its Next process a dedicated loopback port.
if (!env.COCKPIT_URL) {
  env.COCKPIT_URL = 'http://127.0.0.1:3001';
}

const electronBin = require('electron');
const main = path.join(desktopDir, 'src', 'main.js');

console.log(`[SynEvid Dev] Node ${process.versions.node}; npm ${npmVersion}`);
console.log('[SynEvid Dev] Launching source Electron + Next.js dev runtime.');
console.log('[SynEvid Dev] Renderer changes hot-reload; no electron-builder build is used.');

const child = spawn(electronBin, [main, ...process.argv.slice(2)], {
  cwd: repoRoot,
  stdio: 'inherit',
  env,
  windowsHide: false,
});

child.on('error', (error) => {
  console.error(`[SynEvid Dev] Electron failed to start: ${error.message}`);
  process.exitCode = 1;
});

const forwardSignal = (signal) => {
  if (!child.killed) child.kill(signal);
};
process.on('SIGINT', () => forwardSignal('SIGINT'));
process.on('SIGTERM', () => forwardSignal('SIGTERM'));

child.on('exit', (code, signal) => {
  process.exitCode = signal ? 1 : code ?? 0;
});
