use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::command::AssetList;
use super::snapshot::GameStateDto;

/// 最终名次条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerRank {
    pub player_id: Uuid,
    pub name: String,
    pub net_worth: i64,
}

/// 聊天消息载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatPayload {
    pub player_id: Option<Uuid>,
    pub name: String,
    pub text: String,
}

/// 服务器 → 客户端的领域事件（广播）。
///
/// 客户端只根据事件做展示，不做任何本地推演，保证状态始终与权威服务器一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// 完整房间快照（入房/断线重连/每次状态变更后都会补发）。
    RoomSnapshot(GameStateDto),
    /// 面向单个玩家的错误提示。
    Error {
        code: String,
        message: String,
    },
    /// 房间内系统消息。
    Message {
        text: String,
    },
    Chat(ChatPayload),
    PlayerJoined {
        player_id: Uuid,
        name: String,
    },
    PlayerLeft {
        player_id: Uuid,
    },
    /// 大厅中某位玩家标记/取消准备就绪。
    PlayerReady {
        player_id: Uuid,
        ready: bool,
    },
    GameStarted {
        first_player: Uuid,
    },
    TurnStarted {
        player_id: Uuid,
    },
    DiceRolled {
        player_id: Uuid,
        dice: Vec<u8>,
        doubles: bool,
        doubles_count: u8,
        new_position: usize,
    },
    PlayerMoved {
        player_id: Uuid,
        from: usize,
        to: usize,
    },
    PassedGo {
        player_id: Uuid,
        salary: u32,
    },
    LandedOn {
        player_id: Uuid,
        tile_id: usize,
    },
    PropertyBought {
        player_id: Uuid,
        tile_id: usize,
        price: u32,
    },
    RentPaid {
        from: Uuid,
        to: Uuid,
        amount: u32,
        tile_id: usize,
    },
    CardDrawn {
        player_id: Uuid,
        card_text: String,
    },
    CardApplied {
        player_id: Uuid,
        text: String,
    },
    TaxPaid {
        player_id: Uuid,
        amount: u32,
        tile_id: usize,
    },
    WentToJail {
        player_id: Uuid,
    },
    JailReleased {
        player_id: Uuid,
    },
    JailTurnSkipped {
        player_id: Uuid,
    },
    BuildHouse {
        player_id: Uuid,
        tile_id: usize,
        houses: u8,
    },
    SoldHouse {
        player_id: Uuid,
        tile_id: usize,
        houses: u8,
    },
    Mortgaged {
        player_id: Uuid,
        tile_id: usize,
        amount: u32,
    },
    Unmortgaged {
        player_id: Uuid,
        tile_id: usize,
        cost: u32,
    },
    AuctionStarted {
        tile_id: usize,
    },
    AuctionBid {
        player_id: Uuid,
        amount: u32,
    },
    AuctionPassed {
        player_id: Uuid,
    },
    AuctionEnded {
        tile_id: usize,
        winner: Option<Uuid>,
        amount: u32,
    },
    TradeProposed {
        trade_id: Uuid,
        from: Uuid,
        to: Uuid,
        offer: AssetList,
        demand: AssetList,
    },
    TradeAccepted {
        trade_id: Uuid,
    },
    TradeDeclined {
        trade_id: Uuid,
    },
    MoneyChanged {
        player_id: Uuid,
        delta: i64,
    },
    PlayerBroke {
        player_id: Uuid,
        creditor: Option<Uuid>,
    },
    GameEnded {
        winner: Uuid,
        rank: Vec<PlayerRank>,
    },
    RoomClosed,
}
