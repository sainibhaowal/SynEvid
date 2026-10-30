const { app, BrowserWindow, ipcMain, dialog, session } = require('electron');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const http = require('http');

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
const DAEMON_PORT = process.env.SYNEVID_PORT || 3333;
const COCKPIT_URL = process.env.COCKPIT_URL || 'http://127.0.0.1:3000';

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
    <span class="badge">v0.5.0</span>
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

function toLinuxPath(winOrUncPath) {
  const normalized = winOrUncPath.replace(/\\/g, '/');
  const wslMatch = normalized.match(/^\/\/(?:wsl\.localhost|wsl\$)\/[^/]+(\/.*)$/i);
  if (wslMatch) {
    return wslMatch[1];
  }
  const driveMatch = normalized.match(/^([a-zA-Z]):(\/.*)$/);
  if (driveMatch) {
    return `/mnt/${driveMatch[1].toLowerCase()}${driveMatch[2]}`;
  }
  return normalized;
}

// Check health of any HTTP service
function checkServiceHealth(targetUrl, timeoutMs = 1500) {
  return new Promise((resolve) => {
    try {
      const u = new URL(targetUrl);
      const req = http.get(
        {
          hostname: u.hostname === 'localhost' ? '127.0.0.1' : u.hostname,
          port: u.port,
          path: u.pathname,
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
    checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`),
    checkServiceHealth(COCKPIT_URL),
  ]);
  return daemonHealthy && cockpitHealthy;
}

const SPLASH_DATA_URL = 'data:text/html;charset=utf-8,' + encodeURIComponent(SPLASH_HTML);

// Ensure sidecar daemon is running
async function ensureSidecarDaemon() {
  const isHealthy = await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`);
  if (isHealthy) {
    console.log(`[SynEvid Desktop] Reusing active synevid-ui daemon on port ${DAEMON_PORT}`);
    return;
  }

  const repoRoot = path.resolve(__dirname, '../../..');
  const isWslUnc =
    process.platform === 'win32' &&
    (repoRoot.startsWith('\\\\wsl.localhost') ||
      repoRoot.startsWith('\\\\wsl$') ||
      repoRoot.startsWith('//wsl.localhost') ||
      repoRoot.startsWith('//wsl$'));
  const executableName = process.platform === 'win32' && !isWslUnc ? 'synevid-ui.exe' : 'synevid-ui';
  const daemonBuild = fs.existsSync(path.join(repoRoot, 'target', 'release', executableName))
    ? 'release'
    : 'debug';
  console.log(`[SynEvid Desktop] Spawning synevid-ui sidecar for repo: ${repoRoot}`);

  try {
    if (isWslUnc) {
      const linuxRepoRoot = toLinuxPath(repoRoot);
      console.log(`[SynEvid Desktop] Spawning Linux daemon via WSL from ${linuxRepoRoot}`);
      daemonProcess = spawn(
        'wsl.exe',
        ['-d', 'Ubuntu', 'bash', '-lc', `cd "${linuxRepoRoot}" && ./target/${daemonBuild}/synevid-ui --port ${DAEMON_PORT} > /tmp/synevid-ui.log 2>&1`],
        { stdio: 'ignore', detached: false, windowsHide: true }
      );
    } else {
      const daemonBinary = path.join(repoRoot, 'target', daemonBuild, executableName);
      daemonProcess = spawn(daemonBinary, ['--port', String(DAEMON_PORT)], {
        cwd: repoRoot,
        stdio: 'ignore',
        detached: false,
      });
    }

    daemonProcess.on('error', (err) => {
      console.warn('[SynEvid Desktop] Sidecar daemon spawn warning:', err.message);
    });
  } catch (err) {
    console.warn('[SynEvid Desktop] Sidecar spawn exception:', err.message);
  }
}

// Ensure Next.js Cockpit is running
async function ensureWebCockpit() {
  const isHealthy = await checkServiceHealth(COCKPIT_URL);
  if (isHealthy) {
    console.log(`[SynEvid Desktop] Reusing active Cockpit on ${COCKPIT_URL}`);
    return;
  }

  const repoRoot = path.resolve(__dirname, '../../..');
  const webDir = path.join(repoRoot, 'apps/web');
  const productionBuildExists = fs.existsSync(path.join(webDir, '.next', 'BUILD_ID'));
  const nextMode = productionBuildExists ? 'start' : 'dev';
  console.log('[SynEvid Desktop] Spawning Next.js ' + nextMode + ' server on port 3000...');

  try {
    const isWslUnc =
      process.platform === 'win32' &&
      (repoRoot.startsWith('\\\\wsl.localhost') ||
        repoRoot.startsWith('\\\\wsl$') ||
        repoRoot.startsWith('//wsl.localhost') ||
        repoRoot.startsWith('//wsl$'));

    if (isWslUnc) {
      const linuxRepoRoot = toLinuxPath(repoRoot);
      console.log(`[SynEvid Desktop] Spawning Next.js via WSL at ${linuxRepoRoot}/apps/web`);
      const startCommand =
        'cd "' + linuxRepoRoot + '/apps/web" && npx next ' + nextMode + ' -p 3000 -H 0.0.0.0 > /tmp/next-' + nextMode + '.log 2>&1';
      webProcess = spawn(
        'wsl.exe',
        ['-d', 'Ubuntu', 'bash', '-lc', startCommand],
        { stdio: 'ignore', detached: false, windowsHide: true }
      );
    } else {
      const cmd = process.platform === 'win32' ? 'npx.cmd' : 'npx';
      webProcess = spawn(cmd, ['next', nextMode, '-p', '3000', '-H', '0.0.0.0'], {
        cwd: webDir,
        stdio: 'ignore',
        detached: false,
      });
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
          "default-src 'self' data: blob: http://localhost:3000 http://127.0.0.1:3000 http://127.0.0.1:3333; script-src 'self' 'unsafe-inline' http://localhost:3000 http://127.0.0.1:3000; style-src 'self' 'unsafe-inline' http://localhost:3000 http://127.0.0.1:3000; img-src 'self' data: blob: http://localhost:3000 http://127.0.0.1:3000; font-src 'self' data: http://localhost:3000 http://127.0.0.1:3000; connect-src 'self' http://localhost:3000 http://127.0.0.1:3000 http://127.0.0.1:3333 ws://localhost:3000 ws://127.0.0.1:3000 ws://127.0.0.1:3333;",
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

  startCockpit().catch((err) => {
    console.warn('Cockpit startup failed:', err.message);
  });

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
  const selected = result.filePaths[0];
  const wslMatch = selected.match(/^\\\\(?:wsl\.localhost|wsl\$)\\[^\\]+(\\.*)$/i);
  if (wslMatch) {
    return wslMatch[1].replace(/\\/g, '/');
  }
  return selected;
});

ipcMain.handle('daemon:status', async () => {
  const daemonHealthy = await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`);
  const webHealthy = await checkServiceHealth(COCKPIT_URL);
  return {
    port: DAEMON_PORT,
    daemonOnline: daemonHealthy,
    cockpitOnline: webHealthy,
    cockpitUrl: COCKPIT_URL,
  };
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

app.whenReady().then(async () => {
  createWindow();
  await ensureSidecarDaemon();
  await ensureWebCockpit();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
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
