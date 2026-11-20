const path = require('node:path');
const { createRequire } = require('node:module');
const os = require('node:os');
const fs = require('node:fs');
const requireElectron = createRequire(path.join(__dirname, '../../desktop-electron/package.json'));
const { app, BrowserWindow } = requireElectron('electron');

const nextOrigin = process.env.SYNEVID_BROWSER_NEXT_ORIGIN;
const daemonPort = Number(process.env.SYNEVID_BROWSER_DAEMON_PORT);
const expectedRoot = process.env.SYNEVID_BROWSER_ROOT_PATH;
const expectedSeq = Number(process.env.SYNEVID_BROWSER_EXPECTED_SEQ || 0);
const publishEvent = process.env.SYNEVID_BROWSER_PUBLISH_EVENT === '1';

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForJavaScript(window, expression, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await window.webContents.executeJavaScript(expression)) return;
    await delay(100);
  }
  throw new Error(`Timed out waiting for browser expression: ${expression}`);
}

async function publishLiveEvent() {
  const response = await fetch(`http://127.0.0.1:${daemonPort}/api/v1/live/broadcast`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      event: 'phase1.browser.real_process',
      data: { source: 'electron-real-daemon-e2e' },
    }),
  });
  if (!response.ok) throw new Error(`live broadcast failed with HTTP ${response.status}`);
  const payload = await response.json();
  if (!Number.isInteger(payload.seq_id)) throw new Error(`live broadcast returned no sequence: ${JSON.stringify(payload)}`);
  return payload.seq_id;
}

async function main() {
  if (!nextOrigin || !daemonPort || !expectedRoot) throw new Error('Browser runner environment is incomplete');
  app.setPath('userData', fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-electron-phase1-')));
  app.commandLine.appendSwitch('disable-gpu');
  app.commandLine.appendSwitch('headless');
  if (process.platform === 'linux' && process.env.CI) app.commandLine.appendSwitch('no-sandbox');
  await app.whenReady();

  const window = new BrowserWindow({
    show: false,
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  const daemonOrigin = `http://127.0.0.1:${daemonPort}`;
  const pageUrl = `${nextOrigin}${nextOrigin.includes('?') ? '&' : '?'}synevid_daemon=${encodeURIComponent(daemonOrigin)}`;
  await window.loadURL(pageUrl);

  try {
    await waitForJavaScript(window, `document.body.innerText.includes(${JSON.stringify(expectedRoot)})`);
  } catch (error) {
    const body = await window.webContents.executeJavaScript('document.body.innerText');
    throw new Error(`${error.message}; body=${JSON.stringify(body.slice(-3000))}`);
  }
  await waitForJavaScript(window, "document.body.innerText.includes('main.ts')");
  const publishedSeq = publishEvent ? await publishLiveEvent() : expectedSeq;
  if (publishedSeq > 0) {
    await waitForJavaScript(window, `document.body.innerText.includes(${JSON.stringify(`LIVE #${publishedSeq}`)})`);
  }

  const body = await window.webContents.executeJavaScript('document.body.innerText');
  const result = {
    canonicalPathVisible: body.includes(expectedRoot),
    replayedSequenceVisible: publishedSeq === 0 || body.includes(`LIVE #${publishedSeq}`),
    repositoryFilesVisible: body.includes('main.ts'),
    graphUnavailable: body.includes('Repository analysis unavailable'),
    publishedSeq,
  };
  console.log(`SYNEVID_PHASE1_BROWSER_RESULT=${JSON.stringify(result)}`);
  window.destroy();
  app.quit();
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  app.exit(1);
});
