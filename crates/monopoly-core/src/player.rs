use monopoly_common::avatar::{Avatar, Color};
use monopoly_common::snapshot::PlayerDto;
use uuid::Uuid;

/// 玩家运行时状态。
#[derive(Debug, Clone)]
pub struct Player {
    pub id: Uuid,
    pub name: String,
    /// 自选形象。
    pub avatar: Avatar,
    /// 自选颜色。
    pub color: Color,
    pub cash: i64,
    pub position: usize,
    pub in_jail: bool,
    /// 已在狱中停留的回合数。
    pub jail_turns: u8,
    pub out_of_jail_cards: u8,
    pub bankrupt: bool,
}

impl Player {
    pub fn new(id: Uuid, name: String, avatar: Avatar, color: Color, cash: i64) -> Self {
        Self {
            id,
            name,
            avatar,
            color,
            cash,
            position: 0,
            in_jail: false,
            jail_turns: 0,
            out_of_jail_cards: 0,
            bankrupt: false,
        }
    }

    /// 从快照 DTO 重建玩家（对局恢复用）。
    pub(crate) fn restore(dto: &PlayerDto) -> Self {
        Self {
            id: dto.id,
            name: dto.name.clone(),
            avatar: dto.avatar,
            color: dto.color,
            cash: dto.cash,
            position: dto.position,
            in_jail: dto.in_jail,
            jail_turns: dto.jail_turns,
            out_of_jail_cards: dto.out_of_jail_cards,
            bankrupt: dto.bankrupt,
        }
    }
}
