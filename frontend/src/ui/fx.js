//! 棋盘特效层：骰子摇动、地块金额飘字、抽卡翻牌横幅。
//!
//! 特效独立于全量重绘（飘字/横幅挂在 body 级固定层，骰子层在每次重绘时重挂
//! 并延续进行中的动画），保证动画不会被快照刷新打断。

import { h } from './dom.js';
import { gridPos } from '../state/selectors.js';

/* ---- 固定特效根 ---- */

let fxRoot = null;
function root() {
  if (!fxRoot) {
    fxRoot = h('div', { class: 'fx-root' });
    document.body.append(fxRoot);
  }
  return fxRoot;
}

/* ---- 骰子层 ---- */

const DICE_FACES = ['⚀', '⚁', '⚂', '⚃', '⚄', '⚅'];

let diceLayer = null;
let diceTimer = null;
let diceSeq = 0;
let diceUntil = 0;
let diceFinal = null;
let diceDoubles = false;

/**
 * 每次棋盘重绘时调用：把骰子层挂到新棋盘上，并延续未完成的摇动动画。
 * @param {HTMLElement} boardEl .board 元素
 * @param {{dice:number[], doubles:boolean, seq:number}|null} dice store 中的骰子态
 */
export function mountDice(boardEl, dice) {
  diceLayer = h('div', { class: 'dice-layer' });
  boardEl.append(diceLayer);

  if (dice) {
    diceFinal = dice.dice;
    diceDoubles = dice.doubles;
    if (dice.seq !== diceSeq) {
      diceSeq = dice.seq;
      diceUntil = performance.now() + 700;
      diceTimer ??= setInterval(tickDice, 80);
    }
  } else {
    diceFinal = null;
    diceDoubles = false;
  }
  if (!diceTimer) paintDice();
}

/** 摇动中：随机面闪烁；到点后定格真实点数。 */
function tickDice() {
  if (performance.now() >= diceUntil) {
    clearInterval(diceTimer);
    diceTimer = null;
    paintDice();
    return;
  }
  if (!diceLayer) return;
  diceLayer.classList.add('rolling');
  const n = diceFinal?.length ?? 2;
  diceLayer.replaceChildren(
    ...Array.from({ length: n }, () =>
      h('div', { class: 'dice', text: DICE_FACES[Math.floor(Math.random() * 6)] })),
  );
}

function paintDice() {
  if (!diceLayer) return;
  diceLayer.classList.remove('rolling');
  if (!diceFinal) {
    diceLayer.replaceChildren();
    return;
  }
  diceLayer.replaceChildren(
    ...diceFinal.map((n) => h('div', { class: 'dice', text: DICE_FACES[n - 1] ?? '?' })),
    diceDoubles ? h('span', { class: 'doubles-badge', text: '双数！' }) : null,
  );
}

/* ---- 地块飘字 ---- */

/** 11x11 棋盘固定像素：角格 88px，其余 54px（与 board.css 保持一致）。 */
const CORNER = 88;
const CELL = 54;

/**
 * 在地块上方飘出一条金额/状态文字。
 * @param {number} tileId 地块编号
 * @param {string} text 文案（如「租金 -$250K」）
 * @param {'pos'|'neg'} tone 颜色基调
 */
export function floatOnTile(tileId, text, tone) {
  const board = document.querySelector('.board');
  if (!board) return;
  const rect = board.getBoundingClientRect();
  if (!rect.width) return;
  const scale = rect.width / (CORNER * 2 + CELL * 9);

  const { col, row } = gridPos(tileId);
  const w = col === 1 || col === 11 ? CORNER : CELL;
  const ht = row === 1 || row === 11 ? CORNER : CELL;
  const x0 = col === 1 ? 0 : col === 11 ? CORNER + CELL * 9 : CORNER + (col - 2) * CELL;
  const y0 = row === 1 ? 0 : row === 11 ? CORNER + CELL * 9 : CORNER + (row - 2) * CELL;

  spawnFloat(rect.left + (x0 + w / 2) * scale, rect.top + (y0 + ht * 0.3) * scale, text, tone);
}

function spawnFloat(x, y, text, tone) {
  const el = h('div', { class: `fx-float ${tone}`, text });
  el.style.left = `${x}px`;
  el.style.top = `${y}px`;
  root().append(el);
  setTimeout(() => el.remove(), 1900);
}

/* ---- 抽卡横幅 ---- */

let cardNode = null;
let cardTimer = null;

/**
 * 棋盘中央翻牌横幅：展示抽到的机会/命运卡。
 * @param {{deck:string, text:string}} param deck: 'Chance'|'Fate'
 */
export function showCardBanner({ deck, text }) {
  const meta = deck === 'Chance'
    ? { icon: '❓', label: '机 会' }
    : { icon: '📜', label: '命 运' };

  cardNode?.remove();
  clearTimeout(cardTimer);
  cardNode = h('div', { class: 'fx-card-overlay' }, [
    h('div', { class: 'fx-card' }, [
      h('div', { class: 'fx-card-head', text: `${meta.icon} ${meta.label}` }),
      h('div', { class: 'fx-card-text', text }),
    ]),
  ]);
  root().append(cardNode);
  cardTimer = setTimeout(() => {
    cardNode?.classList.add('out');
    cardTimer = setTimeout(() => {
      cardNode?.remove();
      cardNode = null;
      cardTimer = null;
    }, 320);
  }, 3000);
}
