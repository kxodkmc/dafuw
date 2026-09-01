//! 行动面板：按房间阶段提供上下文操作（资料/准备、掷骰、购买、出狱、交易）。

import { h, fmtMoney } from './dom.js';
import { AVATARS, COLORS, cmd, STATUS_LABELS } from '../protocol/types.js';
import { myPlayer, canRoll, canDecide, isLandedTileBuyable, isOwner } from '../state/selectors.js';
import { openDeedCards } from './tile-detail.js';
import { openLedger } from './ledger.js';

/**
 * 全量重绘行动区。草稿态（资料表单）挂在 mountEl 上，重建时保留。
 * @param {HTMLElement} mountEl
 * @param {object} state store.state
 * @param {{client:object, toast:(m:string,t?:string)=>void, openTrade:()=>void}} ctx
 */
export function renderActions(mountEl, state, ctx) {
  const { snapshot } = state;
  mountEl.replaceChildren(
    h('div', { class: 'card-title', text: '行动' }),
  );

  if (!snapshot) {
    mountEl.append(h('div', { class: 'muted', text: '等待房间数据…' }));
    return;
  }

  if (snapshot.status === 'lobby') {
    mountEl.append(h('div', { class: 'badge', text: STATUS_LABELS.lobby }));
    mountEl.append(makeProfileForm(mountEl, state, ctx));
    return;
  }

  if (snapshot.status === 'ended') {
    mountEl.append(h('div', { class: 'muted', text: '游戏已结束，可在顶部离开房间。' }));
    return;
  }

  mountEl.append(makePlayActions(state, ctx));
}

/* ---- 大厅：资料 + 准备 ---- */

function makeProfileForm(mountEl, state, ctx) {
  const me = myPlayer(state);
  if (!me) return h('div', { class: 'muted', text: '等待入座…' });

  const draft = (mountEl._draft ??= { name: me.name, avatar: me.avatar, color: me.color });

  const nameInput = h('input', {
    class: 'input', value: draft.name, maxlength: '16',
    oninput: () => { draft.name = nameInput.value; },
  });

  const avatarChips = h('div', { class: 'chip-row' }, AVATARS.map((a) =>
    h('button', {
      class: `chip${draft.avatar === a.id ? ' selected' : ''}`,
      text: `${a.emoji} ${a.label}`,
      onclick: () => { draft.avatar = a.id; renderActions(mountEl, state, ctx); },
    })));

  const colorChips = h('div', { class: 'chip-row' }, COLORS.map((c) =>
    h('button', {
      class: `chip${draft.color === c.id ? ' selected' : ''}`,
      onclick: () => { draft.color = c.id; renderActions(mountEl, state, ctx); },
    }, [h('span', { class: 'color-dot', style: `background:${c.hex}` }), c.label])));

  const readyBtn = h('button', {
    class: `btn block ${me.ready ? '' : 'primary'}`,
    text: me.ready ? '取消准备' : '准备就绪',
    onclick: () => ctx.client.send(cmd.setReady(!me.ready)),
  });

  const saveBtn = h('button', {
    class: 'btn block',
    text: '保存资料',
    onclick: () => {
      try {
        ctx.client.send(cmd.updateProfile({
          name: draft.name?.trim() || undefined,
          avatar: draft.avatar,
          color: draft.color,
        }));
        ctx.toast('资料已更新', 'success');
      } catch (e) { ctx.toast(e.message, 'error'); }
    },
  });

  return h('div', { class: 'profile-form' }, [
    h('div', { class: 'form-item' }, [h('label', { text: '昵称' }), nameInput]),
    h('div', { class: 'form-item' }, [h('label', { text: '形象' }), avatarChips]),
    h('div', { class: 'form-item' }, [h('label', { text: '颜色' }), colorChips]),
    saveBtn, readyBtn,
  ]);
}

/* ---- 游戏中：上下文操作 ---- */

function makePlayActions(state, ctx) {
  const me = myPlayer(state);
  const parts = [];

  if (state.auction) {
    parts.push(h('div', { class: 'muted', text: '拍卖进行中，请在棋盘中央出价。' }));
  } else if (canDecide(state)) {
    parts.push(...decisionButtons(state, ctx));
  } else if (canRoll(state)) {
    parts.push(...jailOrRollButtons(state, ctx, me));
  } else {
    parts.push(h('div', { class: 'muted', text: '等待其他玩家行动…' }));
  }

  if (me && !me.bankrupt && state.snapshot.status === 'playing') {
    parts.push(
      h('button', {
        class: 'btn',
        text: '🏠 我的地产',
        onclick: () => openDeedCards(ctx),
      }),
      h('button', {
        class: 'btn',
        text: '📒 账单',
        onclick: () => openLedger(ctx),
      }),
      h('button', {
        class: 'btn',
        text: '🤝 发起交易',
        onclick: () => ctx.openTrade(),
      }),
    );
  }

  return h('div', { class: 'actions' }, [
    h('div', { class: 'btn-row' }, parts),
  ]);
}

function decisionButtons(state, ctx) {
  const me = myPlayer(state);
  const tile = state.snapshot.tiles[me.position];
  const buyable = isLandedTileBuyable(state);

  return [
    h('button', {
      class: 'btn primary big block',
      disabled: !buyable,
      text: buyable ? `🏠 购买 ${tile.name}（${fmtMoney(tile.price)}）` : '当前地块不可购买',
      onclick: () => ctx.client.send(cmd.buyProperty()),
    }),
    h('button', {
      class: 'btn block',
      text: '放弃购买（进入拍卖）',
      onclick: () => ctx.client.send(cmd.passProperty()),
    }),
  ];
}

function jailOrRollButtons(state, ctx, me) {
  const parts = [h('button', {
    class: 'btn primary big block',
    text: '🎲 掷骰',
    onclick: () => ctx.client.send(cmd.rollDice()),
  })];

  if (me?.in_jail) {
    parts.push(
      h('button', {
        class: 'btn',
        text: '💳 交保释金 $50',
        onclick: () => ctx.client.send(cmd.payBail()),
      }),
      h('button', {
        class: 'btn',
        disabled: me.out_of_jail_cards <= 0,
        text: `🎟️ 使用出狱卡（${me.out_of_jail_cards}）`,
        onclick: () => ctx.client.send(cmd.useOutOfJailCard()),
      }),
    );
  }

  return parts;
}
