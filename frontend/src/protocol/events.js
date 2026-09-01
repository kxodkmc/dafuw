//! 事件 → 展示文本的纯映射。
//!
//! 只做「协议事件 → 人类可读文案」的转换，不维护状态。
//! 返回 `null` 表示该事件无需进入日志（如纯状态同步事件）。

import { unwrap } from './types.js';

/** 日志条目种类：sys=系统/事件播报，chat=玩家聊天。 */
const money = (n) => `$${n}`;

/**
 * @param {object} tagged 服务器推送的外部标签事件
 * @param {{playerName:(id:string)=>string, tileName:(id:number)=>string, meId:string}} ctx
 * @returns {{kind:'sys'|'chat', text:string} | null}
 */
export function describeEvent(tagged, ctx) {
  const [name, d] = unwrap(tagged);
  const P = (id) => ctx.playerName(id);
  const T = (id) => `「${ctx.tileName(id)}」`;

  switch (name) {
    case 'RoomSnapshot':
    case 'PlayerMoved':
    case 'MoneyChanged':
    case 'Error':
      return null; // 纯状态或由 toast 呈现，不进日志

    case 'Message': return { kind: 'sys', text: d.text };
    case 'Chat': return { kind: 'chat', text: `${d.name}：${d.text}` };

    case 'PlayerJoined': return { kind: 'sys', text: `${d.name} 加入了房间` };
    case 'PlayerLeft': return { kind: 'sys', text: `${P(d.player_id)} 离开了房间` };
    case 'PlayerReady': return { kind: 'sys', text: `${P(d.player_id)} ${d.ready ? '已准备' : '取消了准备'}` };

    case 'GameStarted': return { kind: 'sys', text: `游戏开始！先手：${P(d.first_player)}` };
    case 'TurnStarted': return { kind: 'sys', text: `轮到 ${P(d.player_id)}` };

    case 'DiceRolled': {
      const nums = d.dice.join(' + ');
      const extra = d.doubles ? '（双数！再掷一次）' : '';
      return { kind: 'sys', text: `${P(d.player_id)} 掷出 ${nums}=${d.dice.reduce((a, b) => a + b, 0)}${extra}` };
    }
    case 'PassedGo': return { kind: 'sys', text: `${P(d.player_id)} 经过起点，领取 ${money(d.salary)}` };
    case 'LandedOn': return { kind: 'sys', text: `${P(d.player_id)} 停在 ${T(d.tile_id)}` };

    case 'PropertyBought': return { kind: 'sys', text: `${P(d.player_id)} 以 ${money(d.price)} 购入 ${T(d.tile_id)}` };
    case 'RentPaid': return { kind: 'sys', text: `${P(d.from)} 向 ${P(d.to)} 支付租金 ${money(d.amount)}（${T(d.tile_id)}）` };

    case 'CardDrawn': return { kind: 'sys', text: `${P(d.player_id)}：${d.card_text}` };
    case 'CardApplied': return { kind: 'sys', text: `${P(d.player_id)}：${d.text}` };
    case 'TaxPaid': return { kind: 'sys', text: `${P(d.player_id)} 缴税 ${money(d.amount)}` };

    case 'WentToJail': return { kind: 'sys', text: `${P(d.player_id)} 进监狱了` };
    case 'JailReleased': return { kind: 'sys', text: `${P(d.player_id)} 出狱` };
    case 'JailTurnSkipped': return { kind: 'sys', text: `${P(d.player_id)} 在狱中跳过本回合` };

    case 'BuildHouse': return { kind: 'sys', text: `${P(d.player_id)} 在 ${T(d.tile_id)} 盖房（第 ${d.houses} 栋）` };
    case 'SoldHouse': return { kind: 'sys', text: `${P(d.player_id)} 在 ${T(d.tile_id)} 卖房（剩 ${d.houses} 栋）` };
    case 'Mortgaged': return { kind: 'sys', text: `${P(d.player_id)} 抵押 ${T(d.tile_id)}，得 ${money(d.amount)}` };
    case 'Unmortgaged': return { kind: 'sys', text: `${P(d.player_id)} 赎回 ${T(d.tile_id)}，花 ${money(d.cost)}` };

    case 'AuctionStarted': return { kind: 'sys', text: `${T(d.tile_id)} 进入拍卖` };
    case 'AuctionBid': return { kind: 'sys', text: `${P(d.player_id)} 出价 ${money(d.amount)}` };
    case 'AuctionPassed': return { kind: 'sys', text: `${P(d.player_id)} 弃权` };
    case 'AuctionEnded': {
      const head = `${T(d.tile_id)} 拍卖结束`;
      return { kind: 'sys', text: d.winner ? `${head}：${P(d.winner)} 以 ${money(d.amount)} 拍下` : `${head}，无人出价` };
    }

    case 'TradeProposed': return { kind: 'sys', text: `${P(d.from)} 向 ${P(d.to)} 发起交易` };
    case 'TradeAccepted': return { kind: 'sys', text: `交易 ${short(d.trade_id)} 被接受` };
    case 'TradeDeclined': return { kind: 'sys', text: `交易 ${short(d.trade_id)} 被拒绝` };

    case 'PlayerBroke': {
      const tail = d.creditor ? `，清算给 ${P(d.creditor)}` : '';
      return { kind: 'sys', text: `${P(d.player_id)} 破产了${tail}` };
    }
    case 'GameEnded': return { kind: 'sys', text: `游戏结束！冠军：${P(d.winner)}` };
    case 'RoomClosed': return { kind: 'sys', text: '房间已解散' };

    default: return { kind: 'sys', text: `未知事件：${name}` };
  }
}

const short = (id) => String(id).slice(0, 8);
