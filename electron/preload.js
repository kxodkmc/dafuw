//! preload：通过 contextBridge 向渲染进程暴露最小受控 API。
//! sandbox 与 contextIsolation 保持开启，渲染进程依旧没有 Node 能力。

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('dafuwHost', {
  /** 请求主进程确保本机服务器在线（未在线则自动拉起进程）。 */
  ensureServer: (baseUrl) => ipcRenderer.invoke('host:ensure-server', String(baseUrl ?? '')),
});
