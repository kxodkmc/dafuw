//! 应用状态容器：唯一可变状态 + 事件归约 + 订阅分发。
//!
//! 原则：客户端不做任何规则推演，状态只由服务器快照与事件归约而来。
//! 视图层只读 state（配合 selectors 派生），不直接改写。

import { unwrap } from '../protocol/types.js';
import { describeEvent } from '../protocol/events.js';

const LOG_LIMIT = 200;
const LEDGER_LIMIT = 500;

/** 骰子事件序号：供 fx 层识别新一次掷骰 */
let diceSeq = 0;

/** @typedef {'lobby'|'room'} Screen */

export function createStore() {
  /** @type {Array<(reason:string)=>void>} */
  const subscribers = [];

  const state = {
    /** @type {Screen} */
    screen: 'lobby',
    connected: false,
    /** @type {number|null} */
    roomCode: null,
    /** 当前玩家身份（与 client.ident 同步，供视图读取） */
    me: null,
    /** @type {import('../protocol/types.js').GameStateDto|null} 最新快照 */
    snapshot: null,
    /** 拍卖进行中的 UI 状态：{ tileId, highest, highestBy } */
    auction: null,
    /** 收到的待处理交易：TradeProposed 载荷数组 */
    trades: [],
    /** 最后一次 DiceRolled：{ dice, doubles } */
    dice: null,
    /** GameEnded 载荷（名次榜） */
    result: null,
    /** 滚动日志 [{kind, text}] */
    log: [],
    /** 账单流水 [{ts, playerId, delta, reason}]（本局累计，开局清空） */
    ledger: [],
    /** 回合开始缓冲：此时刻之前掷骰按钮给出提示（与服务端 1.5s 缓冲对应） */
    rollReadyAt: 0,
  };

  function notify(reason) {
    for (const fn of subscribers) fn(reason);
  }

  /** 事件归约：快照替换 + 局部追踪 + 日志追加。 */
  function applyEvent(tagged) {
    const [name, d] = unwrap(tagged);

    switch (name) {
      case 'RoomSnapshot': {
        state.snapshot = d;
        // 快照权威：state.auction 由快照整体重建（含当前出价人/弃权名单），事件只做日志
        const a = d.auction;
        state.auction = a
          ? {
              tileId: a.tile_id,
              highest: a.highest_bid,
              highestBy: a.highest_bidder,
              turn: a.current_bidder,
              passed: a.passed ?? [],
            }
          : null;
        if (d.status === 'lobby') state.result = null;
        break;
      }

      case 'TradeProposed':
        state.trades = [...state.trades, d];
        break;
      case 'TradeAccepted':
      case 'TradeDeclined':
        state.trades = state.trades.filter((t) => t.trade_id !== d.trade_id);
        break;

      case 'TurnStarted':
        state.dice = null;
        // 回合缓冲：与服务端 TURN_COOLDOWN 对应，稍长 100ms 防止时钟误差
        state.rollReadyAt = Date.now() + 1600;
        break;
      case 'DiceRolled':
        state.dice = { dice: d.dice, doubles: d.doubles, seq: ++diceSeq };
        break;

      case 'GameStarted':
        state.result = null;
        state.ledger = [];
        break;
      case 'GameEnded':
        state.result = d;
        break;

      case 'MoneyChanged':
        // 账单流水：随事件累计（断线重连后从当批事件继续，历史不入快照）
        state.ledger = [
          ...state.ledger.slice(-(LEDGER_LIMIT - 1)),
          { ts: Date.now(), playerId: d.player_id, delta: d.delta, reason: d.reason ?? '' },
        ];
        break;

      case 'RoomClosed':
        // 由入口统一处理离开流程
        break;
    }

    const entry = describeEvent(tagged, {
      playerName: (id) => playerName(state, id),
      tileName: (id) => tileName(state, id),
    });
    if (entry) {
      state.log = [...state.log.slice(-(LOG_LIMIT - 1)), { ...entry, ts: Date.now() }];
    }

    notify(name);
  }

  return {
    state,

    subscribe(fn) {
      subscribers.push(fn);
      return () => {
        const i = subscribers.indexOf(fn);
        if (i >= 0) subscribers.splice(i, 1);
      };
    },

    applyEvent,

    /** 进入房间（身份来自 client）。 */
    enterRoom(roomCode, me) {
      state.screen = 'room';
      state.roomCode = roomCode;
      state.me = me;
      state.snapshot = null;
      state.auction = null;
      state.trades = [];
      state.dice = null;
      state.result = null;
      state.log = [];
      state.ledger = [];
      notify('enterRoom');
    },

    /** 离开房间回到大厅（保留服务器地址）。 */
    leaveRoom() {
      state.screen = 'lobby';
      state.roomCode = null;
      state.me = null;
      state.snapshot = null;
      state.auction = null;
      state.trades = [];
      state.dice = null;
      state.result = null;
      state.log = [];
      state.ledger = [];
      notify('leaveRoom');
    },

    setConnected(connected) {
      if (state.connected !== connected) {
        state.connected = connected;
        notify('connected');
      }
    },

    /** 触发一次全量重绘（供特效层在动画结束后归还棋子显示）。 */
    refresh() {
      notify('fx');
    },
  };
}

/* ---- 供日志与视图使用的快照查找 ---- */

export function playerName(state, id) {
  return state.snapshot?.players.find((p) => p.id === id)?.name ?? '玩家';
}

export function tileName(state, id) {
  return state.snapshot?.tiles[id]?.name ?? `格 ${id}`;
}
