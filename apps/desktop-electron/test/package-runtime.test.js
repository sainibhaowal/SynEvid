const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

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
  assert.deepEqual(resourceTargets, ['cockpit', 'deps', 'sidecars', 'mcp-server']);
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
  assert.match(script, /MCP bundle was not produced/);
});

test('Phase 5 packaged smoke path is health-gated', () => {
  const main = fs.readFileSync(path.join(ROOT, 'src', 'main.js'), 'utf8');
  const smoke = fs.readFileSync(path.join(ROOT, 'scripts', 'smoke-packaged.js'), 'utf8');
  assert.match(main, /SYNEVID_SMOKE/);
  assert.match(main, /PACKAGED_SMOKE_PASS/);
  assert.match(main, /waitForApplicationHealth/);
  assert.match(main, /Authorization: `Bearer \$\{LOOPBACK_TOKEN\}`/);
  assert.match(smoke, /SynEvid\.app.*Contents.*MacOS/s);
  assert.match(smoke, /linux-unpacked.*synevid/);
  assert.match(smoke, /delete cleanEnvironment\.ELECTRON_RUN_AS_NODE/);
});

test('Phase 5 release artifact verification covers every native target', () => {
  const pkg = JSON.parse(fs.readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
  const verifier = fs.readFileSync(path.join(ROOT, 'scripts', 'verify-artifacts.js'), 'utf8');
  assert.match(pkg.scripts['verify:artifacts'], /verify-artifacts\.js/);
  assert.match(verifier, /Windows NSIS installer/);
  assert.match(verifier, /macOS disk image/);
  assert.match(verifier, /Linux AppImage/);
});
