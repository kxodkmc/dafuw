//! 玩家列表面板。

import { h, fmtMoney } from './dom.js';
import { AVATARS, COLORS } from '../protocol/types.js';
import { currentPlayer } from '../state/selectors.js';

const emojiOf = (id) => AVATARS.find((a) => a.id === id)?.emoji ?? '❓';
const hexOf = (id) => COLORS.find((c) => c.id === id)?.hex ?? '#999';

/**
 * 全量重绘玩家列表。
 * @param {HTMLElement} mountEl
 * @param {object} state store.state
 */
export function renderPlayers(mountEl, state) {
  const snap = state.snapshot;
  mountEl.replaceChildren(
    h('div', { class: 'card-title', text: '玩家' }),
    h('div', { class: 'player-list' },
      (snap?.players ?? []).map((p) => {
        const cur = currentPlayer(state);
        const isTurn = cur?.id === p.id;
        const tags = [];
        if (snap.status === 'lobby') {
          tags.push(p.ready ? '✓ 已准备' : '… 未准备');
        } else {
          if (p.in_jail) tags.push('⛓️ 狱中');
          if (p.out_of_jail_cards > 0) tags.push(`🎟️ ×${p.out_of_jail_cards}`);
          tags.push(`📍 格${p.position}`);
        }

        return h('div', {
          class: [
            'player-card',
            isTurn && snap.status === 'playing' ? 'turn' : '',
            p.bankrupt ? 'broke' : '',
          ].filter(Boolean).join(' '),
        }, [
          h('div', { class: 'avatar', style: `border-color:${hexOf(p.color)}`, text: emojiOf(p.avatar) }),
          h('div', { class: 'grow' }, [
            h('div', { class: 'p-name' }, [
              p.name,
              p.id === state.me?.playerId ? h('span', { class: 'muted', text: '（你）' }) : null,
              isTurn ? h('span', { class: 'badge', text: '行动中' }) : null,
            ]),
            h('div', { class: 'p-tags' },
              tags.map((t) => h('span', { class: 't', text: t }))),
          ]),
          snap.status !== 'lobby'
            ? h('div', { class: 'p-cash' }, [
                h('div', { text: fmtMoney(p.cash) }),
                h('div', { class: 'muted', text: `净资产 ${fmtMoney(p.net_worth)}` }),
              ])
            : null,
        ]);
      }),
    ),
  );
}
