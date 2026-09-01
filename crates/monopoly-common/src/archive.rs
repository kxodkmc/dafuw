//! 对局存档：可完整序列化/反序列化的对局状态。
//!
//! [`GameStateDto`] 快照覆盖了棋盘与玩家等可展示信息，但不含拍卖、交易、
//! 卡组洗牌顺序等引擎内部状态；[`GameArchive`] 在快照之外补齐这些字段，
//! 保证持久化后能精确重建 [`GameEngine`](monopoly_core::engine::GameEngine)。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::command::AssetList;
use super::room::RoomSettings;
use super::snapshot::GameStateDto;

/// 银行拍卖的中间状态（`GameStateDto` 未覆盖）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuctionDto {
    pub tile_id: usize,
    pub order: Vec<Uuid>,
    pub turn: usize,
    pub highest_bid: u32,
    pub highest_bidder: Option<Uuid>,
    pub passed: Vec<Uuid>,
}

/// 待处理交易的中间状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeDto {
    pub id: Uuid,
    pub from: Uuid,
    pub to: Uuid,
    pub offer: AssetList,
    pub demand: AssetList,
}

/// 快照未覆盖的引擎内部状态，配合 [`GameStateDto`] 才能精确重建一局。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArchiveExtra {
    /// 上次掷骰结果。
    pub dice: Vec<u8>,
    /// 连续对子计数。
    pub doubles_count: u8,
    /// 掷出对子、等待再次掷骰。
    pub pending_doubles: bool,
    /// 本轮动作已结束、等待收尾。
    pub turn_ended: bool,
    /// 进行中的银行拍卖。
    pub auction: Option<AuctionDto>,
    /// 待拍卖的地产队列（破产清算触发，逐块拍卖）。
    pub auction_queue: Vec<usize>,
    /// 待处理的玩家交易。
    pub pending_trades: Vec<TradeDto>,
    /// 「机会」卡组洗牌后的顺序与抽卡位置。
    pub chance_order: Vec<usize>,
    pub chance_draw_index: usize,
    /// 「命运」卡组洗牌后的顺序与抽卡位置。
    pub fate_order: Vec<usize>,
    pub fate_draw_index: usize,
}

/// 完整对局存档 = 房间配置 + 可展示快照 + 补充内部状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameArchive {
    pub settings: RoomSettings,
    pub state: GameStateDto,
    pub extra: ArchiveExtra,
}
