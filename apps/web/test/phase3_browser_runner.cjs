const path = require('node:path');
const { createRequire } = require('node:module');
const os = require('node:os');
const fs = require('node:fs');
const requireElectron = createRequire(path.join(__dirname, '../../desktop-electron/package.json'));
const { app, BrowserWindow, session } = requireElectron('electron');

const nextOrigin = process.env.SYNEVID_BROWSER_NEXT_ORIGIN;
const fixturePort = Number(process.env.SYNEVID_BROWSER_FIXTURE_PORT);

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

async function fixtureCalls() {
  const response = await fetch(`http://127.0.0.1:${fixturePort}/__calls`);
  return response.json();
}

async function waitForFixturePath(path) {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const calls = await fixtureCalls();
    if (calls.some((call) => call.path === path)) return calls;
    await delay(100);
  }
  throw new Error(`Timed out waiting for fixture call ${path}; calls=${JSON.stringify(await fixtureCalls())}`);
}

async function main() {
  if (!nextOrigin || !fixturePort) throw new Error('Browser runner environment is incomplete');
  app.setPath('userData', fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-electron-browser-')));
  app.commandLine.appendSwitch('disable-gpu');
  app.commandLine.appendSwitch('headless');
  await app.whenReady();

  session.defaultSession.webRequest.onBeforeRequest({ urls: ['http://127.0.0.1:3333/*'] }, (details, callback) => {
    const source = new URL(details.url);
    callback({ redirectURL: `http://127.0.0.1:${fixturePort}${source.pathname}${source.search}` });
  });

  const window = new BrowserWindow({
    show: false,
    webPreferences: { contextIsolation: true, nodeIntegration: false, sandbox: true },
  });
  await window.loadURL(nextOrigin);
  await waitForJavaScript(window, `document.querySelector('button[title^="Git Galaxy"]') !== null`);
  await window.webContents.executeJavaScript(`document.querySelector('button[title^="Git Galaxy"]').click()`);
  await waitForJavaScript(window, `document.body.innerText.includes('Evolutionary Topology')`);

  await window.webContents.executeJavaScript(`(() => {
    const button = [...document.querySelectorAll('button')].find((candidate) => candidate.textContent.includes('Range Mode'));
    if (!button) throw new Error('Range Mode button missing');
    button.click();
  })()`);
  await waitForFixturePath(`/api/v1/git/diff/${'a'.repeat(40)}/${'b'.repeat(40)}`);

  await waitForJavaScript(window, `document.querySelector('input[type="range"]') !== null`);
  await window.webContents.executeJavaScript(`(() => {
    const input = document.querySelector('input[type="range"]');
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    // The component maps slider position 0 to the oldest commit.
    setter.call(input, '0');
    input.focus();
    input.dispatchEvent(new Event('input', { bubbles: true }));
    input.dispatchEvent(new Event('change', { bubbles: true }));
    input.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: '0' }));
    input.click();
  })()`);
  await waitForFixturePath(`/api/v1/git/time-travel/${'a'.repeat(40)}?dim=3d`);

  const calls = await fixtureCalls();
  const body = await window.webContents.executeJavaScript('document.body.innerText');
  const result = {
    sliderSelectedBaseline: calls.some((call) => call.path === `/api/v1/git/time-travel/${'a'.repeat(40)}?dim=3d`),
    rangeComparisonLoaded: body.includes('Range Structural Blast') && calls.some((call) => call.path === `/api/v1/git/diff/${'a'.repeat(40)}/${'b'.repeat(40)}`),
    calls,
  };
  console.log(`SYNEVID_BROWSER_RESULT=${JSON.stringify(result)}`);
  window.destroy();
  app.quit();
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  app.exit(1);
});
