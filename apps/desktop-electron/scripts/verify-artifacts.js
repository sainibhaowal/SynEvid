// Verifies that a native Electron release job produced every target declared
// for the current operating system before the packaged health smoke test runs.
const fs = require('node:fs');
const path = require('node:path');

const desktopDir = path.resolve(__dirname, '..');
const distDir = path.join(desktopDir, 'dist');
const platform = process.env.SYNEVID_RELEASE_OS || process.platform;
const requirements = {
  win32: [
    { label: 'Windows NSIS installer', pattern: /-setup\.exe$/i },
    { label: 'Windows portable executable', pattern: /-portable\.exe$/i },
    { label: 'Windows MSI installer', pattern: /\.msi$/i },
  ],
  darwin: [
    { label: 'macOS disk image', pattern: /\.dmg$/i },
    { label: 'macOS archive', pattern: /\.zip$/i },
  ],
  linux: [
    { label: 'Linux AppImage', pattern: /\.AppImage$/i },
    { label: 'Linux Debian package', pattern: /\.deb$/i },
    { label: 'Linux RPM package', pattern: /\.rpm$/i },
  ],
};

const expected = requirements[platform];
if (!expected) {
  console.error(`[SynEvid Artifacts] Unsupported release platform: ${platform}`);
  process.exit(2);
}
if (!fs.existsSync(distDir)) {
  console.error(`[SynEvid Artifacts] Missing output directory: ${distDir}`);
  process.exit(1);
}

const files = fs.readdirSync(distDir, { withFileTypes: true })
  .filter((entry) => entry.isFile())
  .map((entry) => entry.name);
const missing = expected.filter(({ pattern }) => !files.some((file) => pattern.test(file)));
const empty = expected
  .map(({ label, pattern }) => {
    const file = files.find((candidate) => pattern.test(candidate));
    return file && fs.statSync(path.join(distDir, file)).size > 0 ? null : label;
  })
  .filter(Boolean);
if (missing.length > 0) {
  console.error('[SynEvid Artifacts] Missing required targets:');
  for (const item of missing) console.error(`- ${item.label}`);
  console.error(`Output directory: ${distDir}`);
  process.exit(1);
}
if (empty.length > 0) {
  console.error('[SynEvid Artifacts] Empty required targets:');
  for (const label of empty) console.error(`- ${label}`);
  console.error(`Output directory: ${distDir}`);
  process.exit(1);
}

console.log('ELECTRON_ARTIFACTS_PASS', {
  platform,
  targets: expected.map(({ label }) => label),
});
