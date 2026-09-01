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

/** 棋盘固定像素：角格 88px，其余 54px（与 board.css 保持一致）；边数随棋盘规格自适应。 */
const CORNER = 88;
const CELL = 54;

/** 读取当前棋盘与其每边格数（40 格→10，56 格→14）。 */
function readBoard() {
  const board = document.querySelector('.board');
  if (!board) return null;
  const s = getComputedStyle(board).gridTemplateColumns.split(' ').length - 1;
  return { board, s };
}

/** 地块在棋盘内的像素矩形（未缩放坐标，中心点 cx/cy）。 */
function cellRect(tileId, s) {
  const n = s + 1;
  const { col, row } = gridPos(tileId, s * 4);
  const w = col === 1 || col === n ? CORNER : CELL;
  const h = row === 1 || row === n ? CORNER : CELL;
  const x0 = col === 1 ? 0 : col === n ? CORNER + CELL * (s - 1) : CORNER + (col - 2) * CELL;
  const y0 = row === 1 ? 0 : row === n ? CORNER + CELL * (s - 1) : CORNER + (row - 2) * CELL;
  return { x: x0, y: y0, w, h, cx: x0 + w / 2, cy: y0 + h / 2 };
}

/**
 * 在地块上方飘出一条金额/状态文字。
 * @param {number} tileId 地块编号
 * @param {string} text 文案（如「租金 -$250K」）
 * @param {'pos'|'neg'} tone 颜色基调
 */
export function floatOnTile(tileId, text, tone) {
  const env = readBoard();
  if (!env) return;
  const rect = env.board.getBoundingClientRect();
  if (!rect.width) return;
  const scale = rect.width / (CORNER * 2 + CELL * (env.s - 1));
  const r = cellRect(tileId, env.s);
  spawnFloat(rect.left + r.cx * scale, rect.top + (r.y + r.h * 0.3) * scale, text, tone);
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

/* ---- 棋子逐格移动 ---- */

const movingTokens = new Set(); // 动画中的玩家 id：真实棋子暂时隐藏
const activeGhosts = new Map(); // pid -> { el, timer }
let tokenLayer = null;
let onMoveDone = null;

/** 渲染期查询：该玩家的棋子是否正在逐格移动（隐藏真身，由幽灵棋子表现）。 */
export function isTokenMoving(pid) {
  return movingTokens.has(pid);
}

/** 动画结束回调（main.js 注入 store.refresh，归还棋子显示）。 */
export function setOnMoveDone(cb) {
  onMoveDone = cb;
}

/**
 * 每次棋盘重绘时调用：重建棋子动画层，并把进行中的幽灵棋子挂回新棋盘。
 * @param {HTMLElement} boardEl .board 元素
 */
export function rebindTokenLayer(boardEl) {
  tokenLayer = boardEl.querySelector('.fx-token-layer');
  if (!tokenLayer) {
    tokenLayer = h('div', { class: 'fx-token-layer' });
    boardEl.append(tokenLayer);
  }
  for (const g of activeGhosts.values()) tokenLayer.append(g.el);
}

/** 事件到达即先登记（早于重绘），保证真实棋子从第一帧就被隐藏。 */
export function markMoving(pid) {
  movingTokens.add(pid);
}

/**
 * 棋子逐格移动：从 from 逐格走到 to（经过 GO 自动绕行），节奏随路程自适应。
 * @param {string} pid 玩家 id
 * @param {number} from 起始格
 * @param {number} to 目标格
 * @param {{emoji:string, hex:string}} look 棋子外观（形象 emoji + 颜色边框）
 */
export function animateToken(pid, from, to, { emoji, hex }) {
  clearGhost(pid);
  const env = readBoard();
  if (!env || from === to) {
    onMoveDone?.();
    return;
  }
  const total = env.s * 4;
  const path = [];
  let cur = from;
  do {
    cur = (cur + 1) % total;
    path.push(cur);
  } while (cur !== to && path.length < total);

  const stepMs = Math.max(70, Math.min(130, Math.round(1100 / path.length)));
  movingTokens.add(pid);
  const el = h('div', { class: 'fx-token', text: emoji });
  el.style.borderColor = hex;
  const place = (tileId) => {
    const r = cellRect(tileId, env.s);
    el.style.left = `${r.cx}px`;
    el.style.top = `${r.cy}px`;
  };
  place(from);
  tokenLayer?.append(el);
  const my = { el, timer: null };
  activeGhosts.set(pid, my);

  let i = 0;
  const step = () => {
    if (activeGhosts.get(pid) !== my) return; // 已被该玩家的新动画接管
    if (i >= path.length) {
      clearGhost(pid);
      onMoveDone?.();
      return;
    }
    place(path[i]);
    i += 1;
    my.timer = setTimeout(step, stepMs);
  };
  my.timer = setTimeout(step, stepMs);
}

function clearGhost(pid) {
  const g = activeGhosts.get(pid);
  if (g) {
    clearTimeout(g.timer);
    g.el.remove();
    activeGhosts.delete(pid);
  }
  movingTokens.delete(pid);
}
