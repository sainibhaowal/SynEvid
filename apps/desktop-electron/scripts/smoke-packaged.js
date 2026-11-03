// Launches a built Electron artifact and requires both bundled services to
// become healthy. This is intentionally separate from source-mode tests.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');

const desktopDir = path.resolve(__dirname, '..');
const requested = process.argv[2] ? path.resolve(process.argv[2]) : null;
const candidates = [
  requested,
  path.join(desktopDir, 'dist', 'win-unpacked', 'SynEvid.exe'),
  path.join(desktopDir, 'dist', 'mac', 'SynEvid.app', 'Contents', 'MacOS', 'SynEvid'),
  path.join(desktopDir, 'dist', 'mac-arm64', 'SynEvid.app', 'Contents', 'MacOS', 'SynEvid'),
  path.join(desktopDir, 'dist', 'mac-x64', 'SynEvid.app', 'Contents', 'MacOS', 'SynEvid'),
  path.join(desktopDir, 'dist', 'linux-unpacked', 'synevid'),
].filter(Boolean);
const executable = candidates.find((candidate) => fs.existsSync(candidate));
if (!executable) {
  console.error('[SynEvid Smoke] No packaged executable found. Pass its path or run a dist target first.');
  process.exit(2);
}

const smokeUserData = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-packaged-smoke-'));

// The host may itself be running under Electron's Node compatibility mode
// (for example when launched from an Electron-based IDE). Never leak that
// flag into the packaged app: it would prevent the Electron main process from
// creating its BrowserWindow and can make the smoke result misleading.
const cleanEnvironment = { ...process.env };
delete cleanEnvironment.ELECTRON_RUN_AS_NODE;
const child = spawn(executable, [
  `--user-data-dir=${smokeUserData}`,
], {
  cwd: path.dirname(executable),
  env: {
    ...cleanEnvironment,
    SYNEVID_SMOKE: '1',
    SYNEVID_PORT: '3343',
    COCKPIT_URL: 'http://127.0.0.1:3344',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
  windowsHide: true,
});
let output = '';
child.stdout.on('data', (chunk) => { output += chunk.toString(); process.stdout.write(chunk); });
child.stderr.on('data', (chunk) => { output += chunk.toString(); process.stderr.write(chunk); });
const timeout = setTimeout(() => {
  child.kill();
  console.error('[SynEvid Smoke] Timed out waiting for packaged health.');
  process.exit(1);
}, 45_000);
child.on('error', (error) => {
  clearTimeout(timeout);
  console.error(`[SynEvid Smoke] ${error.message}`);
  process.exit(1);
});
child.on('exit', (code) => {
  clearTimeout(timeout);
  if (code === 0 && output.includes('PACKAGED_SMOKE_PASS')) process.exit(0);
  console.error(`[SynEvid Smoke] Failed with exit code ${code}.`);
  process.exit(1);
});
