const { app, BrowserWindow, ipcMain, dialog, session } = require('electron');
const path = require('path');
const { spawn } = require('child_process');
const http = require('http');

let mainWindow = null;
let daemonProcess = null;
let webProcess = null;
const DAEMON_PORT = process.env.SYNEVID_PORT || 3333;
const COCKPIT_URL = process.env.COCKPIT_URL || 'http://127.0.0.1:3000';

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
function checkServiceHealth(targetUrl, timeoutMs = 800) {
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
          resolve(res.statusCode < 500);
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

// Ensure sidecar daemon is running
async function ensureSidecarDaemon() {
  const isHealthy = await checkServiceHealth(`http://127.0.0.1:${DAEMON_PORT}/api/v1/health`);
  if (isHealthy) {
    console.log(`[SynEvid Desktop] Reusing active synevid-ui daemon on port ${DAEMON_PORT}`);
    return;
  }

  const repoRoot = path.resolve(__dirname, '../../..');
  console.log(`[SynEvid Desktop] Spawning synevid-ui sidecar for repo: ${repoRoot}`);

  try {
    const isWslUnc =
      process.platform === 'win32' &&
      (repoRoot.startsWith('\\\\wsl.localhost') ||
        repoRoot.startsWith('\\\\wsl$') ||
        repoRoot.startsWith('//wsl.localhost') ||
        repoRoot.startsWith('//wsl$'));

    if (isWslUnc) {
      const linuxRepoRoot = toLinuxPath(repoRoot);
      console.log(`[SynEvid Desktop] Spawning Linux daemon via WSL from ${linuxRepoRoot}`);
      daemonProcess = spawn(
        'wsl.exe',
        ['-d', 'Ubuntu', 'bash', '-lc', `cd "${linuxRepoRoot}" && ./target/debug/synevid-ui --port ${DAEMON_PORT} > /tmp/synevid-ui.log 2>&1`],
        { stdio: 'ignore', detached: true }
      );
    } else {
      const daemonBinary = path.join(
        repoRoot,
        'target/debug/synevid-ui' + (process.platform === 'win32' ? '.exe' : '')
      );
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
  console.log(`[SynEvid Desktop] Spawning Next.js Cockpit server on port 3000...`);

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
      webProcess = spawn(
        'wsl.exe',
        ['-d', 'Ubuntu', 'bash', '-lc', `cd "${linuxRepoRoot}/apps/web" && npx next dev -p 3000 -H 0.0.0.0 > /tmp/next-dev.log 2>&1`],
        { stdio: 'ignore', detached: true }
      );
    } else {
      const webDir = path.join(repoRoot, 'apps/web');
      const cmd = process.platform === 'win32' ? 'npx.cmd' : 'npx';
      webProcess = spawn(cmd, ['next', 'dev', '-p', '3000', '-H', '0.0.0.0'], {
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
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1024,
    minHeight: 700,
    backgroundColor: '#090A0F',
    title: 'SynEvid — Code Autopsy Visual Cockpit',
    autoHideMenuBar: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  // Strict CSP: Allow strictly local loopback traffic (INV-SANDBOX-004)
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        'Content-Security-Policy': [
          "default-src 'self' 'unsafe-inline' 'unsafe-eval' http://localhost:3000 http://127.0.0.1:3000 http://127.0.0.1:3333 ws://localhost:3000 ws://127.0.0.1:3000 ws://127.0.0.1:3333 data: blob:; connect-src 'self' http://localhost:3000 http://127.0.0.1:3000 http://127.0.0.1:3333 ws://localhost:3000 ws://127.0.0.1:3000 ws://127.0.0.1:3333;",
        ],
      },
    });
  });

  // Show immediate splash screen while services connect
  const splashDataUrl = 'data:text/html;charset=utf-8,' + encodeURIComponent(SPLASH_HTML);
  mainWindow.loadURL(splashDataUrl);

  // Catch any navigation failure and retry rather than leaving a blank screen
  mainWindow.webContents.on('did-fail-load', (event, errorCode, errorDescription, validatedURL) => {
    console.warn(`[SynEvid Desktop] Load failed: ${validatedURL} (${errorCode}: ${errorDescription})`);
    setTimeout(async () => {
      const ready = await checkServiceHealth(COCKPIT_URL);
      if (ready && mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.loadURL(COCKPIT_URL).catch(() => {});
      }
    }, 1500);
  });

  // Poll for Cockpit readiness and transition cleanly
  let attempts = 0;
  const maxAttempts = 60; // 30s max
  const interval = setInterval(async () => {
    attempts++;
    const ready = await checkServiceHealth(COCKPIT_URL);
    if (ready) {
      clearInterval(interval);
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.loadURL(COCKPIT_URL);
      }
    } else if (attempts >= maxAttempts) {
      clearInterval(interval);
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.loadURL(COCKPIT_URL).catch(() => {});
      }
    }
  }, 500);

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
