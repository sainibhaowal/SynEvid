// Launches a built Electron artifact and requires both bundled services to
// become healthy. This is intentionally separate from source-mode tests.
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const net = require('node:net');

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

function checkPortClosed(port) {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: '127.0.0.1', port });
    let settled = false;
    const finish = (closed) => {
      if (settled) return;
      settled = true;
      socket.destroy();
      resolve(closed);
    };
    socket.setTimeout(750, () => finish(false));
    socket.once('connect', () => finish(false));
    socket.once('error', (error) => finish(error.code === 'ECONNREFUSED'));
  });
}

async function waitForSmokeServicesToStop() {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    const [daemonClosed, cockpitClosed] = await Promise.all([
      checkPortClosed(3343),
      checkPortClosed(3344),
    ]);
    if (daemonClosed && cockpitClosed) return true;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  return false;
}

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
}, 300_000);
child.on('error', (error) => {
  clearTimeout(timeout);
  console.error(`[SynEvid Smoke] ${error.message}`);
  fs.rmSync(smokeUserData, { recursive: true, force: true });
  fs.rmSync(smokeRepository, { recursive: true, force: true });
  process.exit(1);
});
child.on('close', async (code) => {
  clearTimeout(timeout);
  let receipt = null;
  try {
    receipt = JSON.parse(fs.readFileSync(path.join(smokeRepository, '.synevid-smoke-receipt.json'), 'utf8'));
  } catch {}
  const digestPattern = /^blake3:[0-9a-f]{64}$/;
  const repositoryPattern = /^[0-9a-f]{64}$/;
  const trayQuitInvoked = fs.existsSync(path.join(smokeRepository, '.synevid-tray-quit-invoked'));
  const validReceipt = receipt?.schema_version === 1 &&
    receipt.status === 'PASS' &&
    repositoryPattern.test(receipt.repository_id || '') &&
    repositoryPattern.test(receipt.snapshot_id || '') &&
    digestPattern.test(receipt.report_digest || '') &&
    Number.isInteger(receipt.mcp_live_event_seq) && receipt.mcp_live_event_seq > 0 &&
    Number.isInteger(receipt.mcp_trajectory_event_seq) && receipt.mcp_trajectory_event_seq > 0 &&
    receipt.mcp_trajectory_persisted === true &&
    receipt.ui_receipt_verified === true &&
    receipt.security_verification?.context_isolation === true &&
    receipt.security_verification?.node_integration_disabled === true &&
    receipt.security_verification?.sandbox === true &&
    receipt.security_verification?.nonce_csp_verified === true &&
    receipt.security_verification?.untrusted_navigation_blocked === true &&
    receipt.security_verification?.popup_blocked === true &&
    receipt.security_verification?.invalid_ipc_sender_refused === true &&
    receipt.desktop_controls?.application_menu_verified === true &&
    receipt.desktop_controls?.tray_available === true &&
    receipt.desktop_controls?.tray_actions_verified === true &&
    receipt.desktop_controls?.tray_quit_action_verified === true &&
    trayQuitInvoked &&
    receipt.watchdog_recovery?.code === 'ERR-DAEMON-301' &&
    receipt.watchdog_recovery?.state === 'RECOVERED' &&
    receipt.watchdog_recovery?.same_repository_restored === true &&
    Number.isInteger(receipt.runtime_files_verified) && receipt.runtime_files_verified > 0 &&
    receipt.missing_token_status === 401 && receipt.invalid_token_status === 401 &&
    Number.isInteger(receipt.files_scanned) && receipt.files_scanned > 0;
  fs.rmSync(smokeUserData, { recursive: true, force: true });
  fs.rmSync(smokeRepository, { recursive: true, force: true });
  const servicesStopped = await waitForSmokeServicesToStop();
  if (!timedOut && code === 0 && validReceipt && servicesStopped) {
    console.log('PACKAGED_SMOKE_RECEIPT_PASS', receipt);
    console.log('PACKAGED_SMOKE_SHUTDOWN_PASS', {
      tray_quit_handler_invoked: trayQuitInvoked,
      daemon_port_closed: true,
      cockpit_port_closed: true,
    });
    process.exit(0);
  }
  console.error(`[SynEvid Smoke] Failed with exit code ${code}; receipt valid=${validReceipt}; sidecars stopped=${servicesStopped}.`);
  process.exit(1);
});
