//! 拍卖弹窗：拍卖期间对所有玩家同步可见。
//!
//! 明确轮次：轮到自己 → 出价输入可用；否则展示「等待 XX 出价」或「已弃权/最高价」锁定态。
//! 弹窗随快照同步（state.auction 由 RoomSnapshot 权威重建），重绘时保留输入焦点与数值。

import { h, fmtMoney } from './dom.js';
import { openModal } from './modal.js';
import { playerName, tileName } from '../state/store.js';
import { myPlayer } from '../state/selectors.js';
import { cmd, COLORS } from '../protocol/types.js';

let handle = null;
let bodyEl = null;

/**
 * 每个渲染周期调用：按 state.auction 同步弹窗开关与内容。
 * @param {{client:object}} ctx
 * @param {object} state store.state
 */
export function syncAuction(ctx, state) {
  const a = state.auction;
  if (!a) {
    handle?.close();
    handle = null;
    bodyEl = null;
    return;
  }
  if (!handle) {
    bodyEl = h('div', { class: 'auction-body' });
    handle = openModal({ title: `🔨 拍卖：${tileName(state, a.tileId)}`, closable: false }, bodyEl);
  }
  renderBody(bodyEl, ctx, state, a);
}

function renderBody(mountEl, ctx, state, a) {
  const me = myPlayer(state);
  const myTurn = !!me && a.turn === me.id;
  const iPassed = !!me && (a.passed ?? []).includes(me.id);
  const iAmHighest = !!me && a.highestBy === me.id;

  // 重建时保留输入值与焦点，避免打字被打断
  const prevInput = mountEl.querySelector('.a-bid-input');
  const keep = prevInput
    ? { value: prevInput.value, focused: document.activeElement === prevInput }
    : null;

  const input = h('input', {
    class: 'input a-bid-input',
    type: 'number',
    min: '1',
    placeholder: '出价金额',
    value: keep?.value ?? '',
  });

  const bid = () => {
    const amount = Math.floor(Number(input.value));
    if (!Number.isFinite(amount) || amount <= 0) return;
    try {
      ctx.client.send(cmd.auctionBid(amount));
      input.value = '';
    } catch { /* 断线提示由上层 toast 统一展示 */ }
  };
  const step = (n) => () => {
    const base = Math.max(Number(input.value) || 0, a.highest);
    input.value = String(base + n);
  };

  // 轮次提示：轮到我 / 等待他人 / 已弃权 / 我最高
  let status;
  if (iPassed) {
    status = h('div', { class: 'a-status wait', text: '你已弃权，等待拍卖结束' });
  } else if (myTurn) {
    status = h('div', { class: 'a-status', text: '🎯 轮到你出价！' });
  } else if (iAmHighest) {
    status = h('div', { class: 'a-status wait', text: '你目前是最高价，等待其他玩家' });
  } else {
    status = h('div', { class: 'a-status wait',
      text: a.turn ? `等待 ${playerName(state, a.turn)} 出价…` : '等待出价…' });
  }

  const hexOf = (id) => COLORS.find((c) => c.id === id)?.hex ?? '#999';
  const chips = (state.snapshot?.players ?? [])
    .filter((p) => !p.bankrupt)
    .map((p) => {
      const cls = (a.passed ?? []).includes(p.id) ? 'out'
        : a.highestBy === p.id ? 'top'
          : a.turn === p.id ? 'turn' : '';
      const mark = cls === 'out' ? ' ✗' : cls === 'top' ? ' 🏆' : cls === 'turn' ? ' ▶' : '';
      return h('span', {
        class: `a-chip ${cls}`,
        style: `border-color:${hexOf(p.color)}`,
        text: `${p.name}${mark}`,
      });
    });

  mountEl.replaceChildren(
    ...[status,
      h('div', { class: 'a-high' },
        a.highestBy
          ? [`当前最高：`, h('b', { text: fmtMoney(a.highest) }), `（${playerName(state, a.highestBy)}）`]
          : '暂无人出价，价高者得'),
      h('div', { class: 'a-order' }, chips),
      myTurn
        ? [
            h('div', { class: 'a-ops' }, [
              input,
              h('button', { class: 'btn', text: '+10', onclick: step(10) }),
              h('button', { class: 'btn', text: '+50', onclick: step(50) }),
              h('button', { class: 'btn primary', text: '出价', onclick: bid }),
            ]),
            h('button', {
              class: 'btn block', text: '放弃竞拍（不可恢复）',
              onclick: () => { try { ctx.client.send(cmd.auctionPass()); } catch { /* 上层提示 */ } },
            }),
          ]
        : null,
    ].flat().filter(Boolean));

  if (myTurn && keep?.focused) {
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  }
}
