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
const smokeRepository = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-smoke-repository-'));
fs.mkdirSync(path.join(smokeRepository, 'src'));
fs.writeFileSync(
  path.join(smokeRepository, 'src', 'index.ts'),
  'export function smokeEntry(value: string): string { return value.trim(); }\n',
  'utf8',
);

// The host may itself be running under Electron's Node compatibility mode
// (for example when launched from an Electron-based IDE). Never leak that
// flag into the packaged app: it would prevent the Electron main process from
// creating its BrowserWindow and can make the smoke result misleading.
const cleanEnvironment = { ...process.env };
delete cleanEnvironment.ELECTRON_RUN_AS_NODE;
const launchArgs = [`--user-data-dir=${smokeUserData}`];
if (executable.toLowerCase().endsWith('.appimage')) launchArgs.unshift('--appimage-extract-and-run');
// Linux CI smoke runs inside a virtual display and can be sandbox-restricted
// by the hosted runner. These flags apply only to the disposable smoke child;
// the packaged production launcher and its GPU/canvas defaults are unchanged.
if (process.platform === 'linux') launchArgs.push('--no-sandbox', '--disable-gpu');
const child = spawn(executable, launchArgs, {
  cwd: path.dirname(executable),
  env: {
    ...cleanEnvironment,
    SYNEVID_SMOKE: '1',
    SYNEVID_SMOKE_REPO: smokeRepository,
    SYNEVID_PORT: '3343',
    COCKPIT_URL: 'http://127.0.0.1:3344',
  },
  stdio: ['ignore', 'pipe', 'pipe'],
  windowsHide: true,
});
let output = '';
let timedOut = false;
child.stdout.on('data', (chunk) => { output += chunk.toString(); process.stdout.write(chunk); });
child.stderr.on('data', (chunk) => { output += chunk.toString(); process.stderr.write(chunk); });
const timeout = setTimeout(() => {
  timedOut = true;
  if (process.platform === 'win32' && child.pid) {
    spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
  } else {
    child.kill();
  }
  console.error('[SynEvid Smoke] Timed out waiting for packaged health.');
  setTimeout(() => process.exit(1), 2_000).unref();
}, 180_000);
child.on('error', (error) => {
  clearTimeout(timeout);
  console.error(`[SynEvid Smoke] ${error.message}`);
  fs.rmSync(smokeUserData, { recursive: true, force: true });
  fs.rmSync(smokeRepository, { recursive: true, force: true });
  process.exit(1);
});
child.on('close', (code) => {
  clearTimeout(timeout);
  let receipt = null;
  try {
    receipt = JSON.parse(fs.readFileSync(path.join(smokeRepository, '.synevid-smoke-receipt.json'), 'utf8'));
  } catch {}
  const digestPattern = /^blake3:[0-9a-f]{64}$/;
  const repositoryPattern = /^[0-9a-f]{64}$/;
  const validReceipt = receipt?.schema_version === 1 &&
    receipt.status === 'PASS' &&
    repositoryPattern.test(receipt.repository_id || '') &&
    repositoryPattern.test(receipt.snapshot_id || '') &&
    digestPattern.test(receipt.report_digest || '') &&
    Number.isInteger(receipt.mcp_live_event_seq) && receipt.mcp_live_event_seq > 0 &&
    Number.isInteger(receipt.mcp_trajectory_event_seq) && receipt.mcp_trajectory_event_seq > 0 &&
    receipt.mcp_trajectory_persisted === true &&
    receipt.ui_receipt_verified === true &&
    Number.isInteger(receipt.runtime_files_verified) && receipt.runtime_files_verified > 0 &&
    receipt.missing_token_status === 401 && receipt.invalid_token_status === 401 &&
    Number.isInteger(receipt.files_scanned) && receipt.files_scanned > 0;
  fs.rmSync(smokeUserData, { recursive: true, force: true });
  fs.rmSync(smokeRepository, { recursive: true, force: true });
  if (!timedOut && code === 0 && validReceipt) {
    console.log('PACKAGED_SMOKE_RECEIPT_PASS', receipt);
    process.exit(0);
  }
  console.error(`[SynEvid Smoke] Failed with exit code ${code}; receipt valid=${validReceipt}.`);
  process.exit(1);
});
