const path = require('node:path');
const { createRequire } = require('node:module');
const os = require('node:os');
const fs = require('node:fs');
const requireElectron = createRequire(path.join(__dirname, '../../desktop-electron/package.json'));
const { app, BrowserWindow } = requireElectron('electron');

const nextOrigin = process.env.SYNEVID_BROWSER_NEXT_ORIGIN;
const daemonPort = Number(process.env.SYNEVID_BROWSER_DAEMON_PORT);
const expectedRoot = process.env.SYNEVID_BROWSER_ROOT_PATH;
const expectedRootMarker = path.basename(expectedRoot || '').replace(/[\\/]+$/, '');
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
  const bodyTail = await window.webContents.executeJavaScript('document.body.innerText.slice(-1800)');
  throw new Error(`Timed out waiting for browser expression: ${expression}; body=${JSON.stringify(bodyTail)}`);
}

function repositoryIdentityVisible(body) {
  const normalizedBody = body.replaceAll('\\', '/').toLowerCase();
  const normalizedRoot = expectedRoot.replaceAll('\\', '/').toLowerCase();
  return normalizedBody.includes(normalizedRoot) || normalizedBody.includes(expectedRootMarker.toLowerCase());
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
  if (process.platform !== 'darwin') app.commandLine.appendSwitch('disable-gpu');
  if (process.platform !== 'darwin') app.commandLine.appendSwitch('headless');
  if (process.platform === 'linux' && process.env.CI) app.commandLine.appendSwitch('no-sandbox');
  await app.whenReady();

  const window = new BrowserWindow({
    show: false,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      offscreen: false,
    },
  });
  const rendererDiagnostics = [];
  window.webContents.on('did-fail-load', (_event, code, description, url, isMainFrame) => {
    rendererDiagnostics.push({ type: 'did-fail-load', code, description, url, isMainFrame });
  });
  window.webContents.on('render-process-gone', (_event, details) => {
    rendererDiagnostics.push({ type: 'render-process-gone', ...details });
  });
  window.webContents.on('console-message', (details) => {
    rendererDiagnostics.push({ level: details.level, message: details.message });
  });
  const daemonOrigin = `http://127.0.0.1:${daemonPort}`;
  const pageUrl = `${nextOrigin}${nextOrigin.includes('?') ? '&' : '?'}synevid_daemon=${encodeURIComponent(daemonOrigin)}`;
  await window.loadURL(pageUrl);

  try {
    await waitForJavaScript(window, `(() => {
      const body = document.body.innerText.replaceAll('\\\\', '/').toLowerCase();
      return body.includes(${JSON.stringify(expectedRoot.replaceAll('\\', '/').toLowerCase())})
        || body.includes(${JSON.stringify(expectedRootMarker.toLowerCase())});
    })()`);
  } catch (error) {
    const body = await window.webContents.executeJavaScript('document.body.innerText');
    const diagnostics = await window.webContents.executeJavaScript(`(async () => {
      const configured = new URL(window.location.href).searchParams.get('synevid_daemon');
      const requests = await Promise.all(['/api/v1/health', '/api/v1/status', '/api/v1/repos/list', '/api/v1/capabilities'].map(async (route) => {
        try {
          const response = await fetch(new URL(route, configured), {
            headers: { 'X-Synevid-Client-Id': 'ui:web', 'X-Synevid-Client-Type': 'ui' },
          });
          return { route, status: response.status, body: (await response.text()).slice(0, 1000) };
        } catch (failure) {
          return { route, error: String(failure) };
        }
      }));
      return { href: window.location.href, configured, stored: localStorage.getItem('synevid_api_url'), requests };
    })()`);
    throw new Error(`${error.message}; diagnostics=${JSON.stringify(diagnostics)}; console=${JSON.stringify(rendererDiagnostics.slice(-30))}; body=${JSON.stringify(body.slice(-3000))}`);
  }
  await waitForJavaScript(window, "document.body.innerText.includes('main.ts')");
  const repositoriesButton = window.webContents.executeJavaScript(`(() => {
    const button = [...document.querySelectorAll('button')].find((item) => item.title.startsWith('Repositories'));
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!(await repositoriesButton)) throw new Error('Repositories workspace button is unavailable');
  await waitForJavaScript(window, "document.body.innerText.includes('Analytics & Reports')");
  const reportButtonClicked = await window.webContents.executeJavaScript(`(() => {
    const button = [...document.querySelectorAll('button')].find((item) => item.innerText.trim() === 'JSON');
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!reportButtonClicked) throw new Error('Analytics JSON report control is unavailable');
  await waitForJavaScript(window, "document.body.innerText.includes('Report receipt:')", 60_000);
  const displayedReportId = await window.webContents.executeJavaScript(`(() => {
    for (const pre of document.querySelectorAll('pre')) {
      try {
        const value = JSON.parse(pre.innerText);
        if (typeof value.report_id === 'string') return value.report_id;
      } catch {}
    }
    return null;
  })()`);
  if (!displayedReportId) throw new Error('Rendered JSON report has no report identity');
  const directHistoryResponse = await fetch(`http://127.0.0.1:${daemonPort}/api/v1/analytics/report-history?limit=50`);
  const directHistoryBody = await directHistoryResponse.json();
  if (!directHistoryBody.reports?.some((entry) => entry.report_id === displayedReportId)) {
    throw new Error(`Daemon history does not contain the rendered report identity: ${JSON.stringify({ displayedReportId, directHistoryBody })}`);
  }
  const directReceiptResponse = await fetch(`http://127.0.0.1:${daemonPort}/api/v1/analytics/report-history/${encodeURIComponent(displayedReportId)}`);
  const directReceiptBody = await directReceiptResponse.json();
  if (directReceiptBody.error_code) {
    throw new Error(`Daemon could not retrieve the report it just rendered: ${JSON.stringify(directReceiptBody)}`);
  }
  if (directReceiptBody.report_id !== displayedReportId
    || directReceiptBody.evidence_digest !== directReceiptBody.report?.result_digest) {
    throw new Error(`Rendered report receipt identity/digest mismatch: ${JSON.stringify(directReceiptBody)}`);
  }
  const receiptButtonClicked = await window.webContents.executeJavaScript(`(() => {
    const button = [...document.querySelectorAll('button')].find((item) => item.innerText.includes('Load persisted receipt'));
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!receiptButtonClicked) throw new Error('Persisted analytics receipt control is unavailable');
  const receiptStateReady = `document.body.innerText.includes('"created_at"') || Boolean(document.querySelector('#analytics-evidence-receipt [role="alert"]'))`;
  await waitForJavaScript(window, receiptStateReady, 30_000);
  const receiptVisible = await window.webContents.executeJavaScript(`document.body.innerText.includes('"created_at"') && document.body.innerText.includes('"evidence_digest"')`);
  if (!receiptVisible) {
    const body = await window.webContents.executeJavaScript('document.body.innerText.slice(-1800)');
    throw new Error(`Persisted report receipt was not returned: ${body}`);
  }
  const historyButtonClicked = await window.webContents.executeJavaScript(`(() => {
    const button = [...document.querySelectorAll('button')].find((item) => item.innerText.includes('Load report history'));
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!historyButtonClicked) throw new Error('Analytics history control is unavailable');
  const historyStateReady = `(() => {
    const body = document.body.innerText;
    const row = [...document.querySelectorAll('button')].some((item) => item.innerText.includes('Snapshot') && item.innerText.includes('blake3:'));
    return row || body.includes('No reports are stored for this repository yet.') || body.includes('Report history is unavailable');
  })()`;
  await waitForJavaScript(window, historyStateReady, 30_000);
  const historyState = await window.webContents.executeJavaScript(`(() => ({
    rowVisible: [...document.querySelectorAll('button')].some((item) => item.innerText.includes('Snapshot') && item.innerText.includes('blake3:')),
    bodyTail: document.body.innerText.slice(-2500)
  }))()`);
  if (!historyState.rowVisible) throw new Error(`Analytics history did not return the generated report: ${JSON.stringify(historyState)}`);
  const publishedSeq = publishEvent ? await publishLiveEvent() : expectedSeq;
  if (publishedSeq > 0) {
    const cursorExpression = `(() => {
      const match = document.body.innerText.match(/LIVE #(\\d+)/);
      return Boolean(match && Number(match[1]) >= ${publishedSeq});
    })()`;
    try {
      await waitForJavaScript(window, cursorExpression);
    } catch (error) {
      const body = await window.webContents.executeJavaScript('document.body.innerText');
      const pageState = {
        url: window.webContents.getURL(),
        title: window.webContents.getTitle(),
        isLoading: window.webContents.isLoading(),
        isLoadingMainFrame: window.webContents.isLoadingMainFrame(),
        rendererCrashed: window.webContents.isCrashed(),
      };
      throw new Error(`${error.message}; page=${JSON.stringify(pageState)}; diagnostics=${JSON.stringify(rendererDiagnostics.slice(-50))}; body=${JSON.stringify(body.slice(-4000))}`);
    }
  }

  const body = await window.webContents.executeJavaScript('document.body.innerText');
  const liveCursor = Number(body.match(/LIVE #(\d+)/)?.[1] || 0);
  const canonicalPathVisible = repositoryIdentityVisible(body);
  if (!canonicalPathVisible) {
    throw new Error(`Repository identity disappeared after load: expected=${expectedRoot}; marker=${expectedRootMarker}; url=${window.webContents.getURL()}; diagnostics=${JSON.stringify(rendererDiagnostics.slice(-30))}; body=${JSON.stringify(body.slice(-4000))}`);
  }
  const result = {
    canonicalPathVisible,
    replayedSequenceVisible: publishedSeq === 0 || liveCursor >= publishedSeq,
    repositoryFilesVisible: body.includes('main.ts'),
    graphUnavailable: body.includes('Repository analysis unavailable'),
    analyticsReportVisible: body.includes('Analytics & Reports') && body.includes('Report receipt:'),
    persistedAnalyticsReceiptVisible: body.includes('"created_at"') && body.includes('"evidence_digest"'),
    analyticsHistoryVisible: [...(await window.webContents.executeJavaScript('([...document.querySelectorAll("button")].map((item) => item.innerText))'))].some((label) => label.includes('Snapshot') && label.includes('blake3:')),
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
