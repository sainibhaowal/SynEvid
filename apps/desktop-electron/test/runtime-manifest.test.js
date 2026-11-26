'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { verifyRuntimeManifest, writeRuntimeManifest } = require('../src/runtime-manifest');

function createRuntimeFixture(root) {
  const files = [
    'cockpit/server.js',
    'sidecars/synevid-ui.exe',
    'sidecars/autopsy.exe',
    'mcp-server/index.bundle.js',
    'schemas/evidence-envelope.schema.json',
  ];
  for (const file of files) {
    const fullPath = path.join(root, file);
    fs.mkdirSync(path.dirname(fullPath), { recursive: true });
    fs.writeFileSync(fullPath, `fixture:${file}\n`);
  }
  fs.mkdirSync(path.join(root, 'deps'), { recursive: true });
}

test('runtime manifest records deterministic file hashes and verifies resource bytes', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-runtime-manifest-'));
  try {
    createRuntimeFixture(root);
    const manifest = writeRuntimeManifest(root, { version: '0.6.0', platform: 'win32', arch: 'x64' });
    assert.deepEqual(manifest.files.map((file) => file.path), [
      'cockpit/server.js',
      'mcp-server/index.bundle.js',
      'schemas/evidence-envelope.schema.json',
      'sidecars/autopsy.exe',
      'sidecars/synevid-ui.exe',
    ]);
    assert.equal(verifyRuntimeManifest(root).files_verified, 5);

    fs.appendFileSync(path.join(root, 'cockpit/server.js'), 'changed');
    assert.throws(() => verifyRuntimeManifest(root), /integrity check failed/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

test('runtime manifest rejects path traversal and a missing required resource', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-runtime-manifest-invalid-'));
  try {
    createRuntimeFixture(root);
    const manifest = writeRuntimeManifest(root, { version: '0.6.0', platform: 'win32', arch: 'x64' });
    const traversal = { ...manifest, files: [{ path: '../escape', sha256: '0'.repeat(64), size_bytes: 0 }, ...manifest.files] };
    assert.throws(() => verifyRuntimeManifest(root, traversal), /Non-canonical runtime resource path/);
    fs.unlinkSync(path.join(root, 'schemas/evidence-envelope.schema.json'));
    assert.throws(() => writeRuntimeManifest(root, { version: '0.6.0', platform: 'win32', arch: 'x64' }), /Missing required runtime resources/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});
