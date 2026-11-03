// Build the production resources consumed by electron-builder.
// The packaged app must contain both the Rust daemon and the Next standalone
// server; source development remains handled by scripts/dev.js.
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const desktopDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopDir, '../..');
const webDir = path.join(repoRoot, 'apps', 'web');
const runtimeDir = path.join(desktopDir, 'build', 'runtime');
const cockpitDir = path.join(runtimeDir, 'cockpit');
const depsDir = path.join(runtimeDir, 'deps');
const sidecarsDir = path.join(runtimeDir, 'sidecars');
const mcpDir = path.join(runtimeDir, 'mcp-server');
const mcpDirSource = path.join(repoRoot, 'apps', 'mcp-server');

function fail(message) {
  console.error(`[SynEvid Package] ${message}`);
  process.exit(1);
}

function run(command, args, cwd, env = {}) {
  const result = spawnSync(command, args, {
    cwd,
    stdio: 'inherit',
    env: { ...process.env, ...env },
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    fail(`Command failed: ${command} ${args.join(' ')}${result.error ? ` (${result.error.message})` : ''}`);
  }
}

function findServer(root) {
  const candidate = path.join(root, 'server.js');
  if (fs.existsSync(candidate)) return candidate;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const nested = findServer(path.join(root, entry.name));
    if (nested) return nested;
  }
  return null;
}

if (Number(process.versions.node.split('.')[0]) < 20) {
  fail(`Node.js 20 or newer is required; found ${process.versions.node}`);
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
const npm = process.platform === 'win32' ? process.execPath : 'npm';
const npmPrefix = process.platform === 'win32'
  ? [process.env.npm_execpath || path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js')]
  : [];

console.log('[SynEvid Package] Building the Rust daemon from source...');
run(cargo, ['build', '--release', '-p', 'synevid-ui'], repoRoot);

console.log('[SynEvid Package] Building the Next.js standalone cockpit...');
run(npm, [...npmPrefix, 'run', 'build'], webDir, { NEXT_TELEMETRY_DISABLED: '1' });

console.log('[SynEvid Package] Bundling the local MCP bridge...');
run(npm, [...npmPrefix, 'run', 'build:bundle'], mcpDirSource);

const standaloneDir = path.join(webDir, '.next', 'standalone');
const staticDir = path.join(webDir, '.next', 'static');
const publicDir = path.join(webDir, 'public');
const server = fs.existsSync(standaloneDir) ? findServer(standaloneDir) : null;
if (!server) fail(`Next standalone server.js was not produced under ${standaloneDir}`);

fs.rmSync(runtimeDir, { recursive: true, force: true });
fs.mkdirSync(cockpitDir, { recursive: true });
fs.mkdirSync(depsDir, { recursive: true });
fs.mkdirSync(sidecarsDir, { recursive: true });
fs.mkdirSync(mcpDir, { recursive: true });
// Preserve the standalone dependency tree; server.js alone is not runnable.
fs.cpSync(standaloneDir, cockpitDir, { recursive: true });
// electron-builder intentionally omits arbitrary node_modules directories from
// extraResources. Keep the standalone dependency tree in a neutral resource
// directory and expose it through NODE_PATH at packaged startup.
const cockpitNodeModules = path.join(cockpitDir, 'node_modules');
if (!fs.existsSync(cockpitNodeModules)) fail(`Next standalone dependencies were not produced under ${cockpitNodeModules}`);
fs.cpSync(cockpitNodeModules, depsDir, { recursive: true });
fs.rmSync(cockpitNodeModules, { recursive: true, force: true });
if (!fs.existsSync(path.join(cockpitDir, 'server.js'))) {
  fs.copyFileSync(server, path.join(cockpitDir, 'server.js'));
}
if (fs.existsSync(staticDir)) fs.cpSync(staticDir, path.join(cockpitDir, '.next', 'static'), { recursive: true });
if (fs.existsSync(publicDir)) fs.cpSync(publicDir, path.join(cockpitDir, 'public'), { recursive: true });

const daemonName = process.platform === 'win32' ? 'synevid-ui.exe' : 'synevid-ui';
const daemonPath = path.join(repoRoot, 'target', 'release', daemonName);
if (!fs.existsSync(daemonPath)) fail(`Release daemon was not produced: ${daemonPath}`);
fs.copyFileSync(daemonPath, path.join(sidecarsDir, daemonName));
const mcpBundle = path.join(mcpDirSource, 'dist', 'index.bundle.js');
if (!fs.existsSync(mcpBundle)) fail(`MCP bundle was not produced: ${mcpBundle}`);
fs.copyFileSync(mcpBundle, path.join(mcpDir, 'index.bundle.js'));

console.log(`[SynEvid Package] Runtime assembled at ${runtimeDir}`);
