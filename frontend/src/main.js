//! 应用入口：装配 store / client / 视图路由 / 全局事件处理。
//!
//! 数据流（单向）：
//!   server ──HTTP/WS──► GameClient ──事件──► store（归约）──订阅──► 视图
//!   视图 ──用户意图──► GameClient.send(Command) ──► server

import { createStore } from './state/store.js';
import { GameClient } from './net/game-client.js';
import { unwrap } from './protocol/types.js';
import { loadIdent, clearIdent } from './state/session.js';
import { canBuildOn } from './state/selectors.js';
import { toast } from './ui/toast.js';
import { mountLobby } from './ui/lobby.js';
import { mountRoom } from './ui/room.js';
import * as fx from './ui/fx.js';
import { fmtMoney } from './ui/dom.js';
import { AVATARS, COLORS } from './protocol/types.js';

/* ---- 组合根 ---- */

const store = createStore();

const client = new GameClient({
  onEvent: handleEvent,
  onStatus: (connected) => {
    const was = store.state.connected;
    store.setConnected(connected);
    // 断线重连成功后给用户明确反馈
    if (!was && connected && store.state.screen === 'room') {
      toast('已重新连接到房间', 'success');
    }
  },
});

const ctx = {
  store,
  client,
  toast,
  exitRoom,
};

/* ---- 事件处理 ---- */

function handleEvent(tagged) {
  const [name, data] = unwrap(tagged);

  if (name === 'RoomClosed') {
    toast('房间已解散', 'error');
    // 房间不复存在，身份随之失效
    client.leaveRoom({ forget: true });
    store.leaveRoom();
    route();
    return;
  }
  if (name === 'Error') {
    toast(data.message, 'error');
  }

  // 抽卡横幅需要牌堆类型：在快照刷新前从玩家所停格推断
  const deck = name === 'CardDrawn' ? inferDeck(data.player_id) : null;

  // 棋子移动：先登记（早于重绘），让真实棋子在第一帧就被动画幽灵接管
  if (name === 'PlayerMoved') fx.markMoving(data.player_id);

  store.applyEvent(tagged);

  // 特效在重绘完成后播放（定位依赖最新 DOM）
  setTimeout(() => playFx(name, data, deck), 0);
}

/** 动画结束 → 触发一次重绘，把隐藏的真实棋子归还显示。 */
fx.setOnMoveDone(() => store.refresh());

/** 事件 → 棋盘特效：飘字让资金变动在棋盘上肉眼可见（如租金扣除）。 */
function playFx(name, d, deck) {
  switch (name) {
    case 'CardDrawn':
      fx.showCardBanner({ deck, text: d.card_text });
      break;
    case 'PlayerMoved': {
      // 棋子逐格移动：真实棋子暂隐，幽灵棋子按格走完才归还
      const p = store.state.snapshot?.players.find((x) => x.id === d.player_id);
      if (p) {
        const emoji = AVATARS.find((a) => a.id === p.avatar)?.emoji ?? '❓';
        const hex = COLORS.find((c) => c.id === p.color)?.hex ?? '#999';
        fx.animateToken(d.player_id, d.from, d.to, { emoji, hex });
      }
      break;
    }
    case 'RentPaid':
      cancelMoneyFloat([d.from, d.to]);
      fx.floatOnTile(d.tile_id, `租金 -${fmtMoney(d.amount)}`, 'neg');
      break;
    case 'PropertyBought':
      cancelMoneyFloat([d.player_id]);
      fx.floatOnTile(d.tile_id, `购入 -${fmtMoney(d.price)}`, 'neg');
      break;
    case 'TaxPaid':
      cancelMoneyFloat([d.player_id]);
      fx.floatOnTile(d.tile_id, `缴税 -${fmtMoney(d.amount)}`, 'neg');
      break;
    case 'PassedGo':
      cancelMoneyFloat([d.player_id]);
      fx.floatOnTile(0, `薪水 +${fmtMoney(d.salary)}`, 'pos');
      break;
    case 'LandedOn': {
      // 落在自己可建房的地块：提醒别错过盖房时机（官方规则：落自己地不弹决策）
      const s = store.state;
      const t = s.snapshot?.tiles[d.tile_id];
      if (
        d.player_id === s.me?.playerId &&
        t?.owner === s.me?.playerId &&
        canBuildOn(s, d.tile_id)
      ) {
        const stage = t.houses === 4 ? '升级旅馆' : '盖房';
        toast(`停在自己的「${t.name}」，可以${stage}（点击地块操作）`, 'success');
      }
      break;
    }
    case 'WentToJail':
      // 监狱角格：40 格棋盘在 10，56 格在 14
      fx.floatOnTile((store.state.snapshot?.tiles.length ?? 40) / 4 === 14 ? 14 : 10, '入狱！', 'neg');
      break;
    case 'BuildHouse':
      fx.floatOnTile(d.tile_id, '建房 +🏠', 'pos');
      break;
    case 'SoldHouse':
      fx.floatOnTile(d.tile_id, '卖房 -🏠', 'neg');
      break;
    case 'Mortgaged':
      fx.floatOnTile(d.tile_id, '已抵押', 'neg');
      break;
    case 'Unmortgaged':
      fx.floatOnTile(d.tile_id, '已赎回', 'pos');
      break;
    case 'MoneyChanged':
      // 卡牌/交易等资金变动：在玩家所在格飘字（与专属飘字重复时由挂起机制去重）
      scheduleMoneyFloat(d);
      break;
  }
}

