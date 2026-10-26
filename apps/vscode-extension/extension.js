'use strict';

// SynEvid (Code Autopsy) VS Code / Cursor / Windsurf Extension (zero-build).
//
// Architectural boundary: pure presentation adapter over the deterministic
// `autopsy` and `synevid-lsp` binaries. Never computes graph semantics itself
// and never calls any LLM or network socket.

const cp = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const SUPPORTED_LANGUAGES = [
  'typescript',
  'typescriptreact',
  'javascript',
  'javascriptreact',
  'python',
  'go',
  'rust',
];

const DECLARATION_PATTERNS = [
  /^\s*(?:export\s+)?(?:async\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)/,
  /^\s*(?:export\s+)?(?:abstract\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)/,
  /^\s*(?:export\s+)?interface\s+([A-Za-z_][A-Za-z0-9_]*)/,
  /^\s*def\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(/,
  /^\s*func\s+(?:\([^)]+\)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\(/,
  /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)/,
  /^\s*(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum|trait)\s+([A-Za-z_][A-Za-z0-9_]*)/,
];

function resolveBinary(name, explicitPath, workspaceRoot) {
  if (explicitPath && fs.existsSync(explicitPath)) {
    return explicitPath;
  }
  const envOverride = name === 'autopsy' ? process.env.AUTOPSY_BIN : process.env.SYNEVID_LSP_BIN;
  if (envOverride && fs.existsSync(envOverride)) {
    return envOverride;
  }
  const isWin = process.platform === 'win32';
  const exe = isWin ? `${name}.exe` : name;
  const home = os.homedir();
  const candidates = [
    path.join(home, '.local', 'bin', exe),
    path.join(home, '.cargo', 'bin', exe),
  ];
  if (workspaceRoot) {
    candidates.push(
      path.join(workspaceRoot, 'target', 'release', exe),
      path.join(workspaceRoot, 'target', 'debug', exe)
    );
  }
  for (const cand of candidates) {
    if (fs.existsSync(cand)) {
      return cand;
    }
  }
  return exe;
}

function formatCodeLensLabel(symbol, callers, tests) {
  if (typeof callers === 'number' && typeof tests === 'number') {
    return `$(pulse) ${callers} caller${callers === 1 ? '' : 's'} · ${tests} test${tests === 1 ? '' : 's'}`;
  }
  if (typeof callers === 'number') {
    return `$(pulse) ${callers} caller${callers === 1 ? '' : 's'}`;
  }
  return `$(pulse) SynEvid: Blast Radius (${symbol})`;
}

function parseBlastRadiusMetrics(reportJson) {
  if (!reportJson || typeof reportJson !== 'object') {
    return { callers: 0, tests: 0 };
  }
  const imp = reportJson.impact || {};
  const entities = Array.isArray(imp.impacted_entities) ? imp.impacted_entities : [];
  const callers = imp.total_impacted ?? entities.length;
  let tests = 0;
  for (const entity of entities) {
    const symId = String(entity.symbol_id || entity.target_symbol || entity.symbol || '').toLowerCase();
    const kind = String(entity.kind || '').toLowerCase();
    if (kind === 'test' || symId.includes('test') || symId.includes('spec') || symId.includes('__test__')) {
      tests += 1;
    }
  }
  return { callers, tests };
}

function extractDeclarationSymbols(text) {
  const lines = String(text || '').split(/\r?\n/);
  const hits = [];
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    for (const regex of DECLARATION_PATTERNS) {
      const m = regex.exec(line);
      if (m && m[1]) {
        hits.push({ line: i, symbol: m[1] });
        break;
      }
    }
  }
  return hits;
}

function loadLatestForensicReport(workspaceRoot) {
  if (!workspaceRoot) return null;
  const latestPath = path.join(workspaceRoot, '.autopsy', 'runs', 'latest.json');
  if (!fs.existsSync(latestPath)) return null;
  try {
    return JSON.parse(fs.readFileSync(latestPath, 'utf8'));
  } catch {
    return null;
  }
}

function formatTrajectoryItems(report) {
  if (!report) {
    return [
      {
        label: 'No forensic trajectory recorded yet',
        description: 'Run SynEvid: Verify or complete an agent session',
      },
    ];
  }
  const items = [
    {
      label: `Verdict: ${String(report.verdict || 'unknown').toUpperCase()}`,
      description: `Session ${String(report.session_id || '').slice(0, 12)} · step: ${report.evidence_step || 'none'}`,
    },
    {
      label: `Digest: ${String(report.report_digest || '').slice(0, 16)}`,
      description: `Steps: ${report.total_steps || 0} (${report.failed_steps || 0} failed)`,
    },
    {
      label: 'Next Agent Guidance',
      description: String(report.next_agent_guidance || ''),
    },
  ];
  for (const step of report.steps || []) {
    items.push({
      label: `Step ${step.index}: autopsy ${(step.invoked || []).join(' ')}`,
      description: `exit ${step.exit_code}${step.note ? ' · ' + step.note : ''}`,
    });
  }
  return items;
}

