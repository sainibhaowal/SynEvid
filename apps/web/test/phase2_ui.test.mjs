import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');

function readSource(relativePath) {
  const filePath = path.join(repoRoot, relativePath);
  const content = fs.readFileSync(filePath, 'utf8');
  const scriptKind = filePath.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS;
  return ts.createSourceFile(filePath, content, ts.ScriptTarget.Latest, true, scriptKind);
}

function findJsxElements(sourceFile, tagName) {
  const found = [];
  const visit = (node) => {
    if (ts.isJsxElement(node) && node.openingElement.tagName.getText(sourceFile) === tagName) {
      found.push(node);
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
  return found;
}

function attributesOf(element) {
  return element.openingElement.attributes.properties.filter(ts.isJsxAttribute);
}

function contrastRatio(first, second) {
  const luminance = (hex) => {
    const cleanHex = hex.slice(1);
    const channels = [0, 2, 4].map((offset) => parseInt(cleanHex.slice(offset, offset + 2), 16) / 255);
    const linear = channels.map((value) => value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4);
    return linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722;
  };
  const values = [luminance(first), luminance(second)].sort((left, right) => right - left);
  return (values[0] + 0.05) / (values[1] + 0.05);
}

test('Phase 2 workspace rail declares eight selectable workspaces', () => {
  const sourceFile = readSource('apps/web/src/app/page.tsx');
  let workspaceDeclaration;
  const visit = (node) => {
    if (ts.isVariableDeclaration(node) && node.name.getText(sourceFile) === 'WORKSPACES') {
      workspaceDeclaration = node;
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);

  assert.ok(workspaceDeclaration && ts.isArrayLiteralExpression(workspaceDeclaration.initializer));
  const workspaceIds = workspaceDeclaration.initializer.elements.map((entry) => {
    assert.ok(ts.isObjectLiteralExpression(entry));
    const id = entry.properties.find((property) =>
      ts.isPropertyAssignment(property) && property.name.getText(sourceFile) === 'id'
    );
    assert.ok(id && ts.isPropertyAssignment(id) && ts.isStringLiteral(id.initializer));
    return id.initializer.text;
  });
  assert.equal(workspaceIds.length, 8);
  assert.deepEqual(workspaceIds, [
    'ws-repos', 'ws-canvas', 'ws-simulator', 'ws-git',
    'ws-trajectory', 'ws-gates', 'ws-ai', 'ws-settings',
  ]);

  const buttons = findJsxElements(sourceFile, 'button');
  const railButton = buttons.find((button) => attributesOf(button).some((attribute) =>
    attribute.name.getText(sourceFile) === 'key' && attribute.initializer?.getText(sourceFile) === '{ws.id}'
  ));
  assert.ok(railButton, 'Workspace navigation must render a button per workspace');
  const onClick = attributesOf(railButton).find((attribute) => attribute.name.getText(sourceFile) === 'onClick');
  assert.ok(onClick?.initializer?.getText(sourceFile).includes('setActiveWorkspace(ws.id)'));
});

test('Startup follows the daemon active repository before stale browser preference', () => {
  const source = fs.readFileSync(path.join(repoRoot, 'apps/web/src/app/page.tsx'), 'utf8');
  const daemonSelection = source.indexOf('else if (status.active_repo_id && userRepos.some');
  const stalePreference = source.indexOf('else if (savedId && userRepos.some');

  assert.ok(daemonSelection >= 0 && stalePreference > daemonSelection);
  assert.ok(!source.includes('refreshAll(saved || undefined)'));
});

test('Phase 2 canvas controls are actionable and accessible', () => {
  const sourceFile = readSource('apps/web/src/components/CodeUniverse3D.tsx');
  const buttons = findJsxElements(sourceFile, 'button');
  assert.equal(buttons.length, 26);

  for (const button of buttons) {
    const attributes = attributesOf(button);
    const names = attributes.map((attribute) => attribute.name.getText(sourceFile));
    assert.ok(names.includes('onClick'), `Canvas button at line ${sourceFile.getLineAndCharacterOfPosition(button.getStart(sourceFile)).line + 1} needs a click handler`);
    assert.ok(names.includes('title') || names.includes('aria-label'), `Canvas button at line ${sourceFile.getLineAndCharacterOfPosition(button.getStart(sourceFile)).line + 1} needs an accessible name`);
    const clickAttribute = attributes.find((attribute) => attribute.name.getText(sourceFile) === 'onClick');
    assert.ok(clickAttribute?.initializer && ts.isJsxExpression(clickAttribute.initializer) && clickAttribute.initializer.expression);
    const handler = clickAttribute.initializer.expression;
    assert.ok(ts.isArrowFunction(handler) || ts.isFunctionExpression(handler) || ts.isIdentifier(handler), 'Canvas click handler must be callable');
    if ((ts.isArrowFunction(handler) || ts.isFunctionExpression(handler)) && ts.isBlock(handler.body)) {
      assert.ok(handler.body.statements.length > 0, 'Canvas click handler must not be empty');
    }
  }

  const componentSource = sourceFile.getText(sourceFile);
  for (const evidence of [
    'simulation.damaged_callers.some',
    'simulation.broken_imports.some',
    'simulation.newly_dead_symbols.some',
    'const damageOverlay = Boolean(simulation)',
    'const selectedNodeId = focusNodeId ?? null',
    'damageOverlayRef.current && nodeState.damaged',
    'damageOverlayRef.current && nodeState.dead',
    'damaged && isSimulationEdgeBroken',
    'edge.source_line',
    'edge.target_line',
    'updateSimulationOverlay(damageOverlayRef.current)',
  ]) assert.ok(componentSource.includes(evidence), `Canvas overlay must handle ${evidence}`);
});

test('Large graph responses stay bounded and dense rendering uses GPU batches', () => {
  const daemon = fs.readFileSync(path.join(repoRoot, 'apps/synevid-ui/src/server.rs'), 'utf8');
  const component = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/CodeUniverse3D.tsx'), 'utf8');
  const renderer = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/denseUniverseRenderer.ts'), 'utf8');
  const api = fs.readFileSync(path.join(repoRoot, 'apps/web/src/lib/api.ts'), 'utf8');

  assert.ok(daemon.includes('MAX_UNIVERSE_NODE_LIMIT: usize = 5_000'));
  assert.ok(daemon.includes('MAX_UNIVERSE_EDGE_LIMIT: usize = 12_000'));
  assert.ok(daemon.includes('total_node_count'));
  assert.ok(daemon.includes('total_edge_count'));
  assert.ok(daemon.includes('is_truncated'));
  assert.ok(daemon.includes('graph_universe_rows_for_snapshot'));
  assert.ok(api.includes('total_node_count?: number'));
  assert.ok(api.includes('is_truncated?: boolean'));
  assert.ok(component.includes('setupDenseUniverseRenderer'));
  assert.ok(component.includes('egoNetwork.incoming_callers.map'));
  assert.ok(component.includes('egoNetwork.outgoing_callees.map'));
  assert.ok(renderer.includes('new THREE.Points('));
  assert.ok(renderer.includes('createFileCardTexture'));
  assert.ok(renderer.includes('buildDenseRenderNodes'));
  assert.ok(renderer.includes('function denseFileCardId'));
  assert.ok(renderer.includes('const fileCardId = denseFileCardId(node)'));
  assert.ok(renderer.includes('const canonicalFilePaths = new Set<string>()'));
  assert.ok(renderer.includes('syntheticFiles.delete(filePath)'));
  assert.ok(renderer.includes('isFileCard: boolean'));
  assert.ok(renderer.includes('DENSE_FILE_SIZE_PX = 68'));
  assert.ok(renderer.includes('HOVER FOR PATH'));
  assert.ok(renderer.includes('buildDenseMindMapPositions'));
  assert.ok(renderer.includes('const clusterColumns = 4'));
  assert.ok(renderer.includes('const fileRowsPerLayer = 10'));
  assert.ok(renderer.includes('const clustersPerShelf = clusterColumns * 3'));
  assert.ok(renderer.includes('(column % 2) * 110 + row * 65'));
  assert.ok(renderer.includes('* 112'));
  assert.ok(renderer.includes('* 88'));
  assert.ok(renderer.includes('* 142'));
  assert.ok(component.includes('universe.node_count >= DENSE_GRAPH_RENDER_THRESHOLD ? 1500'));
  assert.ok(component.includes('const denseTargetZ = universe.node_count >= DENSE_GRAPH_RENDER_THRESHOLD ? 560 : 0'));
  assert.ok(renderer.includes('toLinearRgb(hex, dark ? 0.08 : 0.38)'));
  assert.ok(renderer.includes('new THREE.LineSegments('));
  assert.ok(renderer.includes('MAX_RENDER_PIXEL_RATIO = 2'));
  assert.ok(renderer.includes('canvas.width = 512'));
  assert.ok(renderer.includes('texture.colorSpace = THREE.SRGBColorSpace'));
  assert.ok(renderer.includes('renderer.capabilities.getMaxAnisotropy()'));
  assert.ok(component.includes('antialias: true'));
  assert.ok(component.includes('renderer.outputColorSpace = THREE.SRGBColorSpace'));
  assert.ok(renderer.includes('MAX_DENSE_OVERVIEW_EDGES = 2_000'));
  assert.ok(component.includes('sampleDenseOverviewEdges(universe.edges)'));
  assert.ok(renderer.includes('raycaster.intersectObjects'));
  assert.ok(component.includes('Dense overview renders document cards only'));
  assert.ok(component.includes('Document card: colored by module, hover for path'));
  assert.ok(renderer.includes('pan(-deltaX, -deltaY)'));
  assert.ok(renderer.includes('azimuth += deltaX * 0.005'));
  assert.ok(renderer.includes('elevation - deltaY * 0.005'));
  assert.ok(component.includes('targetTheta += deltaX * 0.005'));
  assert.ok(component.includes('targetPhi - deltaY * 0.005'));
  assert.ok(component.includes('targetAlpha += deltaX * 0.004'));
  assert.ok(component.includes('targetBeta - deltaY * 0.004'));
  assert.ok(component.includes("selectedNodeId.startsWith('file:')"));
  assert.ok(component.includes('fileNodeIds = new Set'));
  assert.ok(component.includes('getUniverseNodeFilePath(node) === filePath'));
  assert.ok(component.includes('edge.target_file === filePath'));
  assert.ok(component.includes('edge.source_file === filePath'));
  assert.ok(renderer.includes('targetRadius = selectedFilePath ? 150'));
  assert.ok(renderer.includes('const focusedFileCard = new THREE.Sprite'));
  assert.ok(renderer.includes('focusedFileCard.scale.set(78, 46, 1)'));
  assert.ok(renderer.includes('const positionFocusedFileCard = (position: THREE.Vector3)'));
  assert.ok(renderer.includes('focusedFileCard.position.x -= Math.cos(azimuth) * 42'));
  assert.ok(renderer.includes('createFocusedFileTexture(node, filePath, activeTheme, accentHex)'));
  assert.ok(renderer.includes('const clusterColors = activeTheme ? options.darkClusterColors : options.lightClusterColors'));
  assert.ok(renderer.includes('context.strokeStyle = accentCss'));
  assert.ok(renderer.includes('depthTest: false'));
  assert.ok(renderer.includes('denseFilePath(selectedNode)'));
  assert.ok(renderer.includes('const selectedFilePath = selectedNode ? denseFilePath(selectedNode) : null'));
  assert.ok(renderer.includes('selectedFilePath !== undefined'));
  assert.ok(renderer.includes('const nodeGroups = [{'));
  assert.ok(renderer.includes("node.kind === 'file' && denseFileCardId(node) === node.id"));
  assert.ok(renderer.includes('const seenFileEdges = new Set<string>()'));
});

test('Canvas graph loading is isolated from the cockpit main thread', () => {
  const page = fs.readFileSync(path.join(repoRoot, 'apps/web/src/app/page.tsx'), 'utf8');
  const worker = fs.readFileSync(path.join(repoRoot, 'apps/web/src/workers/graphUniverse.worker.ts'), 'utf8');

  assert.ok(page.includes("React.lazy(() => import('@/components/CodeUniverse3D'))"));
  assert.ok(page.includes("new Worker(new URL('../workers/graphUniverse.worker.ts', import.meta.url))"));
  assert.ok(page.includes("worker.postMessage({ type: 'cancel'"));
  assert.ok(page.includes("setGraphLoadStage('mounting')"));
  assert.ok(page.includes('startTransition(() =>'));
  assert.ok(page.includes('requestIdleCallback(begin, { timeout: 350 })'));
  assert.ok(page.includes('Canvas loading independently'));
  assert.ok(worker.includes('response.body?.getReader()'));
  assert.ok(worker.includes("postProgress(requestId, 'parsing', 74)"));
  assert.ok(worker.includes('JSON.parse(payload)'));
  assert.ok(worker.includes('normalizeUniverse(raw, dimension)'));
  assert.ok(worker.includes("error.name === 'AbortError'"));
});

test('Large repository explorer virtualizes expanded rows', () => {
  const explorer = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/RepoTreeExplorer.tsx'), 'utf8');

  assert.ok(explorer.includes('const TREE_ROW_HEIGHT = 24'));
  assert.ok(explorer.includes('const TREE_OVERSCAN = 8'));
  assert.ok(explorer.includes('visibleRows.slice(firstVisibleRow, lastVisibleRow)'));
  assert.ok(explorer.includes('windowedRows.map'));
  assert.ok(explorer.includes('role="tree"'));
  assert.ok(explorer.includes('role="treeitem"'));
  assert.ok(!explorer.includes('node.children.map((child) => renderTreeNode(child, depth + 1))'));
});

test('Dense graphs clearly disable the unavailable pulse control', () => {
  const component = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/CodeUniverse3D.tsx'), 'utf8');
  const pulseStart = component.indexOf('{/* Pulse Pause */}');
  const pulseEnd = component.indexOf('</button>', pulseStart);
  const pulseControl = component.slice(pulseStart, pulseEnd);

  assert.ok(component.includes('const dataPulsesAvailable = universe.node_count < DENSE_GRAPH_RENDER_THRESHOLD'));
  assert.ok(pulseControl.includes('disabled={!dataPulsesAvailable}'));
  assert.ok(pulseControl.includes('Data pulses are disabled for dense graphs'));
  assert.ok(pulseControl.includes('Data pulses disabled for dense graph'));
});

test('Dense overview edge sampling is deterministic, evenly spaced, and bounded', async () => {
  const renderer = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/denseUniverseRenderer.ts'), 'utf8');
  const constantStart = renderer.indexOf('export const MAX_DENSE_OVERVIEW_EDGES');
  const functionStart = renderer.indexOf('export function sampleDenseOverviewEdges', constantStart);
  const functionEnd = renderer.indexOf('\n}\n', functionStart) + 2;
  assert.ok(constantStart >= 0 && functionStart > constantStart && functionEnd > functionStart);

  const samplingSource = renderer.slice(constantStart, functionEnd);
  const compiled = ts.transpileModule(`${samplingSource}\nexport { sampleDenseOverviewEdges };`, {
    compilerOptions: { module: ts.ModuleKind.ES2022 },
  }).outputText;
  const { sampleDenseOverviewEdges } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString('base64')}`);
  const edges = Array.from({ length: 12_000 }, (_, index) => `edge-${index}`);
  const sample = sampleDenseOverviewEdges(edges);

  assert.equal(sample.length, 2_000);
  assert.equal(sample[0], 'edge-0');
  assert.equal(sample.at(-1), 'edge-11994');
  assert.equal(new Set(sample).size, sample.length);
  assert.equal(sampleDenseOverviewEdges(edges, edges.length), edges);
  assert.deepEqual(sampleDenseOverviewEdges(edges, 0), []);
});

test('Canvas meter reports presented FPS in Electron and render calls in browser fallback', () => {
  const component = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/CodeUniverse3D.tsx'), 'utf8');
  const renderer = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/denseUniverseRenderer.ts'), 'utf8');
  const meterStart = component.indexOf('data-testid="canvas-fps-meter"');
  const meterEnd = component.indexOf('</div>', meterStart);
  const meter = component.slice(meterStart, meterEnd);
  const regularRender = component.indexOf('renderer.render(scene, camera);', component.indexOf('const animate = () =>'));
  const regularCount = component.indexOf('fpsFrameCountRef.current += 1', regularRender);
  const denseRender = renderer.indexOf('renderer.render(scene, camera);');
  const denseCount = renderer.indexOf('frameCount += 1', denseRender);

  assert.notEqual(meterStart, -1);
  assert.ok(meter.includes('displayedFps.toFixed(1)'));
  assert.ok(meter.includes('Canvas presented FPS'));
  assert.ok(meter.includes('render calls per second'));
  assert.ok(meter.includes('draws/s'));
  assert.ok(meter.includes("hasPresentedFps ? 'FPS' : 'draws/s'"));
  assert.ok(!meter.includes('node_count') && !meter.includes('edge_count'));
  assert.ok(regularRender >= 0 && regularCount > regularRender, 'Regular FPS counts only after render submission');
  assert.ok(denseRender >= 0 && denseCount > denseRender, 'Dense FPS counts only after render submission');
  assert.ok(component.includes('(fpsFrameCountRef.current * 1000) / elapsed'));
  assert.ok(renderer.includes('(frameCount * 1000) / elapsed'));
  assert.ok(component.includes('document.hidden ? 0 : null'));
});

test('Dense theme refresh precomputes palettes and time-slices color buffers', () => {
  const renderer = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/denseUniverseRenderer.ts'), 'utf8');
  const refreshStart = renderer.indexOf('const refreshPointColors = () => {');
  const refreshEnd = renderer.indexOf('const refreshColors = () => {', refreshStart);
  const refreshSource = renderer.slice(refreshStart, refreshEnd);
  const pointWriterStart = renderer.indexOf('const writePointColor = (');
  const edgeWriterStart = renderer.indexOf('const writeEdgeColor = (', pointWriterStart);
  const pointWriterSource = renderer.slice(pointWriterStart, edgeWriterStart);
  const edgeWriterEnd = renderer.indexOf('const refreshPointColors = () => {', edgeWriterStart);
  const edgeWriterSource = renderer.slice(edgeWriterStart, edgeWriterEnd);

  assert.ok(refreshStart >= 0 && refreshEnd > refreshStart);
  assert.ok(pointWriterStart >= 0 && edgeWriterStart > pointWriterStart && edgeWriterEnd > edgeWriterStart);
  assert.ok(renderer.includes('dark: createColorPalette(true, options.darkClusterColors)'));
  assert.ok(renderer.includes('light: createColorPalette(false, options.lightClusterColors)'));
  assert.ok(pointWriterSource.includes('cloud.colors.array as Float32Array'));
  assert.ok(pointWriterSource.includes('const offset = index * 3'));
  assert.ok(edgeWriterSource.includes('cloud.colors.array as Float32Array'));
  assert.ok(edgeWriterSource.includes('colorValues[offset + 5] = blue'));
  assert.doesNotMatch(`${pointWriterSource}\n${edgeWriterSource}\n${refreshSource}`, /new THREE\.Color\(/);
  assert.ok(renderer.includes('const refreshThemeColorsInFrames = () => {'));
  assert.ok(renderer.includes('const deadline = performance.now() + 2'));
  assert.ok(renderer.includes('requestAnimationFrame(() => requestAnimationFrame(updateSlice))'));
});

test('Phase 2 theme swaps are transition-suppressed and status colors meet AAA contrast', () => {
  const css = fs.readFileSync(path.join(repoRoot, 'apps/web/src/app/globals.css'), 'utf8');
  const themeContext = fs.readFileSync(path.join(repoRoot, 'apps/web/src/context/ThemeContext.tsx'), 'utf8');
  const codeUniverse = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/CodeUniverse3D.tsx'), 'utf8');

  for (const selector of [':root, html.light', 'html.dark']) {
    const blockStart = css.indexOf(`${selector} {`);
    assert.notEqual(blockStart, -1, `${selector} color tokens must exist`);
    const blockEnd = css.indexOf('\n}', blockStart);
    const block = css.slice(blockStart, blockEnd);
    const color = (token) => {
      const match = block.match(new RegExp(`${token}: (#[0-9a-f]{6})`, 'i'));
      assert.ok(match, `${token} must have a six-digit color`);
      return match[1];
    };
    const background = color('--bg-canvas');
    assert.ok(contrastRatio(color('--text-primary'), background) >= 7, `${selector} primary text must meet 7:1`);
    for (const token of ['--node-verified', '--node-damaged', '--node-impacted', '--node-deadcode']) {
      assert.ok(contrastRatio(color(token), background) >= 7, `${selector} ${token} must meet 7:1`);
    }
  }

  const switchingAttribute = themeContext.indexOf("root.setAttribute('data-theme-switching', 'true')");
  const themeMutation = themeContext.indexOf("root.classList.remove('dark', 'light')");
  const presentationMark = themeContext.indexOf('desktopBridge.markNextPresentedFrame()');
  assert.ok(themeContext.includes('useSyncExternalStore'));
  assert.ok(themeContext.includes("new CustomEvent('synevid-theme-change', { detail: { mutationMs } })"));
  assert.ok(themeContext.includes("root.setAttribute('data-theme-mutation-ms', mutationMs.toFixed(3))"));
  assert.ok(codeUniverse.includes('data-testid="theme-mutation-latency"'));
  assert.doesNotMatch(codeUniverse, /\buseTheme\(/);
  assert.ok(codeUniverse.includes("window.addEventListener('synevid-theme-change', syncTheme)"));
  assert.ok(codeUniverse.includes("window.addEventListener('storage', syncTheme)"));
  assert.ok(presentationMark >= 0 && presentationMark < switchingAttribute);
  assert.doesNotMatch(themeContext, /void desktopBridge\.markNextPresentedFrame/);
  assert.ok(switchingAttribute >= 0 && switchingAttribute < themeMutation);
  assert.ok(themeContext.includes('window.requestAnimationFrame(() => root.removeAttribute'));
  assert.ok(css.includes('html[data-theme-switching] *'));
  assert.ok(css.includes('transition: none !important'));
});

test('Settings dependency explorer uses persisted graph edges without fixture results', () => {
  const source = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/BlastRadiusSection.tsx'), 'utf8');
  assert.ok(source.includes('api.getEgoNetwork(stableId)'));
  assert.ok(source.includes('network?.incoming_callers'));
  assert.ok(source.includes('network?.outgoing_callees'));
  assert.ok(source.includes('edge.source_file'));
  assert.ok(source.includes('edge.target_file'));
  assert.ok(!/PaymentService|OrderController|CheckoutService|paymentHandler/.test(source));
  assert.ok(source.includes('Direct Callers'));
  assert.ok(source.includes('Direct Callees'));
});

test('Git Galaxy never invents history when the repo or daemon has none', () => {
  const apiSource = fs.readFileSync(path.join(repoRoot, 'apps/web/src/lib/api.ts'), 'utf8');
  const componentSource = fs.readFileSync(path.join(repoRoot, 'apps/web/src/components/GitGalaxySection.tsx'), 'utf8');
  const serverSource = fs.readFileSync(path.join(repoRoot, 'apps/synevid-ui/src/server.rs'), 'utf8');

  assert.doesNotMatch(apiSource, /Initial repo commit|local_git_repo|a0382e3bab199cab4a2cb7912dab85f46f178317/);
  assert.ok(apiSource.includes('commits: [],'));
  assert.ok(apiSource.includes('error: error instanceof Error'));
  assert.ok(componentSource.includes('if (data.error)'));
  assert.ok(componentSource.includes("galaxy?.active_branch || 'Not detected'"));
  assert.ok(serverSource.includes('if !repo_root.ancestors().any(|path| path.join(".git").exists())'));
});
