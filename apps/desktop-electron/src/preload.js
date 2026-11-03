const { contextBridge, ipcRenderer } = require('electron');

// Hardened Preload Bridge
// Complies with FR-P5-001 and INV-SANDBOX-004 (Offline local-only sandboxing)
contextBridge.exposeInMainWorld('synevidDesktop', {
  isDesktop: true,
  platform: process.platform,

  // Native OS Folder Picker
  openDirectory: () => ipcRenderer.invoke('dialog:openDirectory'),

  // Native Window Controls
  minimizeWindow: () => ipcRenderer.send('window:minimize'),
  maximizeWindow: () => ipcRenderer.send('window:maximize'),
  closeWindow: () => ipcRenderer.send('window:close'),

  // Daemon Status Query
  getDaemonStatus: () => ipcRenderer.invoke('daemon:status'),
  // Synchronous by design: renderer fetch construction must attach the
  // ephemeral token before the request is sent.
  getDaemonToken: () => ipcRenderer.sendSync('daemon:token-sync'),

  // Electron compositor presentation meter
  startFrameMeter: () => ipcRenderer.invoke('frame-meter:start'),
  getFrameMeterStats: () => ipcRenderer.invoke('frame-meter:stats'),
  stopFrameMeter: () => ipcRenderer.invoke('frame-meter:stop'),
  markNextPresentedFrame: () => ipcRenderer.sendSync('frame-meter:mark-sync'),
});
