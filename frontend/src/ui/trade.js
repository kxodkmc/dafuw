//! 交易：发起交易弹窗 + 收到交易的监控弹窗。
//! 交易筹码 = 现金 + 地块 + 出狱卡（AssetList）。

import { h, fmtMoney } from './dom.js';
import { openModal } from './modal.js';
import { cmd, emptyAssets } from '../protocol/types.js';
import { myPlayer, tilesOf } from '../state/selectors.js';
import { tileName } from '../state/store.js';

/**
 * 打开「发起交易」弹窗。
 * @param {{store:object, client:object, toast:Function}} ctx
 */
export function openTradeModal(ctx) {
  const { state } = ctx.store;
  const me = myPlayer(state);
  if (!me) return;

  const targets = state.snapshot.players.filter(
    (p) => p.id !== me.id && !p.bankrupt,
  );
  if (targets.length === 0) {
    ctx.toast('当前没有可交易的对象', 'error');
    return;
  }

  // --- 我方给出（offer） ---
  const offerCash = h('input', { class: 'input', type: 'number', min: '0', value: '0' });
  const offerCards = h('input', {
    class: 'input', type: 'number', min: '0', max: String(me.out_of_jail_cards), value: '0',
  });
  const offerTiles = tilePicker(state, me.id, new Set());

  // --- 我方要求（demand），随目标重建 ---
  const demandCash = h('input', { class: 'input', type: 'number', min: '0', value: '0' });
  const demandCards = h('input', { class: 'input', type: 'number', min: '0', value: '0' });
  const demandTilesBox = h('div', {});
  let demandTiles = null;

  const targetSelect = h('select', { class: 'input' },
    targets.map((p) => h('option', { value: p.id, text: p.name })));

  function rebuildDemand() {
    demandTiles = tilePicker(state, targetSelect.value);
    demandTilesBox.replaceChildren(h('div', {}, [...demandTiles.checkboxes]));
  }
  targetSelect.addEventListener('change', rebuildDemand);
  rebuildDemand();

  const readAssets = (cashEl, cardEl, picker) => ({
    ...emptyAssets(),
    cash: Math.max(0, Math.floor(Number(cashEl.value) || 0)),
    out_of_jail_cards: Math.max(0, Math.floor(Number(cardEl.value) || 0)),
    tiles: [...picker.checkedIds()],
  });

  const submit = () => {
    const offer = readAssets(offerCash, offerCards, offerTiles);
    const demand = readAssets(demandCash, demandCards, demandTiles);
    const empty = (a) => a.cash === 0 && a.tiles.length === 0 && a.out_of_jail_cards === 0;
    if (empty(offer) && empty(demand)) {
      ctx.toast('交易筹码不能为空', 'error');
      return;
    }
    try {
      ctx.client.send(cmd.proposeTrade(targetSelect.value, offer, demand));
      ctx.toast('交易提议已发送', 'success');
      modal.close();
    } catch (e) { ctx.toast(e.message, 'error'); }
  };

  const body = h('div', {}, [
    h('div', { class: 'form-item' }, [h('label', { text: '交易对象' }), targetSelect]),
    h('div', { class: 'trade-grid' }, [
      h('div', { class: 'trade-col' }, [
        h('h4', { text: '我方给出' }),
        numRow('现金', offerCash), numRow('出狱卡', offerCards),
        h('div', { class: 'trade-tiles' }, [...offerTiles.checkboxes]),
      ]),
      h('div', { class: 'trade-col' }, [
        h('h4', { text: '我方要求' }),
        numRow('现金', demandCash), numRow('出狱卡', demandCards),
        h('div', { class: 'trade-tiles' }, demandTilesBox),
      ]),
    ]),
    h('div', { class: 'row', style: 'margin-top:14px;justify-content:flex-end' }, [
      h('button', { class: 'btn primary', text: '发送提议', onclick: submit }),
    ]),
  ]);

  const modal = openModal({ title: '发起交易' }, body);
}

function numRow(label, input) {
  return h('div', { class: 'trade-num' }, [h('span', { text: label }), input]);
}

/** 地块勾选组：固定快照集合，暴露 checkedIds()。 */
function tilePicker(state, ownerId) {
  const tiles = tilesOf(state, ownerId);
  const checked = new Set();
  const checkboxes = tiles.map((t) => {
    const box = h('input', {
      type: 'checkbox',
      onchange: () => (box.checked ? checked.add(t.id) : checked.delete(t.id)),
    });
    return h('label', {}, [box, `格${t.id} ${t.name}${t.mortgaged ? '（抵押中）' : ''}`]);
  });
  return { checkboxes, checkedIds: () => checked };
}

/**
 * 监控收到的交易提议，逐个弹窗待处理。
 * @returns {()=>void} 每次状态更新后调用
 */
export function makeTradeWatcher(ctx) {
  const shown = new Set();

  return () => {
    const meId = ctx.store.state.me?.playerId;
    for (const t of ctx.store.state.trades) {
      if (t.to !== meId || shown.has(t.trade_id)) continue;
      shown.add(t.trade_id);
      openIncomingTrade(ctx, t);
    }
  };
}

function openIncomingTrade(ctx, t) {
  const { state } = ctx.store;
  const fromName = state.snapshot?.players.find((p) => p.id === t.from)?.name ?? '玩家';

  const decide = (make) => () => {
    try {
      ctx.client.send(make());
      modal.close();
    } catch (e) { ctx.toast(e.message, 'error'); }
  };

  const body = h('div', {}, [
    h('div', { class: 'trade-grid' }, [
      h('div', { class: 'trade-col' }, [
        h('h4', { text: '对方给出' }),
        h('div', { text: fmtAssets(state, t.offer) }),
      ]),
      h('div', { class: 'trade-col' }, [
        h('h4', { text: '对方要求' }),
        h('div', { text: fmtAssets(state, t.demand) }),
      ]),
    ]),
    h('div', { class: 'row', style: 'margin-top:14px;justify-content:flex-end' }, [
      h('button', { class: 'btn danger', text: '拒绝', onclick: decide(() => cmd.declineTrade(t.trade_id)) }),
      h('button', { class: 'btn primary', text: '接受', onclick: decide(() => cmd.acceptTrade(t.trade_id)) }),
    ]),
  ]);

  const modal = openModal({ title: `来自 ${fromName} 的交易` }, body);
}

function fmtAssets(state, a) {
  const parts = [];
  if (a.cash) parts.push(fmtMoney(a.cash));
  if (a.out_of_jail_cards) parts.push(`出狱卡 ×${a.out_of_jail_cards}`);
  for (const id of a.tiles) parts.push(`格${id} ${tileName(state, id)}`);
  return parts.length ? parts.join('、') : '（无）';
}
