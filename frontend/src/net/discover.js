//! 局域网发现：WebRTC 取本机内网 IP → 并发扫描同网段 /24 的大富翁服务器。

/** 通过 WebRTC ICE 候选收集本机内网 IPv4（渲染进程无 Node 权限时的标准做法）。 */
function localIps(timeoutMs = 1500) {
  return new Promise((resolve) => {
    const ips = new Set();
    let pc;
    try {
      pc = new RTCPeerConnection({ iceServers: [] });
      pc.createDataChannel('probe');
      pc.onicecandidate = (e) => {
        if (!e.candidate) return finish();
        const m = /(\d{1,3}(?:\.\d{1,3}){3})/.exec(e.candidate.candidate);
        if (m && !m[1].startsWith('127.')) ips.add(m[1]);
      };
      pc.createOffer().then((offer) => pc.setLocalDescription(offer)).catch(finish);
    } catch {
      return finish();
    }
    const timer = setTimeout(finish, timeoutMs);
    function finish() {
      clearTimeout(timer);
      try { pc?.close(); } catch { /* 已关闭 */ }
      resolve([...ips]);
    }
  });
}

/** 带超时的 GET JSON；失败返回 null。 */
async function fetchJson(url, timeoutMs) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const res = await fetch(url, { signal: ctrl.signal });
    if (!res.ok) return null;
    return await res.json();
  } catch {
    return null;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * 扫描本机所在 IPv4 网段的大富翁服务器。
 * @param {{port?:number, timeout?:number, onProgress?:(done:number,total:number)=>void}} [opts]
 * @returns {Promise<Array<{url:string, version:string, rooms:Array<object>}>>}
 */
export async function scanLan({ port = 8080, timeout = 1200, onProgress } = {}) {
  const ips = await localIps();
  if (ips.length === 0) return [];

  const hosts = [];
  const seen = new Set();
  for (const ip of ips) {
    const prefix = ip.split('.').slice(0, 3).join('.');
    for (let i = 1; i <= 254; i++) {
      const host = `${prefix}.${i}`;
      if (host !== ip && !seen.has(host)) {
        seen.add(host);
        hosts.push(host);
      }
    }
  }

  const found = [];
  let done = 0;
  const CONCURRENCY = 64;
  const queue = [...hosts];

  async function worker() {
    while (queue.length) {
      const host = queue.shift();
      const base = `http://${host}:${port}`;
      const health = await fetchJson(`${base}/health`, timeout);
      if (health?.service === 'monopoly-server') {
        const rooms = (await fetchJson(`${base}/api/rooms`, timeout)) ?? [];
        found.push({ url: base, version: health.version, rooms });
      }
      done += 1;
      onProgress?.(done, hosts.length);
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(CONCURRENCY, hosts.length) }, () => worker()),
  );

  return found.sort((a, b) => b.rooms.length - a.rooms.length);
}
