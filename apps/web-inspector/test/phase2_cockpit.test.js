import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import net from 'node:net';
import { spawn, execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, '../../..');

test('FR-P1-008: Repository picker supports desktop, browser, and filesystem-browser paths', () => {
  const modalPath = path.join(REPO_ROOT, 'apps/web/src/components/ConnectRepoModal.tsx');
  const modal = fs.readFileSync(modalPath, 'utf8');
  assert.ok(modal.includes('synevidDesktop?.openDirectory'));
  assert.ok(modal.includes("'showDirectoryPicker' in window"));
  assert.ok(modal.includes('webkitdirectory="true"'));
  assert.ok(modal.includes('api.browseFilesystem(targetPath)'));
  assert.ok(modal.includes('api.connectRepo(normalized'));
});

function getLuminance(hex) {
  const cleanHex = hex.replace('#', '');
  const channels = [0, 2, 4].map((offset) => parseInt(cleanHex.substring(offset, offset + 2), 16) / 255);
  const linear = channels.map((value) => value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
  return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
}

function getContrastRatio(first, second) {
  const luminance = [getLuminance(first), getLuminance(second)].sort((left, right) => right - left);
  return (luminance[0] + 0.05) / (luminance[1] + 0.05);
}

test('FR-P2-001: Minimalist Light & Dark Mode CSS Tokens and WCAG Contrast', () => {
  const cssPath = path.join(REPO_ROOT, 'apps/web/src/app/globals.css');
  assert.ok(fs.existsSync(cssPath), 'globals.css must exist');
  const css = fs.readFileSync(cssPath, 'utf8');
  const themeContext = fs.readFileSync(path.join(REPO_ROOT, 'apps/web/src/context/ThemeContext.tsx'), 'utf8');
  const requiredTokens = [
    '--bg-canvas', '--bg-surface', '--bg-elevated', '--border-hairline', '--border-strong',
    '--text-primary', '--text-secondary', '--text-muted', '--node-verified', '--node-damaged',
    '--node-impacted', '--node-deadcode', '--edge-call', '--edge-import', '--edge-ui-to-api',
  ];
  for (const token of requiredTokens) assert.ok(css.includes(token), `Token ${token} must be defined`);

  const themeBlock = (selector) => {
    const match = css.match(new RegExp(`${selector} \\{([^}]*)\\}`));
    assert.ok(match, `${selector} theme block must exist`);
    return match[1];
  };
  const tokenValue = (block, token) => {
    const match = block.match(new RegExp(`${token}: (#[0-9a-fA-F]{6})`));
    assert.ok(match, `${token} must use a concrete six-digit color`);
    return match[1];
  };
  const themes = [
    { selector: ':root, html\\.light', name: 'light' },
    { selector: 'html\\.dark', name: 'dark' },
  ];
  for (const theme of themes) {
    const block = themeBlock(theme.selector);
    const background = tokenValue(block, '--bg-canvas');
    const primaryText = tokenValue(block, '--text-primary');
    assert.ok(getContrastRatio(primaryText, background) >= 7, `${theme.name} primary text must satisfy WCAG AAA`);
    for (const token of ['--node-verified', '--node-damaged', '--node-impacted', '--node-deadcode']) {
      assert.ok(getContrastRatio(tokenValue(block, token), background) >= 7, `${theme.name} ${token} badge must satisfy WCAG AAA`);
    }
  }

  assert.ok(themeContext.includes("setAttribute('data-theme-switching', 'true')"));
  assert.ok(themeContext.includes('requestAnimationFrame(() => root.removeAttribute'));
  assert.ok(themeContext.includes('markNextPresentedFrame'));
  assert.ok(css.includes('html[data-theme-switching] *'));
  assert.ok(css.includes('transition: none !important'));
});

test('FR-P2-002: All 8 workspaces expose their navigation controls and shortcuts', () => {
  const pagePath = path.join(REPO_ROOT, 'apps/web/src/app/page.tsx');
  const page = fs.readFileSync(pagePath, 'utf8');
  for (const workspaceId of [
    'ws-repos', 'ws-canvas', 'ws-simulator', 'ws-git',
    'ws-trajectory', 'ws-gates', 'ws-ai', 'ws-settings',
  ]) assert.ok(page.includes(workspaceId), `Workspace ${workspaceId} must be declared`);
  assert.ok(page.includes('onClick={() => setActiveWorkspace(ws.id)}'), 'Workspace rail buttons must select their workspace');
  assert.match(page, /Alt\+8/);
});

test('FR-P2-003/004/005: Canvas and editor are wired to real analysis telemetry', () => {
  const api = fs.readFileSync(path.join(REPO_ROOT, 'apps/web/src/lib/api.ts'), 'utf8');
  const page = fs.readFileSync(path.join(REPO_ROOT, 'apps/web/src/app/page.tsx'), 'utf8');
  const canvas = fs.readFileSync(path.join(REPO_ROOT, 'apps/web/src/components/CodeUniverse3D.tsx'), 'utf8');
  const editor = fs.readFileSync(path.join(REPO_ROOT, 'apps/web/src/components/LiveEditSimulator.tsx'), 'utf8');
  assert.ok(api.includes('getUniverse') && api.includes('getEgoNetwork'));
  assert.ok(api.includes('readSourceFile') && api.includes('simulateEdit'));
  assert.ok(page.includes('api.getUniverse') && page.includes('setSelectedFile'));
  assert.ok(page.includes('simulationFilePath={simulatorFilePath}'));
  assert.ok(canvas.includes('requestAnimationFrame(animate)'));
  assert.ok(canvas.includes('data-testid="canvas-fps-meter"'));
  assert.ok(canvas.includes('Canvas presented FPS'));
  assert.ok(canvas.includes('compositor-presented frames per second'));
  assert.ok(canvas.includes('edge.source_line') && canvas.includes('edge.target_line'));
  for (const label of [
    'Reset folder filter for', 'Focus graph node', 'Clear module filter',
    'Filter graph by folder', 'Show all folders in graph', 'Simulate edits for',
    'Zoom out to graph overview',
  ]) assert.ok(canvas.includes(label), `Canvas control must have an accessible label: ${label}`);
  assert.ok(canvas.includes('simulation.damaged_callers.some'));
  assert.ok(canvas.includes('simulation.broken_imports.some'));
  assert.ok(canvas.includes('simulation.newly_dead_symbols.some'));
  assert.ok(canvas.includes('damageOverlayRef.current && nodeState.damaged'));
  assert.ok(canvas.includes('damageOverlayRef.current && nodeState.dead'));
  assert.ok(canvas.includes('damaged && isSimulationEdgeBroken'));
  assert.ok(editor.includes('api.readSourceFile') && editor.includes('api.simulateEdit'));
  assert.ok(editor.includes('150'));
  assert.ok(!editor.includes('PaymentService'));
});

async function reservePort() {
  const probe = net.createServer();
  await new Promise((resolve, reject) => {
    probe.once('error', reject);
    probe.listen(0, '127.0.0.1', resolve);
  });
  const port = probe.address().port;
  await new Promise((resolve, reject) => probe.close((error) => error ? reject(error) : resolve()));
  return port;
}

async function waitForDaemon(baseUrl, child, getStderr) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`Daemon exited early: ${getStderr()}`);
    try {
      const response = await fetch(`${baseUrl}/api/v1/health`, { signal: AbortSignal.timeout(500) });
      if (response.ok) return;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Daemon did not become healthy: ${getStderr()}`);
}

test('FR-P2-004/005: Temporary repository drives graph selection and unsaved simulation end to end', { timeout: 240000 }, async () => {
  execFileSync('cargo', ['build', '-p', 'synevid-ui'], { cwd: REPO_ROOT, stdio: 'pipe' });
  const binary = path.join(REPO_ROOT, 'target', 'debug', process.platform === 'win32' ? 'synevid-ui.exe' : 'synevid-ui');
  assert.ok(fs.existsSync(binary), 'Build must produce the synevid-ui daemon');

  const tempRoot = await fs.promises.mkdtemp(path.join(os.tmpdir(), 'synevid-phase2-e2e-'));
  const daemonRoot = path.join(tempRoot, 'daemon-state');
  const projectRoot = path.join(tempRoot, 'checkout-sample');
  const sourcePath = 'src/services/Pricing.ts';
  const callerPath = 'src/controllers/Checkout.ts';
  const unsupportedPath = 'src/legacy.sql';
  const originalSource = 'function formatSubtotal(amount: number) { return amount; }\nexport function calculateTotal(amount: number) { return formatSubtotal(amount); }\n';
  let child;
  let daemonStderr = '';

  try {
    await fs.promises.mkdir(path.join(projectRoot, 'src/services'), { recursive: true });
    await fs.promises.mkdir(path.join(projectRoot, 'src/controllers'), { recursive: true });
    await fs.promises.writeFile(path.join(projectRoot, sourcePath), originalSource);
    await fs.promises.writeFile(
      path.join(projectRoot, callerPath),
      "import { calculateTotal } from '../services/Pricing';\nexport function checkout() { return calculateTotal(100); }\n",
    );
    await fs.promises.writeFile(path.join(projectRoot, unsupportedPath), 'SELECT 1;\n');

    const port = await reservePort();
    const baseUrl = `http://127.0.0.1:${port}`;
    child = spawn(binary, ['--repo-root', daemonRoot, '--host', '127.0.0.1', '--port', String(port)], {
      cwd: REPO_ROOT,
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => { daemonStderr += chunk; });
    await waitForDaemon(baseUrl, child, () => daemonStderr);

    const connectResponse = await fetch(`${baseUrl}/api/v1/repos/connect`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ root_path: projectRoot, display_name: 'checkout-sample' }),
    });
    const connected = await connectResponse.json();
    assert.equal(connected.status, 'connected', JSON.stringify(connected));
    assert.ok(connected.scan.files_scanned >= 3, 'Connect must scan the temporary repository files');

    const filesResponse = await fetch(`${baseUrl}/api/v1/files`);
    const files = await filesResponse.json();
    assert.ok(files.files.some((file) => file.path === sourcePath));
    assert.ok(files.files.some((file) => file.path === callerPath));
    assert.ok(files.files.some((file) => file.path === unsupportedPath));

    const graphResponse = await fetch(`${baseUrl}/api/v1/graph/universe?dim=2d`);
    const graph = await graphResponse.json();
    assert.equal(graph.repo_id, connected.repo_id, JSON.stringify(graph));
    assert.ok(graph.nodes.some((node) => node.file_path === sourcePath));
    const targetNode = graph.nodes.find((node) => node.file_path === sourcePath && node.label.includes('calculateTotal'));
    assert.ok(targetNode, 'Real parsed symbol must be selectable in the graph');
    const privateHelperNode = graph.nodes.find((node) => node.file_path === sourcePath && node.label.includes('formatSubtotal'));
    assert.ok(privateHelperNode, JSON.stringify(graph.nodes.filter((node) => node.file_path === sourcePath)));
    assert.ok(graph.edges.some((edge) => edge.source === targetNode.id && edge.target === privateHelperNode.id && edge.kind === 'calls'), JSON.stringify(graph.edges.filter((edge) => edge.target === privateHelperNode.id)));
    const callEdge = graph.edges.find((edge) => edge.source_file === callerPath && edge.target_file === sourcePath);
    assert.ok(callEdge, 'Graph must expose the analyzed cross-file caller edge');
    assert.equal(callEdge.source_line, 2);
    assert.equal(callEdge.target_line, 2);

    const egoResponse = await fetch(`${baseUrl}/api/v1/graph/ego?id=${encodeURIComponent(targetNode.id)}`);
    const ego = await egoResponse.json();
    assert.equal(ego.target_node.file_path, sourcePath);
    assert.ok(ego.incoming_callers.some((edge) => edge.source_file === callerPath && edge.source_line === 2 && edge.target_line === 2));

    const sourceResponse = await fetch(`${baseUrl}/api/v1/files/content?path=${encodeURIComponent(sourcePath)}&repo_id=${connected.repo_id}`);
    const source = await sourceResponse.json();
    assert.equal(source.content, originalSource);

    const started = performance.now();
    const simulationResponse = await fetch(`${baseUrl}/api/v1/simulate/edit`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        repo_id: connected.repo_id,
        file_path: sourcePath,
        unsaved_buffer: 'function formatSubtotal(amount: number) { return amount; }\nexport function calculateTotal(amount: number, taxRate: number) { return formatSubtotal(amount + taxRate); }\n',
      }),
    });
    const roundTripMs = performance.now() - started;
    const simulation = await simulationResponse.json();
    assert.equal(simulation.verdict, 'FAIL');
    assert.ok(simulation.damaged_callers.some((caller) => caller.file_path === callerPath && caller.calling_line === 2));
    assert.ok(['verified', 'partial', 'unknown'].includes(simulation.coverage_state.toLowerCase()));
    assert.ok(Number.isFinite(simulation.computation_latency_ms));
    assert.ok(simulation.computation_latency_ms <= 50, `Simulation took ${simulation.computation_latency_ms}ms (round trip ${roundTripMs.toFixed(1)}ms)`);
    assert.ok(roundTripMs <= 150, `Simulation round trip took ${roundTripMs.toFixed(1)}ms`);
    assert.equal(await fs.promises.readFile(path.join(projectRoot, sourcePath), 'utf8'), originalSource, 'Unsaved buffer must not alter repository source');

    const deadCodeResponse = await fetch(`${baseUrl}/api/v1/simulate/edit`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        repo_id: connected.repo_id,
        file_path: sourcePath,
        unsaved_buffer: 'function formatSubtotal(amount: number) { return amount; }\nexport function calculateTotal(amount: number) { return amount; }\n',
      }),
    });
    const deadCode = await deadCodeResponse.json();
    assert.ok(deadCode.newly_dead_symbols.length > 0, JSON.stringify(deadCode));

    const deletedExportResponse = await fetch(`${baseUrl}/api/v1/simulate/edit`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        repo_id: connected.repo_id,
        file_path: sourcePath,
        unsaved_buffer: 'function formatSubtotal(amount: number) { return amount; }\nexport function calculateTax(amount: number) { return formatSubtotal(amount); }\n',
      }),
    });
    const deletedExport = await deletedExportResponse.json();
    assert.ok(deletedExport.broken_imports.some((diagnostic) => diagnostic.includes(callerPath) && diagnostic.includes('calculateTotal')), JSON.stringify(deletedExport));
    assert.ok(deletedExport.damaged_callers.some((caller) => caller.file_path === callerPath));

    const syntaxResponse = await fetch(`${baseUrl}/api/v1/simulate/edit`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        repo_id: connected.repo_id,
        file_path: sourcePath,
        unsaved_buffer: 'export function calculateTotal( {\n',
      }),
    });
    const syntaxResult = await syntaxResponse.json();
    assert.equal(syntaxResult.verdict, 'FAIL');
    assert.ok(syntaxResult.syntax_errors.length > 0, 'Invalid draft syntax must be reported by the parser');

    const unsupportedResponse = await fetch(`${baseUrl}/api/v1/simulate/edit`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        repo_id: connected.repo_id,
        file_path: unsupportedPath,
        unsaved_buffer: 'SELECT 2;\n',
      }),
    });
    const unsupported = await unsupportedResponse.json();
    assert.ok(['unknown', 'partial'].includes(unsupported.coverage_state.toLowerCase()));
    assert.notEqual(unsupported.verdict, 'PASS', 'Unsupported analysis must never be reported as PASS');
    assert.ok((unsupported.syntax_errors || []).some((diagnostic) => diagnostic.includes('no language adapter')));
    assert.equal(unsupported.candidate_snapshot_id, null, 'Unsupported drafts do not fabricate a candidate snapshot');
    assert.ok(unsupported.computation_latency_ms <= 50, `Unsupported draft returned in ${unsupported.computation_latency_ms}ms`);
    assert.equal(await fs.promises.readFile(path.join(projectRoot, unsupportedPath), 'utf8'), 'SELECT 1;\n');

    const traversalResponse = await fetch(`${baseUrl}/api/v1/files/content?path=..%2Foutside.txt`);
    const traversal = await traversalResponse.json();
    assert.equal(traversal.error_code, 'ERR-FS-400');

    if (process.platform !== 'win32') {
      const outsidePath = path.join(tempRoot, 'outside.ts');
      await fs.promises.writeFile(outsidePath, 'export const outside = true;\n');
      await fs.promises.symlink(outsidePath, path.join(projectRoot, 'src/services/escape.ts'));
      const escapeResponse = await fetch(`${baseUrl}/api/v1/files/content?path=${encodeURIComponent('src/services/escape.ts')}`);
      const escape = await escapeResponse.json();
      assert.equal(escape.error_code, 'ERR-FS-403');
    }
  } finally {
    if (child && child.exitCode === null) {
      child.kill('SIGTERM');
      await Promise.race([
        new Promise((resolve) => child.once('exit', resolve)),
        new Promise((resolve) => setTimeout(resolve, 3000)),
      ]);
    }
    await fs.promises.rm(tempRoot, { recursive: true, force: true });
  }
});
