//! 地产弹窗：地块详情 + 我的地产卡片总览。
//! 机会、命运、税收等功能格不弹房产信息。

import { h, fmtMoney } from './dom.js';
import { openModal } from './modal.js';
import { cmd, GROUP_COLORS } from '../protocol/types.js';
import { kindLabel, tileOwner, myOwnTiles } from '../state/selectors.js';

const BUYABLE_KINDS = new Set(['Property', 'Railroad', 'Utility']);

/** 火车站租金表（1~4 站，与引擎 RAILROAD_RENTS 一致）。 */
const RAILROAD_RENTS = [250_000, 500_000, 1_000_000, 2_000_000];

/** 当前实际租金（与引擎 deeds::rent 同规则；垄断/电站按整组持有数判断）。 */
function currentRent(state, tile, diceSum = 0) {
  if (!tile.owner) return 0;
  if (tile.kind === 'Railroad') {
    const n = state.snapshot.tiles.filter(
      (t) => t.kind === 'Railroad' && t.owner === tile.owner).length;
    return RAILROAD_RENTS[Math.min(n, 4) - 1];
  }
  if (tile.kind === 'Utility') {
    const n = state.snapshot.tiles.filter(
      (t) => t.kind === 'Utility' && t.owner === tile.owner).length;
    return diceSum * (n >= 2 ? 100_000 : 40_000);
  }
  if (tile.houses >= 5) return tile.hotel_rent;
  if (tile.houses > 0) return tile.rent_with_house[tile.houses - 1];
  const monopoly = state.snapshot.tiles
    .filter((t) => t.group === tile.group)
    .every((t) => t.owner === tile.owner);
  return monopoly ? tile.base_rent * 2 : tile.base_rent;
}

/** 当前过路费的原因说明（仅地产；与 currentRent 同口径）。 */
function currentRentLabel(tile, monopoly) {
  if (tile.houses >= 5) return '当前过路费 · 旅馆';
  if (tile.houses > 0) return `当前过路费 · ${tile.houses} 栋房屋`;
  return monopoly ? '当前过路费 · 整套垄断 ×2' : '当前过路费';
}

/** 过路费表：仅租金梯度；有主时首行高亮当前过路费。盖房成本/抵押归信息表。 */
function rentTable(state, tile) {
  const rows = [];
  let cur = null;
  const owned = !!tile.owner;

  if (tile.kind === 'Property') {
    const monopoly = state.snapshot.tiles
      .filter((t) => t.group === tile.group)
      .every((t) => t.owner === tile.owner);
    rows.push(
      ['空地', tile.base_rent],
      ['空地 · 整套垄断 ×2', tile.base_rent * 2],
      ...tile.rent_with_house.map((r, i) => [`${i + 1} 栋房屋`, r]),
      ['旅馆', tile.hotel_rent],
    );
    if (owned) cur = [currentRentLabel(tile, monopoly), currentRent(state, tile)];
  } else if (tile.kind === 'Railroad') {
    RAILROAD_RENTS.forEach((r, i) => rows.push([`${i + 1} 站连锁`, r]));
    if (owned) {
      const n = state.snapshot.tiles.filter(
        (t) => t.kind === 'Railroad' && t.owner === tile.owner).length;
      cur = [`当前过路费 · ${n} 站连锁`, currentRent(state, tile)];
    }
  } else if (tile.kind === 'Utility') {
    rows.push(
      ['持有 1 座', '骰点 × $40K'],
      ['持有 2 座', '骰点 × $100K'],
    );
  }

  return h('table', { class: 'detail-table rent-table' }, [
    h('tr', {}, [h('th', { text: '过路费（停留在该格时支付）', colspan: '2' })]),
    cur ? h('tr', { class: 'cur' }, [
      h('td', { text: cur[0] }),
      h('td', { text: fmtMoney(cur[1]) }),
    ]) : null,
    ...rows.map(([k, v]) =>
      h('tr', {}, [h('td', { text: k }), h('td', { text: typeof v === 'number' ? fmtMoney(v) : v })])),
  ]);
}

/**
 * @param {{store:object, client:object, toast:Function}} ctx
 * @param {number} tileId
 */
