use uuid::Uuid;

/// 玩家运行时状态。
#[derive(Debug, Clone)]
pub struct Player {
    pub id: Uuid,
    pub name: String,
    pub cash: i64,
    pub position: usize,
    pub in_jail: bool,
    /// 已在狱中停留的回合数。
    pub jail_turns: u8,
    pub out_of_jail_cards: u8,
    pub bankrupt: bool,
}

impl Player {
    pub fn new(id: Uuid, name: String, cash: i64) -> Self {
        Self {
            id,
            name,
            cash,
            position: 0,
            in_jail: false,
            jail_turns: 0,
            out_of_jail_cards: 0,
            bankrupt: false,
        }
    }
}
