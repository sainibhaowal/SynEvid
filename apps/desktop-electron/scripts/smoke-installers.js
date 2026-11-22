'use strict';

// Installs or extracts the actual native release artifact, then delegates to
// the authenticated repository/index/report smoke used by the packaged app.
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const desktopDir = path.resolve(__dirname, '..');
const distDir = path.join(desktopDir, 'dist');
const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-installer-smoke-'));
const smokeScript = path.join(__dirname, 'smoke-packaged.js');

function fail(message) {
  throw new Error(message);
}

function findArtifact(pattern, label) {
  const match = fs.readdirSync(distDir).find((file) => pattern.test(file));
  if (!match) fail(`Missing ${label} in ${distDir}`);
  return path.join(distDir, match);
}

function run(command, args, { capture = false, ...options } = {}) {
  const result = spawnSync(command, args, {
    cwd: desktopDir,
    encoding: 'utf8',
    stdio: capture ? ['ignore', 'pipe', 'inherit'] : 'inherit',
    windowsHide: true,
    timeout: 180_000,
    ...options,
  });
  if (result.error) fail(`${command} failed: ${result.error.message}`);
  if (result.status !== 0) fail(`${command} exited with status ${result.status ?? result.signal}`);
  return result.stdout || '';
}

function waitForPathRemoval(targetPath, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  const delay = new Int32Array(new SharedArrayBuffer(4));
  while (fs.existsSync(targetPath) && Date.now() < deadline) {
    Atomics.wait(delay, 0, 0, 250);
  }
  return !fs.existsSync(targetPath);
}

function smokeExecutable(executable) {
  if (!fs.existsSync(executable)) fail(`Installed executable is missing: ${executable}`);
  // Windows portable builds self-extract the complete runtime on first launch.
  run(process.execPath, [smokeScript, executable], { timeout: 240_000 });
}

function findUninstaller(root) {
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const candidate = path.join(root, entry.name);
    if (entry.isFile() && /^uninstall.*\.exe$/i.test(entry.name)) return candidate;
    if (entry.isDirectory()) {
      const nested = findUninstaller(candidate);
      if (nested) return nested;
    }
  }
  return null;
}

function smokeWindows() {
  const installDir = path.join(tempRoot, 'windows-installed');
  const nsis = findArtifact(/-setup\.exe$/i, 'Windows NSIS setup');
  run(nsis, ['/S', `/D=${installDir}`]);
  const installedExe = path.join(installDir, 'SynEvid.exe');
  smokeExecutable(installedExe);

  const uninstaller = findUninstaller(installDir);
  if (!uninstaller) fail('NSIS install did not produce an uninstaller');
  run(uninstaller, ['/S']);
  if (!waitForPathRemoval(installedExe)) {
    fail('NSIS uninstall did not remove the installed executable within 30 seconds');
  }

  const portable = findArtifact(/-portable\.exe$/i, 'Windows portable executable');
  smokeExecutable(portable);
}

function smokeMac() {
  const mountDir = path.join(tempRoot, 'dmg-mount');
  const installDir = path.join(tempRoot, 'mac-installed');
  fs.mkdirSync(mountDir);
  fs.mkdirSync(installDir);
  const dmg = findArtifact(/\.dmg$/i, 'macOS DMG');
  let mounted = false;
  try {
    run('hdiutil', ['attach', '-nobrowse', '-readonly', '-mountpoint', mountDir, dmg]);
    mounted = true;
    const appName = fs.readdirSync(mountDir).find((name) => name.endsWith('.app'));
    if (!appName) fail('DMG did not contain a macOS application bundle');
    const installedApp = path.join(installDir, appName);
    run('ditto', ['--rsrc', '--extattr', '--acl', path.join(mountDir, appName), installedApp]);
    smokeExecutable(path.join(installedApp, 'Contents', 'MacOS', 'SynEvid'));
  } finally {
    if (mounted) run('hdiutil', ['detach', mountDir]);
  }

  const zip = findArtifact(/\.zip$/i, 'macOS ZIP archive');
  const zipDir = path.join(tempRoot, 'mac-zip');
  fs.mkdirSync(zipDir);
  run('ditto', ['-x', '-k', zip, zipDir]);
  const zippedApp = path.join(zipDir, 'SynEvid.app', 'Contents', 'MacOS', 'SynEvid');
  smokeExecutable(zippedApp);
}

function smokeLinux() {
  const deb = findArtifact(/\.deb$/i, 'Linux DEB package');
  const packageName = run('dpkg-deb', ['-f', deb, 'Package'], { capture: true }).trim();
  if (!packageName) fail('Could not read Debian package name');
  // Let the package manager resolve the native runtime dependencies exactly
  // as a user installation would, rather than invoking dpkg without them.
  run('sudo', ['apt-get', 'install', '--yes', '--no-install-recommends', deb]);
  try {
    const installedFiles = run('dpkg-query', ['-L', packageName], { capture: true });
    const executable = installedFiles.split(/\r?\n/).find((item) => /\/synevid$/.test(item) && fs.existsSync(item));
    if (!executable) fail(`Installed Debian package ${packageName} has no synevid executable`);
    smokeExecutable(executable);
  } finally {
    run('sudo', ['apt-get', 'remove', '--yes', packageName]);
  }

  const appImage = findArtifact(/\.AppImage$/i, 'Linux AppImage');
  fs.chmodSync(appImage, 0o755);
  smokeExecutable(appImage);
}

try {
  if (!fs.existsSync(distDir)) fail(`Missing Electron artifacts directory: ${distDir}`);
  if (process.platform === 'win32') smokeWindows();
  else if (process.platform === 'darwin') smokeMac();
  else if (process.platform === 'linux') smokeLinux();
  else fail(`Unsupported smoke host platform: ${process.platform}`);
  console.log(`ELECTRON_INSTALLER_SMOKE_PASS ${process.platform}`);
} catch (error) {
  console.error(`ELECTRON_INSTALLER_SMOKE_FAIL ${error.message}`);
  process.exitCode = 1;
} finally {
  try {
    fs.rmSync(tempRoot, { recursive: true, force: true, maxRetries: 20, retryDelay: 250 });
  } catch (error) {
    console.warn(`ELECTRON_INSTALLER_SMOKE_CLEANUP_WARNING ${error.message}`);
  }
}