export function openTileDetail(ctx, tileId) {
  const { state } = ctx.store;
  const tile = state.snapshot?.tiles[tileId];
  if (!tile || !BUYABLE_KINDS.has(tile.kind)) return; // 功能格：无房产信息可看
  const mine = tile.owner && tile.owner === state.me?.playerId;
  // 引擎不限制回合：官方规则允许任何时候盖房/抵押，落地后即使轮次流转也可操作
  const owner = tileOwner(state, tile);

  const act = (label, make) => h('button', {
    class: 'btn',
    text: label,
    onclick: () => {
      try {
        ctx.client.send(make());
        modal.close();
      } catch (e) { ctx.toast(e.message, 'error'); }
    },
  });

  const ops = mine
    ? h('div', { class: 'row' }, [
        act('🏠 盖房', () => cmd.buildHouse(tileId)),
        act('🏚️ 卖房', () => cmd.sellHouse(tileId)),
        act('🏦 抵押', () => cmd.mortgage(tileId)),
        act('💵 赎回', () => cmd.unmortgage(tileId)),
      ])
    : h('div', { class: 'muted', text: '并非你的地产，不可操作' });

  const body = h('div', { class: 'col' }, [
    h('div', { class: 'row', style: 'align-items:flex-start;gap:18px' }, [
      h('table', { class: 'detail-table' }, [
        row('名称', tile.name),
        row('类型', kindLabel(tile.kind)),
        row('色组', tile.group ?? '—'),
        row('价格', tile.price > 0 ? fmtMoney(tile.price) : '—'),
        row('所有者', owner ? `${owner.name}` : '无主'),
        row('建筑', tile.houses >= 5 ? '旅馆' : `${tile.houses} 栋房屋`),
        tile.kind === 'Property'
          ? row('盖房成本', `${fmtMoney(tile.building_cost)} / 栋`)
          : null,
        row('抵押状态', tile.mortgaged ? '已抵押' : '正常'),
        tile.price > 0 ? row('抵押可得', fmtMoney(tile.mortgage_value)) : null,
      ]),
      rentTable(state, tile),
    ]),
    ops,
  ]);

  const modal = openModal({ title: `地块详情 · 格 ${tileId}` }, body);
}

/**
 * 我的地产卡片总览：快速浏览名下所有房产，点击卡片查看地块详情。
 * @param {{store:object, client:object, toast:Function}} ctx
 */
export function openDeedCards(ctx) {
  const { state } = ctx.store;
  const tiles = myOwnTiles(state);

  const cards = tiles.map((tile) => {
    const hex = GROUP_COLORS[tile.group] ?? '#8d6e63';
    return h('div', {
      class: 'deed-card',
      onclick: () => {
        modal.close();
        openTileDetail(ctx, tile.id);
      },
    }, [
      h('div', { class: 'deed-head', style: `background:${hex}`, text: tile.name }),
      h('div', { class: 'deed-body' }, [
        h('div', { text: `💰 ${fmtMoney(tile.price)}` }),
        h('div', { text: `🧾 租金 ${fmtMoney(currentRent(state, tile))}` }),
        tile.kind === 'Property'
          ? h('div', { text: `🏠 盖房 ${fmtMoney(tile.building_cost)} / 栋` })
          : null,
        h('div', {
          text: tile.houses >= 5 ? '🏨 旅馆' : tile.houses > 0 ? `🏠 ×${tile.houses}` : '🟩 空地',
        }),
        tile.mortgaged ? h('div', { class: 'deed-mort', text: '已抵押' }) : null,
      ]),
    ]);
  });

  const body = h('div', { class: 'col' }, [
    tiles.length
      ? h('div', { class: 'deed-grid' }, cards)
      : h('div', { class: 'muted', text: '名下还没有地产：轮到你时购地，或参与拍卖捡漏。' }),
    tiles.length ? h('div', { class: 'muted', style: 'font-size:12px', text: '点击卡片可盖房 / 抵押等操作。' }) : null,
  ]);

  const modal = openModal({ title: `我的地产（${tiles.length}）` }, body);
}

function row(k, v) {
  return h('tr', {}, [h('td', { text: k }), h('td', { text: v })]);
}
