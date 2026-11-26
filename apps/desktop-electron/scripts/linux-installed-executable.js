const fs = require('node:fs');

function findLinuxInstalledExecutable(installedFiles, fileSystem = fs) {
  return installedFiles
    .split(/\r?\n/)
    .find((item) => {
      if (!/\/synevid$/.test(item)) return false;
      try {
        return fileSystem.statSync(item).isFile();
      } catch {
        return false;
      }
    });
}

module.exports = { findLinuxInstalledExecutable };
