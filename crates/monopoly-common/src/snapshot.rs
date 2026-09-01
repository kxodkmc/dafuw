use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::avatar::{Avatar, Color};

/// 地块快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileDto {
    pub id: usize,
    pub name: String,
    pub kind: String,
    pub group: Option<String>,
    pub price: u32,
    pub owner: Option<Uuid>,
    pub houses: u8,
    pub mortgaged: bool,
    /// 空地租金（整套垄断时 ×2，由前端展示）。
    pub base_rent: u32,
    /// 1~4 栋房屋的租金。
    pub rent_with_house: Vec<u32>,
    /// 旅馆租金。
    pub hotel_rent: u32,
    /// 盖房成本（每栋）。
    pub building_cost: u32,
    /// 抵押可得金额。
    pub mortgage_value: u32,
}

/// 玩家快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerDto {
    pub id: Uuid,
    pub name: String,
    /// 自选形象。
    pub avatar: Avatar,
    /// 自选颜色。
    pub color: Color,
    /// 大厅中是否已准备就绪。
    pub ready: bool,
    pub cash: i64,
    pub position: usize,
    pub in_jail: bool,
    pub jail_turns: u8,
    pub out_of_jail_cards: u8,
    pub bankrupt: bool,
    pub net_worth: i64,
}

/// 进行中拍卖的公开状态（无拍卖时快照中为空）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuctionStateDto {
    pub tile_id: usize,
    pub highest_bid: u32,
    pub highest_bidder: Option<Uuid>,
    /// 当前应出价/弃权的玩家。
    pub current_bidder: Option<Uuid>,
    /// 已弃权的玩家（弃权后不可再参与）。
    pub passed: Vec<Uuid>,
}

/// 整局游戏快照，客户端以此重建界面。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameStateDto {
    /// lobby / playing / ended。
    pub status: String,
    /// 当前回合玩家。
    pub current_player: Option<Uuid>,
    /// await_roll / decision / auction / over。
    pub phase: String,
    pub dice_count: u8,
    pub board_size: usize,
    pub go_salary: u32,
    pub houses_available: u16,
    pub hotels_available: u16,
    pub tiles: Vec<TileDto>,
    pub players: Vec<PlayerDto>,
    pub winner: Option<Uuid>,
    pub round: u32,
    /// 进行中的拍卖（客户端据此渲染出价弹窗与轮次）。
    pub auction: Option<AuctionStateDto>,
}
