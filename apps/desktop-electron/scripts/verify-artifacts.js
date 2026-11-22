'use strict';

// Verifies native release artifacts and their CycloneDX dependency manifests.
const fs = require('node:fs');
const path = require('node:path');

const desktopDir = path.resolve(__dirname, '..');
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
const sbomPlatform = { win32: 'Windows', darwin: 'macOS', linux: 'Linux' };

function verifyArtifacts(distDir, platform) {
  const targets = requirements[platform];
  if (!targets) throw new Error(`Unsupported release platform: ${platform}`);
  if (!fs.existsSync(distDir)) throw new Error(`Missing output directory: ${distDir}`);

  const sboms = ['electron', 'cockpit', 'mcp'].map((component) => ({
    label: `SynEvid ${component} CycloneDX SBOM`,
    file: `sbom-synevid-${component}-${sbomPlatform[platform]}.cdx.json`,
  }));
  const files = fs.readdirSync(distDir, { withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => entry.name);
  const missingTargets = targets.filter(({ pattern }) => !files.some((file) => pattern.test(file)));
  const emptyTargets = targets
    .map(({ label, pattern }) => {
      const file = files.find((candidate) => pattern.test(candidate));
      return file && fs.statSync(path.join(distDir, file)).size > 0 ? null : label;
    })
    .filter(Boolean);
  const invalidSboms = sboms.filter(({ file }) => {
    const fullPath = path.join(distDir, file);
    if (!fs.existsSync(fullPath) || fs.statSync(fullPath).size === 0) return true;
    try {
      const bom = JSON.parse(fs.readFileSync(fullPath, 'utf8'));
      return bom.bomFormat !== 'CycloneDX' || !bom.specVersion || !Array.isArray(bom.components);
    } catch {
      return true;
    }
  });

  if (missingTargets.length || emptyTargets.length || invalidSboms.length) {
    const details = [
      ...missingTargets.map(({ label }) => `missing target: ${label}`),
      ...emptyTargets.map((label) => `empty target: ${label}`),
      ...invalidSboms.map(({ label }) => `missing, empty, or invalid SBOM: ${label}`),
    ];
    throw new Error(details.join('\n'));
  }
  return { platform, targets: targets.map(({ label }) => label), sboms: sboms.map(({ label }) => label) };
}

if (require.main === module) {
  const platform = process.env.SYNEVID_RELEASE_OS || process.platform;
  try {
    console.log('ELECTRON_ARTIFACTS_PASS', verifyArtifacts(path.join(desktopDir, 'dist'), platform));
  } catch (error) {
    console.error(`[SynEvid Artifacts] ${error.message.replaceAll('\n', '\n- ')}`);
    process.exitCode = 1;
  }
}

module.exports = { verifyArtifacts };
