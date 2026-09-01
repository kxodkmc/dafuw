//! 账单流水弹窗：本局所有资金进出（随 MoneyChanged 事件实时累计）。
//!
//! 支持只看自己过滤；打开期间订阅 store 实时刷新，关闭即停止。

import { h, fmtMoney } from './dom.js';
import { openModal } from './modal.js';
import { playerName } from '../state/store.js';
import { COLORS } from '../protocol/types.js';

const hexOf = (id) => COLORS.find((c) => c.id === id)?.hex ?? '#999';

/**
 * 打开账单流水弹窗。
 * @param {{store:object}} ctx
 */
export function openLedger(ctx) {
  const { state } = ctx.store;
  let mineOnly = false;

  const listEl = h('div', { class: 'ledger-list' });
  const sumEl = h('div', { class: 'ledger-sum' });
  const body = h('div', { class: 'col ledger' }, [
    h('div', { class: 'row', style: 'gap:8px' }, [
      h('button', {
        class: 'btn', text: '只看我',
        onclick: (e) => {
          mineOnly = !mineOnly;
          e.target.classList.toggle('primary', mineOnly);
          renderList();
        },
      }),
      sumEl,
    ]),
    listEl,
  ]);

  const modal = openModal({ title: '📒 账单流水' }, body);

  function entries() {
    const meId = state.me?.playerId;
    const all = [...state.ledger].reverse(); // 最新在上
    return mineOnly && meId ? all.filter((e) => e.playerId === meId) : all;
  }

  function renderList() {
    const rows = entries().slice(0, 200);
    if (!rows.length) {
      listEl.replaceChildren(h('div', { class: 'muted', text: '暂无流水记录' }));
      return;
    }
    listEl.replaceChildren(
      ...rows.map((e) => {
        const pos = e.delta >= 0;
        const time = new Date(e.ts).toTimeString().slice(0, 5);
        return h('div', { class: 'ledger-row' }, [
          h('span', { class: 'muted mono', text: time }),
          h('span', {
            class: 'ledger-who',
            style: `border-color:${hexOf(colorOf(e.playerId))}`,
            text: playerName(state, e.playerId) ?? '？',
          }),
          h('span', { class: 'ledger-why', text: e.reason || '资金变动' }),
          h('span', { class: pos ? 'ledger-amt pos' : 'ledger-amt neg', text: `${pos ? '+' : '-'}${fmtMoney(Math.abs(e.delta))}` }),
        ]);
      }),
    );
    const meId = state.me?.playerId;
    const net = state.ledger
      .filter((e) => !mineOnly || e.playerId === meId)
      .reduce((s, e) => s + e.delta, 0);
    sumEl.replaceChildren(
      h('span', { class: 'muted', text: `净额 ` }),
      h('b', { class: net >= 0 ? 'pos' : 'neg', text: `${net >= 0 ? '+' : '-'}${fmtMoney(Math.abs(net))}` }),
    );
  }

  function colorOf(pid) {
    return state.snapshot?.players.find((p) => p.id === pid)?.color;
  }

  renderList();
  // 实时刷新：弹窗关闭后停止订阅
  const unsub = ctx.store.subscribe(() => {
    if (!document.body.contains(listEl)) {
      unsub();
      return;
    }
    renderList();
  });
}
