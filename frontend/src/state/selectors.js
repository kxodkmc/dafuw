//! 只读派生计算：从 state 推导视图所需的判断结果。
//! 视图不得自行内联业务判断，统一走这里，保证判断口径一致。

import { TILE_KINDS } from '../protocol/types.js';

/** 我的玩家条目；未入房或快照未到时为 null。 */
export function myPlayer(state) {
  if (!state.snapshot || !state.me) return null;
  return state.snapshot.players.find((p) => p.id === state.me.playerId) ?? null;
}

export function currentPlayer(state) {
  if (!state.snapshot?.current_player) return null;
  return state.snapshot.players.find((p) => p.id === state.snapshot.current_player) ?? null;
}

export function isMyTurn(state) {
  return !!state.snapshot?.current_player && state.snapshot.current_player === state.me?.playerId;
}

/** 我是否房主（建房进入时持有 ownerToken）。 */
export function isOwner(state) {
  return !!state.me?.isOwner;
}

/** 轮到我掷骰（含监狱决策）。 */
export function canRoll(state) {
  return isMyTurn(state) && state.snapshot.phase === 'await_roll';
}

/** 轮到我做购买决策（decision 阶段）。 */
export function canDecide(state) {
  return isMyTurn(state) && state.snapshot.phase === 'decision';
}

/** 当前停格是否可购买（无主的地产/铁路/公共事业）。 */
export function isLandedTileBuyable(state) {
  const me = myPlayer(state);
  if (!me) return false;
  const tile = state.snapshot.tiles[me.position];
  return !!tile && tile.price > 0 && !tile.owner && isBuyableKind(tile.kind);
}

/** 我的资产概览：现金 / 未抵押地价 / 可抵押金额；未入局时为 null。 */
export function myAssets(state) {
  const me = myPlayer(state);
  if (!me) return null;
  const tiles = state.snapshot.tiles.filter(
    (t) => t.owner === me.id && isBuyableKind(t.kind),
  );
  const alive = tiles.filter((t) => !t.mortgaged);
  return {
    cash: me.cash,
    property: alive.reduce((s, t) => s + t.price, 0),
    mortgagable: alive.reduce((s, t) => s + (t.mortgage_value ?? 0), 0),
  };
}

/** 名下可操作的地产（建房/抵押入口用）。 */
export function myOwnTiles(state) {
  const me = myPlayer(state);
  if (!me) return [];
  return state.snapshot.tiles.filter(
    (t) => t.owner === me.id && isBuyableKind(t.kind),
  );
}

/** 指定玩家的所有地产（交易面板用）。 */
export function tilesOf(state, playerId) {
  return state.snapshot.tiles.filter(
    (t) => t.owner === playerId && isBuyableKind(t.kind),
  );
}

/** 某地块当前是否可建房/升级旅馆（与引擎 can_build 同规则；不含银行库存）。 */
export function canBuildOn(state, tileId) {
  const tile = state.snapshot?.tiles[tileId];
  if (!tile || tile.kind !== 'Property' || !tile.owner || tile.mortgaged) return false;
  if (tile.houses >= 5) return false;
  const groupTiles = state.snapshot.tiles.filter((t) => t.group === tile.group);
  if (!groupTiles.length) return false;
  // 垄断 + 组内均匀建房
  return (
    groupTiles.every((t) => t.owner === tile.owner) &&
    groupTiles.every((t) => t.houses >= tile.houses)
  );
}

function isBuyableKind(kind) {
  return kind === 'Property' || kind === 'Railroad' || kind === 'Utility';
}

/** 地块所属玩家（可能为 null）。 */
export function tileOwner(state, tile) {
  if (!tile?.owner) return null;
  return state.snapshot.players.find((p) => p.id === tile.owner) ?? null;
}

/** 每边格数（含两端角格）：40 格→10，56 格→14。 */
export function sideLen(total) {
  return total / 4;
}

/** 地块在棋盘上的方位。 */
export function tileSide(id, total = 56) {
  const s = sideLen(total);
  if (id <= s) return 'bottom';
  if (id <= 2 * s) return 'left';
  if (id <= 3 * s) return 'top';
  return 'right';
}

/** 地块在 (side+1)x(side+1) 网格中的行列（1 基）。 */
export function gridPos(id, total = 56) {
  const s = sideLen(total);
  if (id <= s) return { col: s + 1 - id, row: s + 1 };
  if (id <= 2 * s) return { col: 1, row: 2 * s + 1 - id };
  if (id <= 3 * s) return { col: id - (2 * s - 1), row: 1 };
  return { col: s + 1, row: id - (3 * s - 1) };
}

/** 地块类型文案（兜底未知类型）。 */
export function kindLabel(kind) {
  return TILE_KINDS[kind]?.label ?? kind;
}
