const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { verifyArtifacts } = require('../scripts/verify-artifacts');

const ROOT = path.resolve(__dirname, '..');

test('Phase 5 release matrix declares complete standalone runtime resources', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
  assert.equal(pkg.version, '0.6.0');
  assert.equal(pkg.desktopName, 'SynEvid');
  assert.equal(pkg.homepage, 'https://github.com/sainibhaowal/SynEvid');
  assert.equal(pkg.author.email, 'rav.singh039@gmail.com');
  assert.match(pkg.scripts['dist:win'], /nsis portable msi/);
  assert.match(pkg.scripts['dist:mac'], /dmg zip/);
  assert.match(pkg.scripts['dist:linux'], /AppImage deb rpm/);
  const resourceTargets = pkg.build.extraResources.map((entry) => entry.to);
  assert.deepEqual(resourceTargets, ['cockpit', 'deps', 'sidecars', 'mcp-server', 'schemas', 'runtime-manifest.json']);
  assert.deepEqual(pkg.build.win.target, ['nsis', 'portable', 'msi']);
  assert.equal(pkg.build.nsis.artifactName, 'SynEvid-${version}-${os}-${arch}-setup.${ext}');
  assert.equal(pkg.build.portable.artifactName, 'SynEvid-${version}-${os}-${arch}-portable.${ext}');
  assert.equal(pkg.build.msi.artifactName, 'SynEvid-${version}-${os}-${arch}.${ext}');
  assert.equal(pkg.build.linux.syncDesktopName, true);
  assert.ok(pkg.build.files.includes('!src/.autopsy{,/**/*}'));
});

test('Phase 5 runtime assembly is source-backed and refuses incomplete artifacts', () => {
  const script = fs.readFileSync(path.join(ROOT, 'scripts', 'build-runtime.js'), 'utf8');
  assert.match(script, /cargo.*build.*--release/s);
  assert.match(script, /standalone/);
  assert.match(script, /build:bundle/);
  assert.match(script, /synevid-ui/);
  assert.match(script, /'autopsy-cli'/);
  assert.match(script, /autopsy\.exe/);
  assert.match(script, /MCP bundle was not produced/);
  assert.match(script, /Public schemas were not found/);
  assert.match(script, /schemasDir/);
  assert.match(script, /writeRuntimeManifest/);
  const manifest = fs.readFileSync(path.join(ROOT, 'src', 'runtime-manifest.js'), 'utf8');
  assert.match(manifest, /sidecars\/\$\{platform === 'win32' \? 'autopsy\.exe' : 'autopsy'\}/);
});

test('Phase 5 packaged smoke authenticates, indexes, and checks a real report receipt', () => {
  const main = fs.readFileSync(path.join(ROOT, 'src', 'main.js'), 'utf8');
  const smoke = fs.readFileSync(path.join(ROOT, 'scripts', 'smoke-packaged.js'), 'utf8');
  assert.match(main, /SYNEVID_SMOKE/);
  assert.match(main, /PACKAGED_SMOKE_PASS/);
  assert.match(main, /waitForApplicationHealth/);
  assert.match(main, /runPackagedSmokeWorkflow/);
  assert.match(main, /PACKAGED_SMOKE_ANALYSIS_PASS/);
  assert.match(main, /PACKAGED_SMOKE_UI_RECEIPT_PASS/);
  assert.match(main, /PACKAGED_SMOKE_MCP_PASS/);
  assert.match(main, /verifyVisibleAnalyticsReceipt/);
  assert.match(main, /\.synevid-smoke-receipt\.json/);
  assert.match(main, /schema_version: 1/);
  assert.match(main, /rendered\.repository\?\.repo_id !== expected\.repositoryId/);
  assert.match(main, /unauthenticated\.statusCode !== 401 \|\| invalidToken\.statusCode !== 401/);
  assert.match(main, /api\/v1\/repos\/connect/);
  assert.match(main, /api\/v1\/analytics\/report\?format=json/);
  assert.match(main, /report\.result_digest !== reportEnvelope\.digest/);
  assert.match(main, /verifyRuntimeManifest\(process\.resourcesPath\)/);
  assert.match(main, /Authorization: `Bearer \$\{LOOPBACK_TOKEN\}`/);
  assert.match(smoke, /SynEvid\.app.*Contents.*MacOS/s);
  assert.match(smoke, /linux-unpacked.*synevid/);
  assert.match(smoke, /delete cleanEnvironment\.ELECTRON_RUN_AS_NODE/);
  assert.match(smoke, /SYNEVID_SMOKE_REPO/);
  assert.match(smoke, /smokeRepository/);
  assert.match(smoke, /\.synevid-smoke-receipt\.json/);
  assert.match(smoke, /receipt\?\.schema_version === 1/);
  assert.match(smoke, /mcp_trajectory_persisted === true/);
  assert.match(smoke, /ui_receipt_verified === true/);
  assert.match(smoke, /mcp_trajectory_persisted === true/);
  assert.match(smoke, /taskkill.*\/T.*\/F/);
});

