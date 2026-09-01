//! 棋盘：11x11 网格布局、地块渲染、棋子层与中央面板。
//! 固定 662px 尺寸 + 容器缩放，保证各分辨率下布局稳定。

import { h, fmtMoney } from './dom.js';
import { GROUP_COLORS, TILE_KINDS, PHASE_LABELS, AVATARS, COLORS } from '../protocol/types.js';
import { gridPos, tileSide, currentPlayer } from '../state/selectors.js';
import { playerName } from '../state/store.js';
import { mountDice } from './fx.js';

const avatarEmoji = (id) => AVATARS.find((a) => a.id === id)?.emoji ?? '❓';
const colorHex = (id) => COLORS.find((c) => c.id === id)?.hex ?? '#999';

/**
 * 全量重绘棋盘。
 * @param {HTMLElement} mountEl 容器
 * @param {object} state store.state
 * @param {{client:import('../net/game-client.js').GameClient, onTileClick:(id:number)=>void}} env
 */
export function renderBoard(mountEl, state, env) {
  const snap = state.snapshot;
  if (!snap) {
    mountEl.replaceChildren(
      h('div', { class: 'center-banner', text: '正在同步房间数据…' }),
    );
    return;
  }

  const board = h('div', { class: 'board' }, snap.tiles.map((tile) => makeTile(state, tile, env)));

  const center = h('div', { class: 'board-center' }, [makeCenter(state)]);
  board.append(center);
  // 骰子层独立于重绘：挂在棋盘上，摇动动画跨渲染延续
  mountDice(board, state.dice);

  mountEl.replaceChildren(makeScaler(mountEl, board));
}

/** 棋盘视图：自适应缩放 + 滚轮缩放 + 拖拽平移（双击复位）。 */
function makeScaler(mountEl, board) {
  const st = (mountEl._view ??= { zoom: 1, tx: 0, ty: 0 });
  const view = h('div', { class: 'board-view' }, [board]);
  const wrap = h('div', { class: 'board-wrap' }, [view]);

  const apply = () => {
    const { clientWidth: w, clientHeight: ht } = mountEl;
    if (!w || !ht) return;
    const fit = Math.min(w / 686, ht / 686, 1.6);
    board.style.transform = `translate(${st.tx}px, ${st.ty}px) scale(${fit * st.zoom})`;
  };

  // 滚轮缩放
  view.addEventListener('wheel', (e) => {
    e.preventDefault();
    st.zoom = Math.min(5, Math.max(0.6, st.zoom * (e.deltaY < 0 ? 1.12 : 0.89)));
    apply();
  }, { passive: false });

  // 左键拖拽平移：位移超过阈值才算拖拽；拖拽后吞掉本次 click，避免误开地块详情
  let drag = null;
  const onMove = (e) => {
    if (!drag) return;
    const dx = e.clientX - drag.sx;
    const dy = e.clientY - drag.sy;
    if (!drag.moved && Math.hypot(dx, dy) < 5) return;
    drag.moved = true;
    st.tx = drag.tx + dx;
    st.ty = drag.ty + dy;
    apply();
  };
  const onUp = () => {
    if (drag?.moved) {
      view.addEventListener('click', (e) => {
        e.stopPropagation();
        e.preventDefault();
      }, { capture: true, once: true });
    }
    drag = null;
    window.removeEventListener('pointermove', onMove);
    window.removeEventListener('pointerup', onUp);
  };
  view.addEventListener('pointerdown', (e) => {
    if (e.button !== 0) return;
    drag = { sx: e.clientX, sy: e.clientY, tx: st.tx, ty: st.ty, moved: false };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  });

  // 双击复位
  view.addEventListener('dblclick', () => {
    st.zoom = 1;
    st.tx = 0;
    st.ty = 0;
    apply();
  });

  if (mountEl._fitBoard) mountEl._fitBoard.disconnect();
  const ro = new ResizeObserver(apply);
  ro.observe(mountEl);
  mountEl._fitBoard = ro;

  apply();
  return wrap;
}

