use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
}

/// 玩家快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerDto {
    pub id: Uuid,
    pub name: String,
    pub cash: i64,
    pub position: usize,
    pub in_jail: bool,
    pub jail_turns: u8,
    pub out_of_jail_cards: u8,
    pub bankrupt: bool,
    pub net_worth: i64,
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
}