test('Phase 5 release artifact verification covers every native target', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
  const verifier = fs.readFileSync(path.join(ROOT, 'scripts', 'verify-artifacts.js'), 'utf8');
  assert.match(pkg.scripts['verify:artifacts'], /verify-artifacts\.js/);
  assert.match(verifier, /Windows NSIS installer/);
  assert.match(verifier, /macOS disk image/);
  assert.match(verifier, /Linux AppImage/);
});

test('native artifact verifier rejects missing, empty, or malformed platform outputs', () => {
  const fixtures = {
    win32: ['SynEvid-0.6.0-win-x64-setup.exe', 'SynEvid-0.6.0-win-x64-portable.exe', 'SynEvid-0.6.0-win-x64.msi'],
    darwin: ['SynEvid-0.6.0-mac-arm64.dmg', 'SynEvid-0.6.0-mac-arm64.zip'],
    linux: ['SynEvid-0.6.0-linux-x64.AppImage', 'SynEvid-0.6.0-linux-x64.deb', 'SynEvid-0.6.0-linux-x64.rpm'],
  };
  const platformNames = { win32: 'Windows', darwin: 'macOS', linux: 'Linux' };
  const dist = fs.mkdtempSync(path.join(os.tmpdir(), 'synevid-artifact-verifier-'));
  try {
    for (const [platform, files] of Object.entries(fixtures)) {
      fs.rmSync(dist, { recursive: true, force: true });
      fs.mkdirSync(dist);
      for (const file of files) fs.writeFileSync(path.join(dist, file), 'artifact');
      for (const component of ['electron', 'cockpit', 'mcp']) {
        fs.writeFileSync(path.join(dist, `sbom-synevid-${component}-${platformNames[platform]}.cdx.json`), JSON.stringify({
          bomFormat: 'CycloneDX', specVersion: '1.6', components: [{ name: component }],
        }));
      }
      assert.equal(verifyArtifacts(dist, platform).platform, platform);
    }

    fs.writeFileSync(path.join(dist, 'sbom-synevid-mcp-Linux.cdx.json'), '{invalid');
    assert.throws(() => verifyArtifacts(dist, 'linux'), /invalid SBOM/);
    fs.unlinkSync(path.join(dist, 'SynEvid-0.6.0-linux-x64.rpm'));
    assert.throws(() => verifyArtifacts(dist, 'linux'), /missing target/);
  } finally {
    fs.rmSync(dist, { recursive: true, force: true });
  }
});

test('Phase 5 native install smoke covers required Windows, macOS, and Linux artifacts', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
  const installerSmoke = fs.readFileSync(path.join(ROOT, 'scripts', 'smoke-installers.js'), 'utf8');
  assert.equal(pkg.scripts['smoke:installers'], 'node scripts/smoke-installers.js');
  assert.match(installerSmoke, /function smokeWindows/);
  assert.match(installerSmoke, /setup\\\.exe/);
  assert.match(installerSmoke, /function smokeMac/);
  assert.match(installerSmoke, /hdiutil/);
  assert.match(installerSmoke, /function smokeLinux/);
  assert.match(installerSmoke, /dpkg/);
  assert.match(installerSmoke, /AppImage/);
  assert.match(installerSmoke, /smokeExecutable\(executable\)/);
});

test('Phase 5 packaged UI smoke verifies the visible report matches daemon receipt', () => {
  const main = fs.readFileSync(path.join(ROOT, 'src', 'main.js'), 'utf8');
  const smoke = fs.readFileSync(path.join(ROOT, 'scripts', 'smoke-packaged.js'), 'utf8');
  assert.match(main, /section\[aria-label="Analytics reports"\]/);
  assert.match(main, /rendered\.repository\?\.repo_id !== expected\.repositoryId/);
  assert.match(main, /rendered\.snapshot\?\.snapshot_id !== expected\.snapshotId/);
  assert.match(main, /visibleDigest !== expected\.digest/);
  assert.match(smoke, /validReceipt/);
});
