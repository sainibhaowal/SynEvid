'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

function compareText(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function sha256(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

function listFiles(rootDir, relativeDir = '', activeDirectories = new Set()) {
  const dir = path.resolve(rootDir, relativeDir);
  const realDir = fs.realpathSync(dir);
  const rootReal = fs.realpathSync(rootDir);
  const relativeReal = path.relative(rootReal, realDir);
  if (relativeReal === '..' || relativeReal.startsWith(`..${path.sep}`) || path.isAbsolute(relativeReal)) {
    throw new Error(`Runtime resource escaped its root: ${relativeDir || '.'}`);
  }
  if (activeDirectories.has(realDir)) throw new Error(`Runtime resource symlink cycle: ${relativeDir || '.'}`);

  const nextActive = new Set(activeDirectories);
  nextActive.add(realDir);
  const entries = fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => compareText(a.name, b.name));
  const files = [];
  for (const entry of entries) {
    const rel = relativeDir ? `${relativeDir}/${entry.name}` : entry.name;
    const abs = path.join(dir, entry.name);
    const stat = fs.statSync(abs);
    if (stat.isDirectory()) {
      files.push(...listFiles(rootDir, rel, nextActive));
    } else if (stat.isFile()) {
      files.push({ path: rel.replaceAll('\\', '/'), sha256: sha256(abs), size_bytes: stat.size });
    } else {
      throw new Error(`Unsupported runtime resource type: ${rel}`);
    }
  }
  return files;
}

function writeRuntimeManifest(runtimeDir, { version, platform, arch }) {
  const roots = ['cockpit', 'deps', 'sidecars', 'mcp-server', 'schemas'];
  for (const dir of roots) {
    if (!fs.statSync(path.join(runtimeDir, dir), { throwIfNoEntry: false })?.isDirectory()) {
      throw new Error(`Missing runtime component directory: ${dir}`);
    }
  }
  const files = roots.flatMap((dir) => listFiles(runtimeDir, dir)).sort((a, b) => compareText(a.path, b.path));
  const required = [
    'cockpit/server.js',
    `sidecars/${platform === 'win32' ? 'autopsy-daemon.exe' : 'autopsy-daemon'}`,
    `sidecars/${platform === 'win32' ? 'autopsy.exe' : 'autopsy'}`,
    'mcp-server/index.bundle.js',
    'schemas/evidence-envelope.schema.json',
  ];
  const available = new Set(files.map((file) => file.path));
  const missing = required.filter((file) => !available.has(file));
  if (missing.length) throw new Error(`Missing required runtime resources: ${missing.join(', ')}`);

  const manifest = {
    schema_version: 1,
    application_version: version,
    platform,
    architecture: arch,
    sqlite_migrations: 'embedded-in-autopsy-daemon-sidecar',
    files,
  };
  fs.writeFileSync(path.join(runtimeDir, 'runtime-manifest.json'), `${JSON.stringify(manifest, null, 2)}\n`, 'utf8');
  return manifest;
}

function verifyRuntimeManifest(resourcesDir, suppliedManifest) {
  const manifestPath = path.join(resourcesDir, 'runtime-manifest.json');
  const manifest = suppliedManifest || JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (manifest.schema_version !== 1 || !Array.isArray(manifest.files) ||
      !['win32', 'darwin', 'linux'].includes(manifest.platform) ||
      typeof manifest.application_version !== 'string' ||
      typeof manifest.architecture !== 'string') {
    throw new Error('Unsupported or malformed runtime resource manifest');
  }
  const required = [
    'cockpit/server.js',
    `sidecars/${manifest.platform === 'win32' ? 'autopsy-daemon.exe' : 'autopsy-daemon'}`,
    `sidecars/${manifest.platform === 'win32' ? 'autopsy.exe' : 'autopsy'}`,
    'mcp-server/index.bundle.js',
    'schemas/evidence-envelope.schema.json',
  ];
  const manifestPaths = new Set(manifest.files.map((file) => file?.path));
  const missingRequired = required.filter((file) => !manifestPaths.has(file));
  if (missingRequired.length) throw new Error(`Manifest omits required runtime resources: ${missingRequired.join(', ')}`);
  const rootReal = fs.realpathSync(resourcesDir);
  let previousPath = '';
  for (const record of manifest.files) {
    if (!record || typeof record.path !== 'string' || !/^[a-f0-9]{64}$/.test(record.sha256) || !Number.isSafeInteger(record.size_bytes)) {
      throw new Error('Malformed runtime resource manifest entry');
    }
    if (record.path <= previousPath || record.path.includes('\\') || record.path.startsWith('/') || record.path.split('/').some((part) => part === '' || part === '.' || part === '..')) {
      throw new Error(`Non-canonical runtime resource path: ${record.path}`);
    }
    previousPath = record.path;
    const absolute = path.resolve(resourcesDir, ...record.path.split('/'));
    const relative = path.relative(resourcesDir, absolute);
    if (relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
      throw new Error(`Runtime resource escaped its root: ${record.path}`);
    }
    const realFile = fs.realpathSync(absolute);
    const realRelative = path.relative(rootReal, realFile);
    if (realRelative === '..' || realRelative.startsWith(`..${path.sep}`) || path.isAbsolute(realRelative)) {
      throw new Error(`Runtime resource resolved outside its root: ${record.path}`);
    }
    const stat = fs.statSync(realFile);
    if (!stat.isFile() || stat.size !== record.size_bytes || sha256(realFile) !== record.sha256) {
      throw new Error(`Runtime resource integrity check failed: ${record.path}`);
    }
  }
  return { files_verified: manifest.files.length, application_version: manifest.application_version };
}

module.exports = { listFiles, verifyRuntimeManifest, writeRuntimeManifest };
