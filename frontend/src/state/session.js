//! 本地会话持久化：服务器地址与房间身份（localStorage）。
//! 身份包含令牌，重载应用后可据此恢复房间连接，无需重新 join。

const KEY_SERVER = 'dafuw.server';
const KEY_IDENT = 'dafuw.ident';

const DEFAULT_SERVER = 'http://127.0.0.1:8080';

export function loadServer() {
  return localStorage.getItem(KEY_SERVER) || DEFAULT_SERVER;
}

export function saveServer(url) {
  localStorage.setItem(KEY_SERVER, url.replace(/\/+$/, ''));
}

/**
 * @returns {{roomCode:number, playerId:string, token:string, isOwner:boolean, ownerToken:string} | null}
 */
export function loadIdent() {
  try {
    const raw = localStorage.getItem(KEY_IDENT);
    if (!raw) return null;
    const ident = JSON.parse(raw);
    if (!ident.roomCode || !ident.playerId || !ident.token) return null;
    return ident;
  } catch {
    return null;
  }
}

export function saveIdent(ident) {
  localStorage.setItem(KEY_IDENT, JSON.stringify(ident));
}

export function clearIdent() {
  localStorage.removeItem(KEY_IDENT);
}

/* ---- 全局玩家身份：一个客户端一个唯一身份，所有房间共用 ----
 * 身份码 = DAFUWP1-<base64url(JSON{uid,secret})>
 * join 时自动携带；服务器凭 uid 认领原座位（跨天/重进/换设备恢复原角色）。 */

const KEY_PID = 'dafuw.player-identity';
const PID_PREFIX = 'DAFUWP1-';

const b64urlEncode = (str) =>
  btoa(String.fromCharCode(...new TextEncoder().encode(str)))
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/, '');

const b64urlDecode = (s) => {
  let t = s.replace(/-/g, '+').replace(/_/g, '/');
  while (t.length % 4) t += '=';
  const bin = atob(t);
  return new TextDecoder().decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
};

/** 读取本机身份；不存在返回 null。 */
export function loadPlayerIdentity() {
  try {
    const raw = localStorage.getItem(KEY_PID);
    if (!raw) return null;
    const p = JSON.parse(raw);
    if (!p.uid || !p.secret) return null;
    return p;
  } catch {
    return null;
  }
}

export function savePlayerIdentity(pid) {
  localStorage.setItem(KEY_PID, JSON.stringify(pid));
}

/** 生成全新身份（重新生成/首次使用）。 */
export function newPlayerIdentity() {
  return { uid: crypto.randomUUID(), secret: crypto.randomUUID() };
}

/** 取本机身份，没有则自动创建并保存（对用户完全透明）。 */
export function getOrCreatePlayerIdentity() {
  return loadPlayerIdentity() ?? (savePlayerIdentity(newPlayerIdentity()), loadPlayerIdentity());
}

export function clearPlayerIdentity() {
  localStorage.removeItem(KEY_PID);
}

/** 导出全局身份码；无身份返回 null。 */
export function exportPlayerIdentity(pid = loadPlayerIdentity()) {
  if (!pid) return null;
  return PID_PREFIX + b64urlEncode(JSON.stringify(pid));
}

/** 解析全局身份码；格式非法返回 null。 */
export function importPlayerIdentity(raw) {
  try {
    const text = String(raw ?? '').trim().replace(/\s+/g, '');
    if (!text.startsWith(PID_PREFIX)) return null;
    const pid = JSON.parse(b64urlDecode(text.slice(PID_PREFIX.length)));
    if (!pid.uid || !pid.secret) return null;
    return pid;
  } catch {
    return null;
  }
}
