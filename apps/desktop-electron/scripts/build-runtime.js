// Build the production resources consumed by electron-builder.
// The packaged app must contain both the Rust daemon and the Next standalone
// server; source development remains handled by scripts/dev.js.
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const { writeRuntimeManifest } = require('../src/runtime-manifest');

const desktopDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(desktopDir, '../..');
const webDir = path.join(repoRoot, 'apps', 'web');
const runtimeDir = path.join(desktopDir, 'build', 'runtime');
const cockpitDir = path.join(runtimeDir, 'cockpit');
const depsDir = path.join(runtimeDir, 'deps');
const sidecarsDir = path.join(runtimeDir, 'sidecars');
const mcpDir = path.join(runtimeDir, 'mcp-server');
const schemasDir = path.join(runtimeDir, 'schemas');
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

const [nodeMajor, nodeMinor] = process.versions.node.split('.').map(Number);
if (nodeMajor < 22 || (nodeMajor === 22 && nodeMinor < 12)) {
  fail(`Node.js 22.12 or newer is required; found ${process.versions.node}`);
}

const cargo = process.platform === 'win32' ? 'cargo.exe' : 'cargo';
const npm = process.platform === 'win32' ? process.execPath : 'npm';
const npmPrefix = process.platform === 'win32'
  ? [process.env.npm_execpath || path.join(path.dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js')]
  : [];

console.log('[SynEvid Package] Building the Rust daemon from source...');
run(cargo, ['build', '--release', '-p', 'synevid-ui', '-p', 'autopsy-cli'], repoRoot);

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
fs.mkdirSync(schemasDir, { recursive: true });
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
const cliName = process.platform === 'win32' ? 'autopsy.exe' : 'autopsy';
const daemonPath = path.join(repoRoot, 'target', 'release', daemonName);
if (!fs.existsSync(daemonPath)) fail(`Release daemon was not produced: ${daemonPath}`);
const packagedDaemonPath = path.join(sidecarsDir, daemonName);
fs.copyFileSync(daemonPath, packagedDaemonPath);
// copyFileSync does not preserve the executable mode on POSIX hosts. Without
// this bit, the packaged Linux/macOS shell can launch but cannot start the
// Rust daemon sidecar, so the health-gated smoke test correctly fails.
if (process.platform !== 'win32') fs.chmodSync(packagedDaemonPath, 0o755);
const cliPath = path.join(repoRoot, 'target', 'release', cliName);
if (!fs.existsSync(cliPath)) fail(`Release CLI was not produced: ${cliPath}`);
const packagedCliPath = path.join(sidecarsDir, cliName);
fs.copyFileSync(cliPath, packagedCliPath);
if (process.platform !== 'win32') fs.chmodSync(packagedCliPath, 0o755);
const mcpBundle = path.join(mcpDirSource, 'dist', 'index.bundle.js');
if (!fs.existsSync(mcpBundle)) fail(`MCP bundle was not produced: ${mcpBundle}`);
fs.copyFileSync(mcpBundle, path.join(mcpDir, 'index.bundle.js'));
const schemaSource = path.join(repoRoot, 'schemas');
if (!fs.existsSync(schemaSource)) fail(`Public schemas were not found: ${schemaSource}`);
fs.cpSync(schemaSource, schemasDir, { recursive: true });
const mcpReadme = path.join(mcpDirSource, 'README.md');
if (fs.existsSync(mcpReadme)) fs.copyFileSync(mcpReadme, path.join(mcpDir, 'README.md'));
writeRuntimeManifest(runtimeDir, {
  version: require('../package.json').version,
  platform: process.platform,
  arch: process.arch,
});

console.log(`[SynEvid Package] Runtime assembled at ${runtimeDir}`);
