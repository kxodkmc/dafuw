//! 房间页：顶栏（房间信息/房主操作/离开） + 棋盘主区 + 侧栏（玩家/行动/日志）。
//! 自身订阅 store 全量重绘；子组件各自保持轻量重建。

import { h, fmtMoney, fmtCode, copyText } from './dom.js';
import { cmd, STATUS_LABELS, PHASE_LABELS } from '../protocol/types.js';
import { isOwner, myAssets } from '../state/selectors.js';
import { exportPlayerIdentity } from '../state/session.js';
import { openModal } from './modal.js';
import { renderBoard } from './board.js';
import { syncAuction } from './auction.js';
import { renderPlayers } from './players.js';
import { renderActions } from './actions.js';
import { renderLog } from './log.js';
import { openTradeModal, makeTradeWatcher } from './trade.js';
import { openTileDetail } from './tile-detail.js';

/**
 * 挂载房间视图。
 * @param {{store:object, client:object, toast:Function, exitRoom:()=>void}} ctx
 * @param {HTMLElement} rootEl
 * @returns {{unmount:()=>void}}
 */
export function mountRoom(ctx, rootEl) {
  const { store, client, toast } = ctx;
  const checkTrades = makeTradeWatcher(ctx);

  /* ---- 结构 ---- */
  const boardMount = h('div', { class: 'board-cell' });
  const playersMount = h('div', { class: 'players-body' });
  const actionsMount = h('div', {});
  const logMount = h('div', { class: 'card log-card' });
  const topbar = h('div', { class: 'topbar' });

  const page = h('div', { class: 'room' }, [
    topbar,
    h('div', { class: 'room-main' }, [
      h('div', { class: 'log-col' }, [logMount]),
      boardMount,
      h('div', { class: 'sidebar' }, [
        h('div', { class: 'card players-card' }, [playersMount]),
        h('div', { class: 'card' }, [actionsMount]),
      ]),
    ]),
  ]);
  rootEl.replaceChildren(page);

  const render = () => {
    const s = store.state;
    renderTopbar(s);
    renderBoard(boardMount, s, {
      client,
      onTileClick: (id) => openTileDetail(ctx, id),
    });
    syncAuction({ client }, s); // 拍卖弹窗：全员可见，随快照同步
    renderPlayers(playersMount, s);
    renderActions(actionsMount, s, ctxWithTrade);
    renderLog(s.log, logMount, {
      onSend: (text) => {
        try {
          client.send(cmd.chat(text));
        } catch (e) { toast(e.message, 'error'); }
      },
    });
    checkTrades();
  };

  // 行动面板的 ctx 需要额外的 openTrade 入口
  const ctxWithTrade = { ...ctx, openTrade: () => openTradeModal(ctx) };

  const unsub = store.subscribe(render);
  render();

  return {
    unmount() {
      unsub();
      syncAuction({ client }, { auction: null }); // 离开房间时收起拍卖弹窗
      boardMount._fitBoard?.disconnect();
    },
  };

  /* ---- 内部 ---- */

  function renderTopbar(s) {
    const snap = s.snapshot;
    const owner = isOwner(s);
    const inLobby = snap?.status === 'lobby';

    topbar.replaceChildren(
      ...[
        h('span', { class: 'title', text: `房间 ${fmtCode(s.roomCode ?? 0)}` }),
        snap ? h('span', { class: 'badge', text: STATUS_LABELS[snap.status] ?? snap.status }) : null,
        snap?.status === 'playing'
          ? h('span', { class: 'muted', text: `第 ${snap.round} 轮 · ${PHASE_LABELS[snap.phase] ?? snap.phase}` })
          : null,
        meHud(s),
        h('span', { class: 'grow' }),
        h('span', { class: `conn-dot ${s.connected ? 'on' : 'off'}`, title: s.connected ? '已连接' : '连接断开，重连中' }),
        h('span', { class: 'muted', text: s.connected ? '已连接' : '重连中…' }),
        client.ident
          ? h('button', {
              class: 'btn', text: '🪪 身份码',
              title: '复制后可在任何设备恢复本角色继续对局',
              onclick: showIdentityModal,
            })
          : null,
        owner && inLobby
          ? h('button', {
              class: 'btn primary', text: '▶ 开局',
              onclick: async () => {
                try { await client.startGame(); } catch (e) { toast(e.message, 'error'); }
              },
            })
          : null,
        owner
          ? h('button', {
              class: 'btn danger', text: '解散房间',
              onclick: async () => {
                try { await client.closeRoom(); } catch (e) { toast(e.message, 'error'); }
              },
            })
          : null,
        h('button', { class: 'btn', text: '🚪 离开', onclick: () => ctx.exitRoom() }),
      ].filter(Boolean),
    );
  }

  /** 左上角我的资产：现金 / 房产 / 可抵押，一眼分清资金结构。 */
  function meHud(s) {
    const a = myAssets(s);
    if (!a) return null;
    const item = (k, cls, v) =>
      h('span', { class: 'me-item' }, [
        h('span', { class: 'k', text: k }),
        h('b', { class: `v ${cls}`, text: fmtMoney(v) }),
      ]);
    return h('div', {
      class: 'me-hud',
      title: '现金可随时动用；房产为未抵押地价合计；可抵押为抵押后可获得的资金',
    }, [
      item('现金', 'cash', a.cash),
      h('span', { class: 'me-sep', text: '·' }),
      item('房产', 'prop', a.property),
      h('span', { class: 'me-sep', text: '·' }),
      item('可抵押', 'mort', a.mortgagable),
    ]);
  }

  /** 身份码弹窗：展示 + 复制。全局身份全房间共用，跨设备/隔天导入即回原角色。 */
  function showIdentityModal() {
    const code = exportPlayerIdentity();
    if (!code) return;
    const box = h('textarea', {
      class: 'input mono', readonly: true, rows: '4',
      style: 'width:100%;resize:none;word-break:break-all;font-size:12px',
    });
    box.value = code;
    const body = h('div', { class: 'col' }, [
      h('div', { class: 'muted', text: '这是你的唯一身份码（所有房间共用）。换设备或隔天续玩时，在大厅「我的身份」导入即可回到原角色。请妥善保管，等同账号。' }),
      box,
      h('button', {
        class: 'btn primary block',
        text: '复制身份码',
        onclick: async () => {
          box.select();
          if (await copyText(code)) toast('身份码已复制', 'success');
        },
      }),
    ]);
    openModal({ title: '我的身份码' }, body);
  }
}