function runAutopsyCommand(bin, repoRoot, args) {
  return new Promise((resolve) => {
    cp.execFile(
      bin,
      ['--repo-root', repoRoot, ...args],
      { cwd: repoRoot, maxBuffer: 20 * 1024 * 1024, timeout: 30000 },
      (err, stdout, stderr) => {
        const exitCode = err && typeof err.code === 'number' ? err.code : err ? 3 : 0;
        resolve({ exitCode, stdout: String(stdout || ''), stderr: String(stderr || '') });
      }
    );
  });
}

function activate(context) {
  // Lazy-require `vscode` so unit tests can import helper functions outside the VS Code host.
  const vscode = require('vscode');
  const output = vscode.window.createOutputChannel('SynEvid Forensics');
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBar.text = '$(shield) SynEvid: Ready';
  statusBar.tooltip = 'Click to run SynEvid architectural & contract verification';
  statusBar.command = 'synevid.verify';
  statusBar.show();
  context.subscriptions.push(output, statusBar);

  const getRoot = () => {
    const folders = vscode.workspace.workspaceFolders;
    return folders && folders.length > 0 ? folders[0].uri.fsPath : process.cwd();
  };

  const getAutopsyBin = () => {
    const cfg = vscode.workspace.getConfiguration('synevid');
    return resolveBinary('autopsy', cfg.get('autopsyPath') || '', getRoot());
  };

  // TreeDataProvider for `.autopsy/runs/latest.json`
  const emitter = new vscode.EventEmitter();
  const treeProvider = {
    onDidChangeTreeData: emitter.event,
    getTreeItem(element) {
      const item = new vscode.TreeItem(element.label, vscode.TreeItemCollapsibleState.None);
      item.description = element.description;
      item.tooltip = `${element.label}\n${element.description || ''}`;
      return item;
    },
    getChildren() {
      const report = loadLatestForensicReport(getRoot());
      return formatTrajectoryItems(report);
    },
  };
  context.subscriptions.push(
    vscode.window.registerTreeDataProvider('synevidTrajectoryView', treeProvider)
  );

  // CodeLens Provider with cached caller/test metrics
  const codeLensCache = new Map();
  const codeLensProvider = {
    provideCodeLenses(document) {
      const cfg = vscode.workspace.getConfiguration('synevid');
      if (!cfg.get('enableCodeLens', true)) return [];
      const hits = extractDeclarationSymbols(document.getText());
      const root = getRoot();
      const rel = path.relative(root, document.uri.fsPath).split(path.sep).join('/');
      return hits.map((h) => {
        const range = new vscode.Range(h.line, 0, h.line, 0);
        const cacheKey = `${rel}::${h.symbol}`;
        const cached = codeLensCache.get(cacheKey);
        const title = cached
          ? formatCodeLensLabel(h.symbol, cached.callers, cached.tests)
          : formatCodeLensLabel(h.symbol);
        const lens = new vscode.CodeLens(range, {
          title,
          command: 'synevid.impactAtCursor',
          arguments: [h.symbol],
        });
        lens.symbol = h.symbol;
        lens.seed = cacheKey;
        return lens;
      });
    },
    async resolveCodeLens(codeLens) {
      if (!codeLens.symbol || !codeLens.seed) return codeLens;
      const root = getRoot();
      try {
        const res = await runAutopsyCommand(getAutopsyBin(), root, [
          '--format',
          'json',
          'impact',
          '-s',
          codeLens.seed,
          '--profile',
          'callers',
          '--detail',
          'summary',
        ]);
        if (res.exitCode === 0 || res.exitCode === 4) {
          const data = JSON.parse(res.stdout);
          const metrics = parseBlastRadiusMetrics(data);
          codeLensCache.set(codeLens.seed, metrics);
          if (codeLens.command) {
            codeLens.command.title = formatCodeLensLabel(codeLens.symbol, metrics.callers, metrics.tests);
          }
        }
      } catch {
        // Fallback silently if engine query cannot finish in time
      }
      return codeLens;
    },
  };
  for (const lang of SUPPORTED_LANGUAGES) {
    context.subscriptions.push(
      vscode.languages.registerCodeLensProvider({ language: lang }, codeLensProvider)
    );
  }

  // Hover Provider backed by `autopsy find` + `autopsy impact` or `synevid-lsp`
  const hoverProvider = {
    async provideHover(document, position) {
      const wordRange = document.getWordRangeAtPosition(position);
      if (!wordRange) return null;
      const word = document.getText(wordRange);
      if (!word || word.length < 2) return null;
      const root = getRoot();
      const rel = path.relative(root, document.uri.fsPath).split(path.sep).join('/');
      const seed = `${rel}::${word}`;
      const res = await runAutopsyCommand(getAutopsyBin(), root, [
        '--format',
        'json',
        'impact',
        '-s',
        seed,
        '--profile',
        'callers',
        '--top-k',
        '5',
      ]);
      if (res.exitCode !== 0 && res.exitCode !== 4) return null;
      try {
        const data = JSON.parse(res.stdout);
        const imp = data.impact || {};
        const total = imp.total_impacted ?? (imp.impacted_entities || []).length;
        const cov = (data.coverage && data.coverage.state) || 'verified';
        const md = new vscode.MarkdownString(
          `**SynEvid Blast Radius** — \`${seed}\`\n\n` +
            `- **Impacted callers:** \`${total}\` (coverage: \`${cov}\`)\n` +
            `- **Digest:** \`${String(data.result_digest || '').slice(0, 16)}\``
        );
        return new vscode.Hover(md, wordRange);
      } catch {
        return null;
      }
    },
  };
  for (const lang of SUPPORTED_LANGUAGES) {
    context.subscriptions.push(
      vscode.languages.registerHoverProvider({ language: lang }, hoverProvider)
    );
  }

  // Commands
  context.subscriptions.push(
    vscode.commands.registerCommand('synevid.init', async () => {
      const root = getRoot();
      const res = await runAutopsyCommand(getAutopsyBin(), root, ['init']);
      output.appendLine(res.stdout || res.stderr);
      output.show(true);
      emitter.fire();
    }),
    vscode.commands.registerCommand('synevid.verify', async () => {
      const root = getRoot();
      statusBar.text = '$(sync~spin) SynEvid: Verifying...';
      const res = await runAutopsyCommand(getAutopsyBin(), root, ['verify']);
      output.appendLine(res.stdout || res.stderr);
      output.show(true);
      statusBar.text =
        res.exitCode === 0
          ? '$(shield) SynEvid: PASS'
          : `$(warning) SynEvid: FAIL (exit ${res.exitCode})`;
      emitter.fire();
    }),
    vscode.commands.registerCommand('synevid.impactAtCursor', async (explicitSymbol) => {
      const root = getRoot();
      let symbol = explicitSymbol;
      if (!symbol) {
        const editor = vscode.window.activeTextEditor;
        if (editor) {
          const range = editor.document.getWordRangeAtPosition(editor.selection.active);
          if (range) symbol = editor.document.getText(range);
        }
      }
      if (!symbol) return;
      const findRes = await runAutopsyCommand(getAutopsyBin(), root, [
        '--format',
        'json',
        'find',
        symbol,
        '--limit',
        '1',
      ]);
      let seed = symbol;
      try {
        const parsed = JSON.parse(findRes.stdout);
        if (parsed.symbols && parsed.symbols[0] && parsed.symbols[0].stable_id) {
          seed = parsed.symbols[0].stable_id;
        }
      } catch {
        // fallback to raw symbol
      }
      const res = await runAutopsyCommand(getAutopsyBin(), root, [
        'impact',
        '-s',
        seed,
        '--profile',
        'callers',
        '--top-k',
        '10',
      ]);
      output.appendLine(res.stdout || res.stderr);
      output.show(true);
    }),
    vscode.commands.registerCommand('synevid.showForensicReport', async () => {
      const root = getRoot();
      const mdPath = path.join(root, '.autopsy', 'runs', 'latest.md');
      if (!fs.existsSync(mdPath)) {
        await runAutopsyCommand(getAutopsyBin(), root, ['session', 'report']);
      }
      if (fs.existsSync(mdPath)) {
        const doc = await vscode.workspace.openTextDocument(mdPath);
        await vscode.window.showTextDocument(doc, { preview: false });
      }
      emitter.fire();
    }),
    vscode.commands.registerCommand('synevid.doctor', async () => {
      const root = getRoot();
      const res = await runAutopsyCommand(getAutopsyBin(), root, ['doctor']);
      output.appendLine(res.stdout || res.stderr);
      output.show(true);
    }),
    vscode.commands.registerCommand('synevid.refreshTrajectory', () => {
      emitter.fire();
    })
  );
}

function deactivate() {}

module.exports = {
  activate,
  deactivate,
  resolveBinary,
  extractDeclarationSymbols,
  loadLatestForensicReport,
  formatTrajectoryItems,
  formatCodeLensLabel,
  parseBlastRadiusMetrics,
  SUPPORTED_LANGUAGES,
};
