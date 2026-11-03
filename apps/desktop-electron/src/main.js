const { app, BrowserWindow, ipcMain, dialog, session, Menu, Tray, nativeImage } = require('electron');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const http = require('http');
const crypto = require('crypto');

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

// React's development build uses eval() only to reconstruct cross-environment
// call stacks. Keep that capability limited to the source-run development
// shell; packaged/production Electron stays on the strict CSP.
function buildContentSecurityPolicy(liveDevelopment) {
  const developmentEvaluation = liveDevelopment ? " 'unsafe-eval'" : '';
  return [
    "default-src 'self' data: blob: http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001 http://127.0.0.1:3333",
    `script-src 'self' 'unsafe-inline'${developmentEvaluation} http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001`,
    "style-src 'self' 'unsafe-inline' http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001",
    "img-src 'self' data: blob: http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001",
    "font-src 'self' data: http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001",
    'connect-src \'self\' http://localhost:3000 http://127.0.0.1:3000 http://localhost:3001 http://127.0.0.1:3001 http://127.0.0.1:3333 ws://localhost:3000 ws://127.0.0.1:3000 ws://localhost:3001 ws://127.0.0.1:3001 ws://127.0.0.1:3333',
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
        env: { ...process.env, SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN },
      });
    } else if (existingBinary) {
      daemonProcess = spawn(existingBinary, ['--host', '127.0.0.1', '--port', String(DAEMON_PORT), '--repo-root', daemonRepositoryRoot()], {
        cwd: daemonRepositoryRoot(),
        stdio: 'ignore',
        detached: false,
        windowsHide: true,
        env: { ...process.env, SYNEVID_LOOPBACK_TOKEN: LOOPBACK_TOKEN },
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
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [
          buildContentSecurityPolicy(LIVE_DEVELOPMENT),
        ],
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
          await shellWindow.loadURL(COCKPIT_URL);
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
ipcMain.handle('dialog:openDirectory', async () => {
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

ipcMain.handle('daemon:status', async () => {
  const daemonHealthy = await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`, 1500, {
    Authorization: `Bearer ${LOOPBACK_TOKEN}`,
  });
  const webHealthy = await checkServiceHealth(COCKPIT_URL);
  return {
    port: DAEMON_PORT,
    daemonOnline: daemonHealthy,
    cockpitOnline: webHealthy,
    cockpitUrl: COCKPIT_URL,
    loopbackToken: LOOPBACK_TOKEN,
  };
});

ipcMain.handle('daemon:token', () => LOOPBACK_TOKEN);
ipcMain.on('daemon:token-sync', (event) => {
  event.returnValue = LOOPBACK_TOKEN;
});

ipcMain.handle('frame-meter:start', () => startFrameMeter());
ipcMain.handle('frame-meter:stats', () => readFrameMeterStats());
ipcMain.handle('frame-meter:stop', () => stopFrameMeter());
ipcMain.handle('frame-meter:mark', () => markNextPresentedFrame());
ipcMain.on('frame-meter:mark-sync', (event) => {
  event.returnValue = markNextPresentedFrame();
});

ipcMain.on('window:minimize', () => {
  if (mainWindow) mainWindow.minimize();
});

ipcMain.on('window:maximize', () => {
  if (mainWindow) {
    if (mainWindow.isMaximized()) {
      mainWindow.unmaximize();
    } else {
      mainWindow.maximize();
    }
  }
});

ipcMain.on('window:close', () => {
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
    const smokeReady = await waitForApplicationHealth();
    console.log('PACKAGED_SMOKE_COMPONENTS', {
      daemon: await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`, 1500, {
        Authorization: `Bearer ${LOOPBACK_TOKEN}`,
      }),
      cockpit: await checkServiceHealth(COCKPIT_URL),
    });
    console.log(smokeReady ? 'PACKAGED_SMOKE_PASS' : 'PACKAGED_SMOKE_FAIL');
    if (!smokeReady) showDaemonErrorBanner('Packaged runtime health check failed');
    setTimeout(() => app.quit(), smokeReady ? 50 : 250);
  }

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  shuttingDown = true;
  if (daemonRestartTimer) clearTimeout(daemonRestartTimer);
  if (daemonProcess) {
    try {
      daemonProcess.kill();
    } catch {}
  }
  if (webProcess) {
    try {
      webProcess.kill();
    } catch {}
  }
  if (process.platform !== 'darwin') {
    app.quit();
  }
});
