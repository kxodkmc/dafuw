//! 宿主集成：Electron 环境下建房前自动拉起本机服务器；浏览器环境为空操作。

const LOOPBACK = new Set(['localhost', '127.0.0.1', '::1', '[::1]']);
const hostApi = globalThis.dafuwHost ?? null;

function isLoopbackUrl(url) {
  try {
    const u = new URL(url);
    return (u.protocol === 'http:' || u.protocol === 'https:')
      && LOOPBACK.has(u.hostname.toLowerCase());
  } catch { return false; }
}

/**
 * 请求 Electron 宿主确保本机服务器在线（未在线则自动启动进程）。
 * 浏览器环境或目标是远程地址时返回 null，不阻断建房流程。
 * @param {string} serverUrl 服务器根地址，如 http://127.0.0.1:8080
 * @returns {Promise<?{ok:boolean, started?:boolean, error?:string}>}
 */
export async function ensureLocalServer(serverUrl) {
  if (!hostApi || !isLoopbackUrl(serverUrl)) return null;
  try {
    return await hostApi.ensureServer(serverUrl);
  } catch (e) {
    return { ok: false, error: `自动启动服务器失败：${e?.message ?? e}` };
  }
}