/** 单个地块（方向化色条、房屋、抵押、所有者旗与棋子，文字全部水平可读）。 */
function makeTile(state, tile, env) {
  const pos = gridPos(tile.id);
  const side = tileSide(tile.id);
  const isCorner = tile.id % 10 === 0;
  const kind = TILE_KINDS[tile.kind];
  const groupHex = tile.group ? GROUP_COLORS[tile.group] : null;

  const owner = tile.owner;
  const ownerColor = owner
    ? (state.snapshot.players.find((p) => p.id === owner)?.color)
    : null;

  const inner = h('div', { class: 'tile-inner' }, [
    groupHex ? h('div', { class: 'bar', style: `background:${groupHex}` }) : null,
    h('div', { class: 'tile-mid' }, [
      h('div', { class: 'name', text: tile.name }),
      tile.price > 0 ? h('div', { class: 'price', text: fmtMoney(tile.price) }) : null,
    ]),
  ]);

  const houses = tile.houses > 0
    ? h('div', { class: 'houses', text: '🏠'.repeat(Math.min(tile.houses, 4)) + (tile.houses >= 5 ? '🏨' : '') })
    : null;

  const tokens = h('div', { class: 'tokens' }, state.snapshot.players
    .filter((p) => p.position === tile.id)
    .map((p) => h('div', {
      class: `token${p.id === state.snapshot.current_player ? ' active' : ''}`,
      style: `border-color:${colorHex(p.color)}`,
      title: p.name,
      text: avatarEmoji(p.avatar),
    })));

  return h('div', {
    class: [
      'tile',
      isCorner ? 'corner' : `side-${side}`,
      `kind-${tile.kind.toLowerCase()}`,
      tile.mortgaged ? 'mortgaged' : '',
    ].filter(Boolean).join(' '),
    style: `grid-column:${pos.col};grid-row:${pos.row};`,
    onclick: () => env.onTileClick(tile.id),
  }, [
    inner,
    houses,
    ownerColor ? h('div', { class: 'owner-flag', style: `background:${colorHex(ownerColor)}` }) : null,
    tokens,
    isCorner ? h('div', { class: 'icon', text: kind?.icon ?? '' }) : null,
  ]);
}

/** 中央面板：阶段信息 / 播报 / 结算（骰子由特效层渲染）。 */
let lastNews = null;

function makeCenter(state) {
  const snap = state.snapshot;

  if (snap.status === 'ended') {
    return h('div', { class: 'game-over' }, [
      h('div', { class: 'center-logo', text: '游戏结束' }),
      state.result
        ? [
            h('div', { class: 'champ', text: `🏆 ${playerName(state, state.result.winner)}` }),
            h('ol', {}, state.result.rank.map((r) =>
              h('li', { text: `${r.name} · 净资产 ${fmtMoney(r.net_worth)}` }))),
          ]
        : h('div', { class: 'phase-turn', text: '正在获取最终名次…' }),
    ]);
  }

  if (snap.status === 'lobby') {
    return [
      h('div', { class: 'center-logo', text: '大富翁' }),
      h('div', { class: 'center-sub', text: '等待玩家入座与准备…' }),
      h('div', { class: 'phase-turn', text: `已入座 ${snap.players.length} 人` }),
    ];
  }

  const cur = currentPlayer(state);
  // 中央播报：最近一条系统事件，变化时重放入场动画
  const news = state.log.at(-1)?.text ?? null;
  const fresh = news && news !== lastNews;
  lastNews = news;

  return [
    h('div', { class: 'center-logo', text: '大富翁' }),
    h('div', { class: 'phase-line', text: PHASE_LABELS[snap.phase] ?? snap.phase }),
    cur ? h('div', { class: 'phase-turn', text: `当前回合：${cur.name}` }) : null,
    news ? h('div', { class: `center-news${fresh ? ' fresh' : ''}`, text: news }) : null,
    h('div', { class: 'center-sub', text: `第 ${snap.round} 轮 · 点击地块查看详情` }),
  ];
}
