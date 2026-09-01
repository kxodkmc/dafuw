//! 协议常量与指令构造：镜像 Rust 侧 `monopoly-common` 的类型约定。
//!
//! 服务端使用 serde 外部标签格式（externally tagged）：
//! - Command → `{"RollDice": null}`、`{"AuctionBid": {"amount": 500}}`
//! - Event   → `{"DiceRolled": {...}}`
//! 字段名保持 snake_case，与 Rust 结构体一致。

/** 玩家形象（对应 Rust Avatar，snake_case 序列化）。 */
export const AVATARS = [
  { id: 'dog', emoji: '🐕', label: '狗' },
  { id: 'car', emoji: '🚗', label: '轿车' },
  { id: 'bicycle', emoji: '🚲', label: '单车' },
  { id: 'tree', emoji: '🌳', label: '大树' },
  { id: 'bed', emoji: '🛏️', label: '床' },
];

/** 玩家颜色（对应 Rust Color，snake_case 序列化）。 */
export const COLORS = [
  { id: 'red', hex: '#c62828', label: '红色' },
  { id: 'yellow', hex: '#f9a825', label: '黄色' },
  { id: 'green', hex: '#43a047', label: '绿色' },
  { id: 'blue', hex: '#1e88e5', label: '蓝色' },
  { id: 'pink', hex: '#f06292', label: '粉色' },
  { id: 'purple', hex: '#8e24aa', label: '紫色' },
  { id: 'orange', hex: '#ef6c00', label: '橙色' },
];

/** 地产色组（对应 Rust Group，PascalCase 序列化）。 */
export const GROUP_COLORS = {
  Brown: '#8d6e63',
  LightBlue: '#81d4fa',
  Pink: '#f48fb1',
  Orange: '#ffb74d',
  Red: '#e57373',
  Yellow: '#fff176',
  Green: '#81c784',
  DarkBlue: '#5c6bc0',
};

/** 地块类型图标与文案（对应 Rust TileKind，PascalCase 序列化）。 */
export const TILE_KINDS = {
  Go: { icon: '🏁', label: '起点' },
  Property: { icon: '', label: '地产' },
  Railroad: { icon: '🚂', label: '铁路' },
  Utility: { icon: '💡', label: '公共事业' },
  Chance: { icon: '❓', label: '机会' },
  Fate: { icon: '📜', label: '命运' },
  Tax: { icon: '💰', label: '税收' },
  Jail: { icon: '🚔', label: '监狱' },
  GoToJail: { icon: '👮', label: '入狱' },
  FreeParking: { icon: '🅿️', label: '免费停车' },
};

/** 提取外部标签事件的名称与载荷：`{"DiceRolled":{...}} → ["DiceRolled", {...}]`。 */
export function unwrap(tagged) {
  const name = Object.keys(tagged)[0];
  return [name, tagged[name]];
}

/**
 * 指令构造器：只负责形状，不负责校验；合法性由服务器裁决。
 * 所有方法返回可直接 JSON.stringify 的 serde 外部标签对象。
 */
export const cmd = {
  startGame: () => ({ StartGame: null }),
  rollDice: () => ({ RollDice: null }),
  payBail: () => ({ PayBail: null }),
  useOutOfJailCard: () => ({ UseOutOfJailCard: null }),
  buyProperty: () => ({ BuyProperty: null }),
  passProperty: () => ({ PassProperty: null }),
  auctionBid: (amount) => ({ AuctionBid: { amount } }),
  auctionPass: () => ({ AuctionPass: null }),
  buildHouse: (tile_id) => ({ BuildHouse: { tile_id } }),
  sellHouse: (tile_id) => ({ SellHouse: { tile_id } }),
  mortgage: (tile_id) => ({ Mortgage: { tile_id } }),
  unmortgage: (tile_id) => ({ Unmortgage: { tile_id } }),
  proposeTrade: (target, offer, demand) => ({ ProposeTrade: { target, offer, demand } }),
  acceptTrade: (trade_id) => ({ AcceptTrade: { trade_id } }),
  declineTrade: (trade_id) => ({ DeclineTrade: { trade_id } }),
  updateProfile: (patch) => ({ UpdateProfile: patch }),
  setReady: (ready) => ({ SetReady: { ready } }),
  chat: (text) => ({ Chat: { text } }),
};

/** 空的资产清单（AssetList），用于交易构造。 */
export const emptyAssets = () => ({ cash: 0, tiles: [], out_of_jail_cards: 0 });

/** 房间状态文案。 */
export const STATUS_LABELS = { lobby: '等待中', playing: '游戏中', ended: '已结束' };

/** 回合阶段文案。 */
export const PHASE_LABELS = {
  await_roll: '等待掷骰',
  decision: '决定是否购买',
  auction: '拍卖进行中',
  over: '游戏结束',
};
