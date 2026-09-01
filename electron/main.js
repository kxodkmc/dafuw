//! Electron 主进程：创建窗口加载纯 Web 前端，并负责托管本机 Rust 服务器进程。
//! 渲染进程不使用 Node 能力（sandbox + contextIsolation），与浏览器环境行为一致。

const { app, BrowserWindow, shell, ipcMain } = require('electron');
const { statSync, mkdirSync } = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

const INDEX_HTML = path.join(__dirname, '..', 'frontend', 'index.html');
const PROJECT_ROOT = path.join(__dirname, '..');

const HEALTH_TIMEOUT_MS = 2000;
const START_TIMEOUT_MS = 20000;
const DEFAULT_PORT = 8080;

let serverProc = null; // 由本应用拉起的服务器子进程
let startingPromise = null; // 防止并发触发重复拉起

/* ---- 本机服务器托管 ---- */

const LOOPBACK = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);

function findServerBinary() {
  const cli = process.platform === 'win32' ? 'monopoly-cli.exe' : 'monopoly-cli';
  const candidates = [
    process.env.MONOPOLY_SERVER_BIN,
    // 打包后：<resources/app>/bin/monopoly-cli.exe
    path.join(__dirname, '..', 'bin', cli),
    // 开发：项目 target 目录
    path.join(PROJECT_ROOT, 'target', 'release', cli),
    path.join(PROJECT_ROOT, 'target', 'debug', cli),
  ].filter(Boolean);
  return candidates.find((p) => {
    try { return statSync(p).isFile(); } catch { return false; }
  }) ?? null;
}

async function fetchHealth(origin) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), HEALTH_TIMEOUT_MS);
  try {
    const res = await fetch(`${origin}/health`, { signal: ctrl.signal });
    return res.ok ? await res.json().catch(() => null) : null;
  } catch { return null; }
  finally { clearTimeout(timer); }
}

async function startServerProcess(origin, port) {
  const bin = findServerBinary();
  if (!bin) {
    return {
      ok: false,
      error: '未找到服务器程序（monopoly-cli）。请先在项目根目录执行 '
        + 'cargo build --release -p monopoly-cli，或设置环境变量 MONOPOLY_SERVER_BIN 指向可执行文件。',
    };
  }
  console.log(`[服务器] 启动 ${bin} --bind 0.0.0.0 --port ${port}`);
  // 打包后写入用户数据目录（避免安装目录只读）；开发时写入项目 data/。
  const cwd = app.isPackaged ? app.getPath('userData') : PROJECT_ROOT;
  try { mkdirSync(cwd, { recursive: true }); } catch { /* 已存在 */ }
  serverProc = spawn(bin, ['serve', '--bind', '0.0.0.0', '--port', String(port)], {
    cwd,
    stdio: ['ignore', 'inherit', 'inherit'],
    windowsHide: true,
  });
  serverProc.on('exit', (code) => {
    console.log(`[服务器] 进程退出 code=${code}`);
    serverProc = null;
  });

  const deadline = Date.now() + START_TIMEOUT_MS;
  while (Date.now() < deadline) {
    await new Promise((r) => setTimeout(r, 300));
    if (await fetchHealth(origin)) return { ok: true, started: true };
    if (!serverProc) return { ok: false, error: '服务器进程启动失败，请查看终端日志' };
  }
  return { ok: false, error: '服务器启动超时，请查看终端日志' };
}

function ensureLocalServer(origin, port) {
  if (startingPromise) return startingPromise;
  startingPromise = (async () => {
    // 已在线（手动启动或此前已拉起）→ 直接复用，不重复开进程
    if (await fetchHealth(origin)) return { ok: true, started: false };
    return startServerProcess(origin, port);
  })().finally(() => { startingPromise = null; });
  return startingPromise;
}

ipcMain.handle('host:ensure-server', (_event, baseUrl) => {
  let url;
  try { url = new URL(baseUrl); } catch { return { ok: false, error: '服务器地址无效' }; }
  if (url.protocol !== 'http:' && url.protocol !== 'https:') {
    return { ok: false, error: '服务器地址需以 http:// 开头' };
  }
  if (!LOOPBACK.has(url.hostname.toLowerCase())) {
    return { ok: false, error: `目标 ${url.hostname} 不是本机地址，无法自动开服` };
  }
  return ensureLocalServer(url.origin, Number(url.port) || DEFAULT_PORT);
});

/* ---- 窗口 ---- */

function createWindow() {
  const win = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 960,
    minHeight: 640,
    title: '大富翁',
    backgroundColor: '#f3efe3',
    autoHideMenuBar: true,
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
      preload: path.join(__dirname, 'preload.js'),
    },
  });

  // 渲染进程日志转发到终端，便于排查前端问题
  win.webContents.on('console-message', (event, level, message) => {
    const text = typeof event === 'object' && event !== null && event.message !== undefined
      ? `${event.message} (${event.sourceId}:${event.lineNumber})`
      : message;
    console.log('[前端]', text);
  });

  // F12 切换开发者工具
  win.webContents.on('before-input-event', (event, input) => {
    if (input.type === 'keyDown' && input.key === 'F12') {
      win.webContents.toggleDevTools();
      event.preventDefault();
    }
  });

  // 外部链接交给系统浏览器，避免在应用内跳走。
  win.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url);
    return { action: 'deny' };
  });

  win.loadFile(INDEX_HTML);
}

app.whenReady().then(() => {
  createWindow();
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow();
  });
});

// 应用退出时回收由本应用拉起的服务器进程
app.on('before-quit', () => {
  if (serverProc) {
    try { serverProc.kill(); } catch { /* 进程可能已退出 */ }
    serverProc = null;
  }
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit();
});
