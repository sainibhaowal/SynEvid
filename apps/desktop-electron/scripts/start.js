// Launcher: ensures Electron starts as a real Electron app even when the parent
// shell exports ELECTRON_RUN_AS_NODE (which makes `require('electron')` undefined).
const { spawn } = require('child_process');
const path = require('path');

const env = { ...process.env };
delete env.ELECTRON_RUN_AS_NODE;

const electronBin = require('electron');
const main = path.join(__dirname, '..', 'src', 'main.js');
const child = spawn(electronBin, [main, ...process.argv.slice(2)], { stdio: 'inherit', env });
child.on('exit', (code) => process.exit(code ?? 0));

