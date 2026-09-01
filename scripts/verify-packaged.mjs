// 打包验证脚本：通过 CDP 在打包应用渲染进程里调用 ensureServer，并探测服务器健康。
const list = await fetch('http://127.0.0.1:9222/json/list').then((r) => r.json());
const target = list.find((t) => t.type === 'page');
if (!target) { console.error('VERIFY-FAIL: page target not found'); process.exit(1); }
const ws = new WebSocket(target.webSocketDebuggerUrl);
let id = 0;
const pending = new Map();

function send(method, params) {
  return new Promise((resolve, reject) => {
    const mid = ++id;
    pending.set(mid, { resolve, reject });
    ws.send(JSON.stringify({ id: mid, method, params }));
  });
}

ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
  }
};

ws.onopen = async () => {
  try {
    const expr = `window.dafuwHost.ensureServer('http://127.0.0.1:8080')`;
    const res = await send('Runtime.evaluate', {
      expression: expr,
      awaitPromise: true,
      returnByValue: true,
    });
    console.log('ensureServer result:', JSON.stringify(res.result.value));
    const health = await fetch('http://127.0.0.1:8080/health').then((r) => r.json());
    console.log('health:', JSON.stringify(health));
    const rooms = await fetch('http://127.0.0.1:8080/api/rooms').then((r) => r.json());
    console.log('rooms:', rooms.length);
    process.exit(0);
  } catch (e) {
    console.error('VERIFY-FAIL:', e.message);
    process.exit(1);
  }
};
ws.onerror = () => { console.error('WS error'); process.exit(1); };