/** MoneyChanged 延迟播放：若同批次出现专属资金事件（租金/购地等）则取消，避免重复飘字。 */
const pendingMoney = new Map();

function scheduleMoneyFloat(d) {
  const prev = pendingMoney.get(d.player_id);
  if (prev) clearTimeout(prev.timer);
  const timer = setTimeout(() => {
    pendingMoney.delete(d.player_id);
    const p = store.state.snapshot?.players.find((x) => x.id === d.player_id);
    if (p) fx.floatOnTile(p.position, `${d.delta >= 0 ? '+' : '-'}${fmtMoney(Math.abs(d.delta))}`, d.delta >= 0 ? 'pos' : 'neg');
  }, 80);
  pendingMoney.set(d.player_id, { timer });
}

/** 专属资金事件到达：取消同玩家挂起中的 MoneyChanged 飘字。 */
function cancelMoneyFloat(playerIds) {
  for (const id of playerIds) {
    const p = pendingMoney.get(id);
    if (p) {
      clearTimeout(p.timer);
      pendingMoney.delete(id);
    }
  }
}

/** 推断抽卡牌堆：玩家当前所停格的类型（Chance/Fate）。 */
function inferDeck(playerId) {
  const s = store.state;
  const p = s.snapshot?.players.find((x) => x.id === playerId);
  return s.snapshot?.tiles[p?.position]?.kind;
}

/** 用户主动离开：保留身份，回大厅后可一键「回到房间」恢复原角色。 */
function exitRoom() {
  client.leaveRoom();
  store.leaveRoom();
  route(); // screen 已变为 lobby，强制切视图
}

/* ---- 视图路由 ---- */

const appEl = document.getElementById('app');
let current = null; // { name, view }

function route() {
  const name = store.state.screen;
  if (current?.name === name) return;

  current?.view.unmount();
  const view = name === 'room' ? mountRoom(ctx, appEl) : mountLobby(ctx, appEl);
  current = { name, view };
}

store.subscribe(route);

/* ---- 启动：恢复上次会话 ---- */

async function boot() {
  const saved = loadIdent();
  if (saved) {
    try {
      await client.http.snapshot(saved.roomCode); // 房间仍存在才恢复
      client.resumeRoom();
      store.enterRoom(saved.roomCode, {
        playerId: saved.playerId,
        isOwner: saved.isOwner,
        ownerToken: saved.ownerToken,
      });
      return;
    } catch {
      clearIdent(); // 房间已不存在，回大厅
    }
  }
  route();
}

boot();
