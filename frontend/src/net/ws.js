//! WebSocket 连接：自动重连 + 状态回调。
//! 断线重连成功后，服务器会主动推送新的 RoomSnapshot，客户端无需补拉。

/** 重连退避序列（毫秒），耗尽后从头开始。 */
const BACKOFF_MS = [1000, 2000, 4000, 8000, 10000];

/**
 * @param {string} url 完整 ws 地址（含 token 查询参数）
 * @param {{onMessage:(data:object)=>void, onStatus:(connected:boolean, retrying:boolean)=>void}} hooks
 * @returns {{close:()=>void}}
 */
export function createSocket(url, hooks) {
  let ws = null;
  let closedByUser = false;
  let attempt = 0;
  let timer = null;

  function connect() {
    ws = new WebSocket(url);

    ws.onopen = () => {
      console.info('[ws] 已连接');
      attempt = 0;
      hooks.onStatus(true, false);
    };

    ws.onmessage = (e) => {
      try {
        hooks.onMessage(JSON.parse(e.data));
      } catch (err) {
        console.error('[ws] 消息解析失败', err);
      }
    };

    ws.onclose = (e) => {
      // 用户主动关闭（如切换房间）：不通知状态、不重连，
      // 避免旧连接的 close 事件踩踏新连接的 onStatus。
      if (closedByUser) return;
      hooks.onStatus(false, true);
      const delay = BACKOFF_MS[Math.min(attempt, BACKOFF_MS.length - 1)];
      attempt += 1;
      console.info(`[ws] 连接断开（code=${e.code}），${delay}ms 后第 ${attempt} 次重连`);
      timer = setTimeout(connect, delay);
    };

    ws.onerror = () => ws?.close();
  }

  connect();

  // 薄包装：暴露底层 WebSocket 的关键能力，同时保留 close/重连控制
  return {
    get readyState() { return ws?.readyState; },
    get url() { return ws?.url; },
    send(text) {
      if (ws?.readyState === WebSocket.OPEN) ws.send(text);
    },
    /** OPEN 后回调一次（握手期排队发送用）。 */
    whenOpen(cb) {
      if (ws?.readyState === WebSocket.OPEN) return cb();
      ws?.addEventListener('open', () => cb(), { once: true });
    },
    close() {
      closedByUser = true;
      clearTimeout(timer);
      ws?.close();
    },
  };
}
