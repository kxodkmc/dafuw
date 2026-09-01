//! 大厅页：服务器连接（自动探测/局域网扫描）、创建房间、加入房间、房间列表。

import { h, fmtCode, copyText } from './dom.js';
import { STATUS_LABELS } from '../protocol/types.js';
import { createHttp } from '../net/http.js';
import { scanLan } from '../net/discover.js';
import { ensureLocalServer } from '../net/host.js';
import {
  loadIdent, clearIdent, saveIdent,
  getOrCreatePlayerIdentity, savePlayerIdentity, newPlayerIdentity,
  exportPlayerIdentity, importPlayerIdentity,
} from '../state/session.js';

const REFRESH_MS = 5000;

/**
 * 挂载大厅视图。
 * @param {{store:object, client:object, toast:Function}} ctx
 * @param {HTMLElement} rootEl
 * @returns {{unmount:()=>void}}
 */
export function mountLobby(ctx, rootEl) {
  const { client, toast } = ctx;

  /* ---- 服务器 ---- */
  const serverInput = h('input', { class: 'input', value: client.serverUrl });
  const serverStatus = h('div', { class: 'server-status muted', text: '正在探测服务器…' });

  async function probeAndUpdate(url) {
    serverStatus.textContent = `正在探测 ${url} …`;
    try {
      const http = createHttp(url);
      const health = await http.health();
      const rooms = await http.listRooms();
      serverStatus.innerHTML = '';
      serverStatus.append(h('span', {
        class: 'badge',
        text: `✅ 已连接 v${health.version} · ${rooms.length} 个房间`,
      }));
      return true;
    } catch {
      serverStatus.textContent = `未连接（${url}）。请先启动服务器，或点「扫描局域网」发现主机。`;
      return false;
    }
  }

  const useServer = async (url) => {
    client.setServerUrl(url);
    serverInput.value = url;
    const ok = await probeAndUpdate(url);
    if (ok) refreshRooms();
    return ok;
  };

  const saveBtn = h('button', {
    class: 'btn primary', text: '连接',
    onclick: async () => {
      const url = serverInput.value.trim().replace(/\/+$/, '');
      if (!/^https?:\/\//.test(url)) {
        toast('地址需以 http:// 或 https:// 开头', 'error');
        return;
      }
      if (!(await useServer(url))) toast('连接失败，请确认服务器已启动', 'error');
    },
  });

  const scanBox = h('div', { class: 'scan-box' });
  const scanBtn = h('button', {
    class: 'btn', text: '🔎 扫描局域网',
    onclick: async () => {
      scanBtn.disabled = true;
      scanBtn.textContent = '扫描中…';
      scanBox.replaceChildren(h('div', { class: 'muted', text: '正在扫描本机网段（约几秒）…' }));
      const found = await scanLan();
      scanBtn.disabled = false;
      scanBtn.textContent = '🔎 扫描局域网';

      if (found.length === 0) {
        scanBox.replaceChildren(h('div', { class: 'muted', text: '未发现大富翁服务器。确认房主已用 --bind 0.0.0.0 开服，且与本机在同一网段。' }));
        return;
      }
      scanBox.replaceChildren(...found.map((f) => h('div', { class: 'scan-row' }, [
        h('span', { class: 'mono', text: f.url.replace('http://', '') }),
        h('span', { class: 'badge', text: `v${f.version}` }),
        h('span', { class: 'grow muted', text: `${f.rooms.length} 个房间` }),
        h('button', {
          class: 'btn', text: '使用此服务器',
          onclick: async () => {
            if (await useServer(f.url)) {
              toast('已切换服务器', 'success');
              scanBox.replaceChildren();
            }
          },
        }),
      ])));
    },
  });

  const serverCard = h('div', { class: 'card' }, [
    h('div', { class: 'card-title', text: '游戏服务器' }),
    h('div', { class: 'row' }, [serverInput, saveBtn, scanBtn]),
    serverStatus,
    scanBox,
  ]);

  /* ---- 我的身份：自动保存、全房间共用，可复制/导入/重新生成 ---- */
  const identPreview = h('span', { class: 'mono muted', text: '' });
  const refreshIdentPreview = () => {
    const code = exportPlayerIdentity();
    identPreview.textContent = code ? `${code.slice(0, 28)}…` : '';
  };

  const importInput = h('input', {
    class: 'input', placeholder: '粘贴身份码导入（覆盖本机身份）',
  });
  const identityCard = h('div', { class: 'card' }, [
    h('div', { class: 'row' }, [
      h('div', { class: 'card-title grow', text: '我的身份' }),
      identPreview,
    ]),
    h('div', { class: 'muted', text: '本机自动保存，所有房间共用；换设备/隔天续玩时导出导入即可回到原角色。' }),
    h('div', { class: 'row' }, [
      h('button', {
        class: 'btn primary grow', text: '🪪 复制身份码',
        onclick: async () => {
          if (await copyText(exportPlayerIdentity())) toast('身份码已复制', 'success');
        },
      }),
      h('button', {
        class: 'btn grow', text: '重新生成',
        title: '生成全新身份；旧身份码作废，未结束房间里的旧座位将无法认领',
        onclick: () => {
          if (!confirm('确定生成全新身份吗？旧身份码将作废。')) return;
          savePlayerIdentity(newPlayerIdentity());
          refreshIdentPreview();
          toast('已生成新身份', 'success');
        },
      }),
    ]),
    h('div', { class: 'join-row' }, [
      importInput,
      h('button', {
        class: 'btn', text: '导入',
        onclick: () => {
          const pid = importPlayerIdentity(importInput.value);
          if (!pid) {
            toast('身份码格式不正确', 'error');
            return;
          }
          savePlayerIdentity(pid);
          importInput.value = '';
          refreshIdentPreview();
          toast('身份已导入', 'success');
        },
      }),
    ]),
  ]);
  refreshIdentPreview();

  /* ---- 建房 ---- */
  const numField = (label, value, min, max) => {
    const input = h('input', { class: 'input', type: 'number', value, min, max });
    return [label, input];
  };
  const [nameLabel, nameInput] = ['房间名', h('input', { class: 'input', placeholder: '不填则显示房号', maxlength: '30' })];
  const [maxLabel, maxInput] = numField('人数上限', 8, 2, 8);
  const [moneyLabel, moneyInput] = numField('初始资金', 15000000, 1, 100000000);
  const [diceLabel, diceInput] = numField('骰子数量', 2, 1, 3);
  const [salaryLabel, salaryInput] = numField('GO 薪水', 2000000, 1, 10000000);
  const [roundsLabel, roundsInput] = numField('限时回合(空=不限)', 0, 0, 9999);
  const sizeSelect = h('select', { class: 'input' }, [
    h('option', { value: '56', text: '56 格 · 扩展世界版（推荐）' }),
    h('option', { value: '40', text: '40 格 · 经典世界版' }),
  ]);

  const createBtn = h('button', {
    class: 'btn primary block span2', text: '🏠 创建房间',
    onclick: async () => {
      const rounds = Math.floor(Number(roundsInput.value)) || 0;
      createBtn.disabled = true;
      createBtn.textContent = '正在确认服务器…';
      try {
        // Electron 宿主下，目标为本机且服务器未在线 → 自动拉起服务器进程
        const host = await ensureLocalServer(client.serverUrl);
        if (host && !host.ok) { toast(host.error, 'error'); return; }
        if (host?.started) {
          toast('已自动启动本机服务器', 'success');
          probeAndUpdate(client.serverUrl); // 同步刷新连接徽章
        }
        const res = await client.createRoom({
          player_count_max: Number(maxInput.value),
          initial_money: Number(moneyInput.value),
          dice_count: Number(diceInput.value),
          go_salary: Number(salaryInput.value),
          board_size: Number(sizeSelect.value),
          max_rounds: rounds > 0 ? rounds : null,
        }, nameInput.value.trim());
        await enterRoom(Number(res.room_code), {
          ownerToken: res.owner_token,
          isOwner: true,
        });
      } catch (e) { toast(e.message, 'error'); }
      finally {
        createBtn.disabled = false;
        createBtn.textContent = '🏠 创建房间';
      }
    },
  });

  const createCard = h('div', { class: 'card' }, [
    h('div', { class: 'card-title', text: '创建房间' }),
    h('div', { class: 'lobby-form' }, [
      h('div', { class: 'form-item span2' }, [h('label', { text: nameLabel }), nameInput]),
      h('div', { class: 'form-item' }, [h('label', { text: maxLabel }), maxInput]),
      h('div', { class: 'form-item' }, [h('label', { text: moneyLabel }), moneyInput]),
      h('div', { class: 'form-item' }, [h('label', { text: '棋盘格数' }), sizeSelect]),
      h('div', { class: 'form-item' }, [h('label', { text: diceLabel }), diceInput]),
      h('div', { class: 'form-item' }, [h('label', { text: salaryLabel }), salaryInput]),
      h('div', { class: 'form-item' }, [h('label', { text: roundsLabel }), roundsInput]),
      createBtn,
    ]),
  ]);

  /* ---- 加入 ---- */
  const codeInput = h('input', {
    class: 'input', placeholder: '8 位房号', maxlength: '8', inputmode: 'numeric',
  });
  const joinCard = h('div', { class: 'card' }, [
    h('div', { class: 'card-title', text: '加入房间' }),
    h('div', { class: 'join-row' }, [
      codeInput,
      h('button', {
        class: 'btn primary', text: '加入',
        onclick: async () => {
          const code = Number(codeInput.value);
          if (!Number.isInteger(code) || code < 1000000) {
            toast('请输入 8 位数字房号', 'error');
            return;
          }
          await enterRoom(code, { isOwner: false });
        },
      }),
    ]),
  ]);

  /* ---- 房间列表 ---- */
  const listBox = h('div', { class: 'room-list' });
  const listCard = h('div', { class: 'card' }, [
    h('div', { class: 'row' }, [
      h('div', { class: 'card-title grow', text: '本服房间' }),
      h('button', { class: 'btn', text: '刷新', onclick: refreshRooms }),
    ]),
    listBox,
  ]);

  /* ---- 组装 ---- */
  const rejoinSlot = h('div', {});
  const page = h('div', { class: 'lobby' }, [
    h('div', { class: 'lobby-inner' }, [
      h('div', { class: 'lobby-hero' }, [
        h('h1', { text: '大富翁' }),
        h('div', {}, [h('span', { class: 'hero-sub', text: 'MONOPOLY · 世界版' })]),
        h('p', { text: '自动连接服务器，建房开战或加入好友房间' }),
      ]),
      rejoinSlot,
      identityCard,
      serverCard,
      h('div', { class: 'lobby-grid' }, [createCard, h('div', { class: 'col' }, [joinCard, listCard])]),
    ]),
  ]);
  rootEl.replaceChildren(page);

  const timer = setInterval(refreshRooms, REFRESH_MS);
  useServer(client.serverUrl); // 启动即自动探测，成功则拉取房间列表
  checkRejoin(); // 有历史身份且房间仍在 → 显示一键回到房间

  /* ---- 内部 ---- */

  /** 有未过期的房间身份时，在大厅顶部提供「回到房间」入口（保留原角色/房主权）。 */
  async function checkRejoin() {
    const saved = loadIdent();
    if (!saved) return;
    try {
      const snap = await client.http.snapshot(saved.roomCode);
      const name = snap.players.find((p) => p.id === saved.playerId)?.name ?? '原角色';
      rejoinSlot.replaceChildren(h('div', { class: 'card rejoin-banner' }, [
        h('span', { class: 'grow' }, [
          h('b', { text: `↩ 回到房间 ${fmtCode(saved.roomCode)}` }),
          h('span', { class: 'muted', text: `　你的角色：${name}${saved.isOwner ? '（房主）' : ''}` }),
        ]),
        h('button', {
          class: 'btn primary', text: '回到房间',
          onclick: () => {
            if (client.resumeRoom()) {
              ctx.store.enterRoom(saved.roomCode, {
                playerId: saved.playerId,
                isOwner: saved.isOwner,
                ownerToken: saved.ownerToken,
              });
            }
          },
        }),
        h('button', {
          class: 'btn', text: '放弃身份',
          title: '丢弃本机保存的入场令牌；房间内的角色将转为托管',
          onclick: () => {
            clearIdent();
            rejoinSlot.replaceChildren();
          },
        }),
      ]));
    } catch {
      clearIdent(); // 房间已不存在，清理过期身份
    }
  }

  async function enterRoom(code, { isOwner, ownerToken = '' }) {
    try {
      // 自动携带全局身份：同身份再次加入任意房间 → 服务器认领原座位
      const pid = getOrCreatePlayerIdentity();
      const res = await client.joinRoom(code, pid);
      // 回写服务器确认生效的全局身份（服务器代生成/先到先得登记时与本地一致）
      savePlayerIdentity({ uid: res.player_uid, secret: res.secret });
      saveIdent({
        roomCode: code,
        playerId: res.player_id,
        token: res.player_token,
        isOwner,
        ownerToken,
      });
      client.enterRoom(code, {
        playerId: res.player_id,
        token: res.player_token,
        isOwner,
        ownerToken,
      });
      ctx.store.enterRoom(code, { playerId: res.player_id, isOwner, ownerToken });
      // 视图切换由路由根据 store.screen 完成
    } catch (e) { toast(e.message, 'error'); }
  }

  async function refreshRooms() {
    let rooms;
    try {
      rooms = await client.listRooms();
    } catch {
      listBox.replaceChildren(h('div', { class: 'empty', text: '无法获取房间列表（服务器未连接）' }));
      return;
    }
    if (rooms.length === 0) {
      listBox.replaceChildren(h('div', { class: 'empty', text: '暂无房间，创建一个吧' }));
      return;
    }
    listBox.replaceChildren(...rooms.map((r) => h('div', { class: 'room-row' }, [
      h('span', { class: 'code', text: fmtCode(r.code) }),
      h('span', { class: 'grow muted', text: r.name || '（未命名房间）' }),
      h('span', { class: 'badge', text: STATUS_LABELS[r.status] ?? r.status }),
      h('span', { class: 'muted', text: `${r.players}/${r.player_count_max} 人` }),
      h('button', {
        class: 'btn', text: '加入',
        onclick: () => enterRoom(r.code, { isOwner: false }),
      }),
    ])));
  }

  return {
    unmount() {
      clearInterval(timer);
    },
  };
}
