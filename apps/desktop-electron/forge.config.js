const path = require('path');

module.exports = {
  packagerConfig: {
    asar: true,
    executableName: 'SynEvid',
    icon: path.join(__dirname, 'assets', 'icon'),
    extraResource: [
      'assets/schemas',
      'assets/migrations',
      '.autopsy',
    ],
    ignore: /^\.git/,
    extraMetadata: {
      author: 'Synevid Core Team',
      description: 'SynEvid Code Autopsy Desktop Application',
    },
  },
  makers: [
    {
      name: '@electron-forge/maker-squirrel',
      config: {
        name: 'SynEvid',
        authors: 'Synevid Core Team',
        exe: 'SynEvid.exe',
        setupIcon: path.join(__dirname, 'assets', 'icon.ico'),
        loadingGif: path.join(__dirname, 'assets', 'installer.gif'),
        noMsi: true,
        setupExe: 'SynEvid-Setup-${version}.exe',
        remoteReleases: 'https://github.com/sainibhaowal/SynEvid/releases',
      },
      platforms: ['win32'],
    },
    {
      name: '@electron-forge/maker-dmg',
      config: {
        name: 'SynEvid',
        icon: path.join(__dirname, 'assets', 'icon.icns'),
        background: path.join(__dirname, 'assets', 'dmg-background.png'),
        window: { x: 400, y: 220, width: 540, height: 380 },
        contents: [
          { x: 180, y: 170, type: 'file' },
          { x: 420, y: 170, type: 'link', path: '/Applications' },
        ],
      },
      platforms: ['darwin'],
    },
    {
      name: '@electron-forge/maker-deb',
      config: {
        options: {
          name: 'synevid',
          productName: 'SynEvid',
          version: '${version}',
          section: 'devel',
          priority: 'optional',
          maintainer: 'Synevid Core Team <team@synevid.dev>',
          homepage: 'https://github.com/sainibhaowal/SynEvid',
          description: 'SynEvid Code Autopsy Desktop Application',
          icon: path.join(__dirname, 'assets', 'icon.png'),
        },
      },
      platforms: ['linux'],
    },
    {
      name: '@electron-forge/maker-appimage',
      config: {
        options: {
          name: 'SynEvid',
          icon: path.join(__dirname, 'assets', 'icon.png'),
        },
      },
      platforms: ['linux'],
    },
  ],
  plugins: [
    {
      name: '@electron-forge/plugin-auto-unpack-natives',
      config: {},
    },
    {
      name: '@electron-forge/plugin-fuses',
      config: {
        version: 'v1',
        runAsNode: false,
        enableCookieEncryption: true,
        enableNodeOptionsEnvironmentVariable: false,
        enableNodeCliInspectArguments: false,
        enableEmbeddedAsarIntegrityValidation: true,
        onlyLoadAppFromAsar: true,
        loadBrowserProcessSpecificV8Snapshot: false,
      },
    },
  ],
  plugins: [
    {
      name: '@electron-forge/plugin-auto-unpack-natives',
      config: {},
    },
    {
      name: '@electron-forge/plugin-fuses',
      config: {
        version: 'v1',
        runAsNode: false,
        enableCookieEncryption: true,
        enableNodeOptionsEnvironmentVariable: false,
        enableNodeCliInspectArguments: false,
        enableEmbeddedAsarIntegrityValidation: true,
        onlyLoadAppFromAsar: true,
        loadBrowserProcessSpecificV8Snapshot: false,
      },
    },
  ],
  hooks: {
    packageAfterPrune: async (config, options) => {
      const { copyFileSync, mkdirSync, existsSync } = require('fs');
      const { resolve } = require('path');
      const resourcesDir = resolve(options.appOutDir, 'resources');
      
      // Copy daemon binary
      const daemonSrc = resolve(__dirname, '..', '..', '..', 'target', 'release', 'synevid-ui');
      const daemonDest = resolve(resourcesDir, process.platform === 'win32' ? 'synevid-ui.exe' : 'synevid-ui');
      if (existsSync(daemonSrc)) {
        copyFileSync(daemonSrc, daemonDest);
      }
      
      // Copy MCP bundle
      const mcpSrc = resolve(__dirname, '..', 'mcp-server', 'dist', 'index.bundle.js');
      const mcpDest = resolve(resourcesDir, 'mcp-server', 'index.bundle.js');
      if (existsSync(mcpSrc)) {
        mkdirSync(resolve(resourcesDir, 'mcp-server'), { recursive: true });
        copyFileSync(mcpSrc, mcpDest);
      }
      
      // Copy schemas
      const schemasSrc = resolve(__dirname, '..', '..', '..', 'schemas');
      const schemasDest = resolve(resourcesDir, 'schemas');
      if (existsSync(schemasSrc)) {
        require('fs').cpSync(schemasSrc, schemasDest, { recursive: true });
      }
    },
  },
};