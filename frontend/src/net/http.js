//! REST 客户端：对应 monopoly-server 的 HTTP 接口。
//! 只做请求与错误转译，不含任何业务状态。

export function createHttp(baseUrl) {
  const root = baseUrl.replace(/\/+$/, '');
  const httpUrl = (p) => `${root}${p}`;

  const wsBase = root.replace(/^http/, 'ws');
  const wsUrl = (p) => `${wsBase}${p}`;

  /** 统一请求：非 2xx 时抛出携带服务器 message 的 Error。 */
  async function request(method, path, body) {
    let res;
    try {
      res = await fetch(httpUrl(path), {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch {
      throw new Error('无法连接服务器，请检查地址与网络');
    }
    if (!res.ok) {
      let msg = `请求失败（HTTP ${res.status}）`;
      try {
        const data = await res.json();
        msg = data.message || data.error || msg;
      } catch { /* 保留默认文案 */ }
      throw new Error(msg);
    }
    if (res.status === 204) return null;
    return res.json();
  }

  return {
    wsUrl,

    /** 就绪探针：GET /health */
    health: () => request('GET', '/health'),

    /** 房间列表：GET /api/rooms */
    listRooms: () => request('GET', '/api/rooms'),

    /** 建房：POST /api/rooms → { room_code, owner_token } */
    createRoom: (settings, name) =>
      request('POST', '/api/rooms', { settings, name }),

    /** 入座：POST /api/rooms/{code}/join → JoinRes（携带全局身份 uid+secret） */
    joinRoom: (code, identity) =>
      request('POST', `/api/rooms/${code}/join`, {
        player_uid: identity?.uid,
        secret: identity?.secret,
      }),

    /** 快照：GET /api/rooms/{code} → GameStateDto */
    snapshot: (code) => request('GET', `/api/rooms/${code}`),

    /** 房主开局：POST /api/rooms/{code}/start */
    startGame: (code, owner_token) =>
      request('POST', `/api/rooms/${code}/start`, { owner_token }),

    /** 房主解散：DELETE /api/rooms/{code} */
    closeRoom: (code, owner_token) =>
      request('DELETE', `/api/rooms/${code}`, { owner_token }),
  };
}
