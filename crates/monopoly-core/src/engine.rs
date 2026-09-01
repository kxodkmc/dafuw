use monopoly_common::avatar::{Avatar, Color};
use monopoly_common::command::Command;
use monopoly_common::event::{Event, PlayerRank};
use monopoly_common::room::RoomSettings;
use monopoly_common::snapshot::{AuctionStateDto, GameStateDto, PlayerDto, TileDto};
use uuid::Uuid;

use crate::auction::Auction;
use crate::board::Board;
use crate::cards::Deck;
use crate::deeds::{Deeds, DEFAULT_HOTELS, DEFAULT_HOUSES};
use crate::player::Player;
use crate::rng::RngSource;
use crate::trade::Trade;

/// 引擎阶段状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// 等待开局（可加入/改配置）。
    Lobby,
    /// 等待当前玩家掷骰。
    AwaitRoll,
    /// 停在空地，等待买/放弃。
    AwaitDecision,
    /// 银行拍卖中。
    InAuction,
    /// 对局结束。
    Over,
}

/// 大富翁核心引擎：权威状态 + 指令执行 + 事件产出。
///
/// 通过 [`GameEngine::handle`] 注入玩家指令与随机源；引擎不关心网络与身份来源。
pub struct GameEngine {
    pub settings: RoomSettings,
    pub(crate) board: Board,
    pub players: Vec<Player>,
    pub deeds: Deeds,
    pub(crate) chance: Deck,
    pub(crate) fate: Deck,
    pub(crate) current: usize,
    pub(crate) phase: Phase,
    pub(crate) dice: Vec<u8>,
    pub(crate) doubles_count: u8,
    pub(crate) pending_doubles: bool,
    pub(crate) turn_ended: bool,
    pub(crate) auction: Option<Auction>,
    /// 待拍卖的地产队列（破产清算触发，逐块拍卖）。
    pub(crate) auction_queue: Vec<usize>,
    pub(crate) pending_trades: Vec<Trade>,
    pub(crate) round: u32,
    pub(crate) winner: Option<Uuid>,
}

impl GameEngine {
    pub fn new(settings: RoomSettings) -> Result<Self, String> {
        settings.validate()?;
        let board = crate::board::build_board(settings.board_size as usize)?;
        let deeds = Deeds::new(board.len(), DEFAULT_HOUSES, DEFAULT_HOTELS);
        Ok(Self {
            settings,
            board,
            players: Vec::new(),
            deeds,
            chance: Deck::chance(),
            fate: Deck::fate(),
            current: 0,
            phase: Phase::Lobby,
            dice: Vec::new(),
            doubles_count: 0,
            pending_doubles: false,
            turn_ended: false,
            auction: None,
            auction_queue: Vec::new(),
            pending_trades: Vec::new(),
            round: 0,
            winner: None,
        })
    }

    /// 大厅阶段加入玩家（座位）。
    ///
    /// `name` 在房间内唯一；`avatar`/`color` 由玩家自选，房间内「形象 + 颜色」组合必须唯一；
    /// 形象相同但颜色不同（如红色狗 vs 绿色狗）视为不同组合，允许并存。
    pub fn add_player(
        &mut self,
        id: Uuid,
        name: String,
        avatar: Avatar,
        color: Color,
    ) -> Result<(), String> {
        if self.phase != Phase::Lobby {
            return Err("对局已开始，无法加入".into());
        }
        if self.players.len() as u8 >= self.settings.player_count_max {
            return Err("房间人数已满".into());
        }
        validate_name(&name)?;
        if self.players.iter().any(|p| p.id == id) {
            return Err("该玩家已在房间中".into());
        }
        if self.players.iter().any(|p| p.name == name) {
            return Err(format!("昵称已被占用：{name}"));
        }
        if self.players.iter().any(|p| p.avatar == avatar && p.color == color) {
            return Err(format!("该形象组合已被占用：{color}{avatar}"));
        }
        self.players.push(Player::new(
            id,
            name,
            avatar,
            color,
            self.settings.initial_money as i64,
        ));
        Ok(())
    }

    /// 大厅中修改昵称/形象/颜色（未提供的字段保持不变）。
    ///
    /// 昵称沿用加入时的格式校验（非空、≤16 字符、仅中文/字母/数字），
    /// 且与「昵称」「形象+颜色组合」的房间内唯一性约束保持一致。
    pub fn update_profile(
        &mut self,
        player: Uuid,
        name: Option<String>,
        avatar: Option<Avatar>,
        color: Option<Color>,
    ) -> Result<(), String> {
        if self.phase != Phase::Lobby {
            return Err("对局已开始，无法修改资料".into());
        }
        let cur = self.player(player).ok_or("玩家不在本房间")?.clone();
        let new_name = name.clone().unwrap_or(cur.name.clone());
        let new_avatar = avatar.unwrap_or(cur.avatar);
        let new_color = color.unwrap_or(cur.color);
        validate_name(&new_name)?;
        if self.players.iter().any(|q| q.id != player && q.name == new_name) {
            return Err(format!("昵称已被占用：{new_name}"));
        }
        if self
            .players
            .iter()
            .any(|q| q.id != player && q.avatar == new_avatar && q.color == new_color)
        {
            return Err(format!("该形象组合已被占用：{new_color}{new_avatar}"));
        }
        let p = self.player_mut(player).ok_or("玩家不在本房间")?;
        if let Some(n) = name {
            p.name = n.trim().to_string();
        }
        if let Some(a) = avatar {
            p.avatar = a;
        }
        if let Some(c) = color {
            p.color = c;
        }
        Ok(())
    }

    /// 大厅中标记/取消准备就绪。
    pub fn set_ready(&mut self, player: Uuid, ready: bool) -> Result<(), String> {
        if self.phase != Phase::Lobby {
            return Err("对局已开始，无法修改准备状态".into());
        }
        let p = self.player_mut(player).ok_or("玩家不在本房间")?;
        p.ready = ready;
        Ok(())
    }

    /// 开局：全员就绪后洗牌、洗乱玩家顺序（系统抽签决定行动顺序）、进入 AwaitRoll。
    pub fn start(&mut self, rng: &mut dyn RngSource) -> Result<Vec<Event>, String> {
        if self.phase != Phase::Lobby {
            return Err("对局已开始".into());
        }
        if self.players.len() < 2 {
            return Err("至少需要 2 名玩家才能开局".into());
        }
        if !self.players.iter().all(|p| p.ready) {
            return Err("所有玩家准备就绪后才能开局".into());
        }
        self.chance.shuffle(rng);
        self.fate.shuffle(rng);
        self.shuffle_players(rng);
        self.current = 0;
        self.phase = Phase::AwaitRoll;
        let first = self.players[0].id;
        Ok(vec![
            Event::GameStarted {
                first_player: first,
            },
            Event::TurnStarted { player_id: first },
        ])
    }

    fn shuffle_players(&mut self, rng: &mut dyn RngSource) {
        let n = self.players.len();
        for i in (1..n).rev() {
            let j = rng.next_below((i + 1) as u32) as usize;
            self.players.swap(i, j);
        }
    }

    /// 指令入口：校验身份/阶段后执行，返回要广播的事件流。
    pub fn handle(
        &mut self,
        player: Uuid,
        cmd: Command,
        rng: &mut dyn RngSource,
    ) -> Result<Vec<Event>, String> {
        if self.phase == Phase::Over {
            return Err("对局已结束".into());
        }
        let mut events = Vec::new();
        match cmd {
            Command::RollDice => {
                self.require_turn(player)?;
                self.do_roll(rng, &mut events)?;
            }
            Command::PayBail => {
                self.require_player(player)?;
                self.do_pay_bail(player, &mut events)?;
            }
            Command::UseOutOfJailCard => {
                self.require_player(player)?;
                self.do_use_card(player, &mut events)?;
            }
            Command::BuyProperty => {
                self.require_player(player)?;
                self.do_buy(player, &mut events)?;
            }
            Command::PassProperty => {
                self.require_player(player)?;
                self.do_pass(player, &mut events)?;
            }
            Command::AuctionBid { amount } => {
                self.do_bid(player, amount, &mut events)?;
            }
            Command::AuctionPass => {
                self.do_auction_pass(player, &mut events)?;
            }
            Command::BuildHouse { tile_id } => {
                self.require_player(player)?;
                self.do_build(player, tile_id, &mut events)?;
            }
            Command::SellHouse { tile_id } => {
                self.require_player(player)?;
                self.do_sell_house(player, tile_id, &mut events)?;
            }
            Command::Mortgage { tile_id } => {
                self.require_player(player)?;
                self.do_mortgage(player, tile_id, &mut events)?;
            }
            Command::Unmortgage { tile_id } => {
                self.require_player(player)?;
                self.do_unmortgage(player, tile_id, &mut events)?;
            }
            Command::ProposeTrade {
                target,
                offer,
                demand,
            } => {
                self.do_propose_trade(player, target, offer, demand, &mut events)?;
            }
            Command::AcceptTrade { trade_id } => {
                self.do_accept_trade(player, trade_id, &mut events)?;
            }
            Command::DeclineTrade { trade_id } => {
                self.do_decline_trade(player, trade_id, &mut events)?;
            }
            Command::UpdateProfile {
                name,
                avatar,
                color,
            } => {
                self.require_player(player)?;
                self.update_profile(player, name, avatar, color)?;
            }
            Command::SetReady { ready } => {
                self.set_ready(player, ready)?;
                events.push(Event::PlayerReady {
                    player_id: player,
                    ready,
                });
            }
            Command::StartGame => {
                return Err("开局请由房主在房间层发起".into());
            }
            Command::Chat { .. } => {
                return Err("聊天指令应由房间层处理".into());
            }
        }
        Ok(events)
    }

    // ---------- 校验与基础访问 ----------

    pub fn is_over(&self) -> bool {
        self.phase == Phase::Over
    }

    pub fn winner(&self) -> Option<Uuid> {
        self.winner
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// 拍卖中当前应出价的玩家（无拍卖时为 `None`）。
    pub fn auction_current_bidder(&self) -> Option<Uuid> {
        self.auction.as_ref().and_then(|a| a.current_bidder())
    }

    pub fn current_player_id(&self) -> Option<Uuid> {
        self.players.get(self.current).map(|p| p.id)
    }

    pub fn player(&self, id: Uuid) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn player_mut(&mut self, id: Uuid) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    pub fn player_cash(&self, id: Uuid) -> i64 {
        self.player(id).map(|p| p.cash).unwrap_or(0)
    }

    pub fn player_position(&self, id: Uuid) -> usize {
        self.player(id).map(|p| p.position).unwrap_or(0)
    }

    pub fn alive_players(&self) -> Vec<Uuid> {
        self.players
            .iter()
            .filter(|p| !p.bankrupt)
            .map(|p| p.id)
            .collect()
    }

    pub fn require_player(&self, player: Uuid) -> Result<(), String> {
        match self.player(player) {
            Some(p) if !p.bankrupt => Ok(()),
            Some(_) => Err("玩家已破产".into()),
            None => Err("玩家不在本房间".into()),
        }
    }

    fn require_turn(&self, player: Uuid) -> Result<(), String> {
        if self.phase == Phase::Lobby {
            return Err("对局尚未开始".into());
        }
        if self.players[self.current].bankrupt {
            return Err("当前玩家已破产，对局应已结束".into());
        }
        if self.current_player_id() != Some(player) {
            return Err("还没轮到你".into());
        }
        if self.phase != Phase::AwaitRoll {
            return Err("当前阶段无法掷骰".into());
        }
        Ok(())
    }

    // ---------- 资金与状态变更 ----------

    /// 变更现金并产出事件；返回变更后余额。
    pub fn change_cash(&mut self, player: Uuid, delta: i64, reason: &str, events: &mut Vec<Event>) -> i64 {
        let cash = if let Some(p) = self.player_mut(player) {
            p.cash += delta;
            p.cash
        } else {
            return 0;
        };
        events.push(Event::MoneyChanged {
            player_id: player,
            delta,
            reason: reason.to_string(),
        });
        cash
    }

    /// 净资产 = 现金 + 地产估值。
    pub fn net_worth(&self, player: Uuid) -> i64 {
        self.player_cash(player) + self.deeds.total_value(&self.board, player)
    }

    /// 玩家入狱。
    pub fn send_to_jail(&mut self, player: Uuid, events: &mut Vec<Event>) {
        let jail = self.board.jail_index();
        if let Some(p) = self.player_mut(player) {
            p.position = jail;
            p.in_jail = true;
            p.jail_turns = 0;
        }
        events.push(Event::WentToJail { player_id: player });
    }

    /// 末位名次（按净资产降序）。
    pub(crate) fn ranking(&self) -> Vec<PlayerRank> {
        let mut rank: Vec<PlayerRank> = self
            .players
            .iter()
            .map(|p| PlayerRank {
                player_id: p.id,
                name: p.name.clone(),
                net_worth: self.net_worth(p.id),
            })
            .collect();
        rank.sort_by(|a, b| b.net_worth.cmp(&a.net_worth));
        rank
    }

    /// 推进到下一位未破产玩家（轮次计数、限时结束判定）。
    pub fn advance_turn(&mut self, events: &mut Vec<Event>) {
        if self.phase == Phase::Over {
            return;
        }
        self.pending_doubles = false;
        self.doubles_count = 0;
        let n = self.players.len();
        let mut idx = self.current;
        for _ in 0..n {
            idx = (idx + 1) % n;
            if !self.players[idx].bankrupt {
                break;
            }
        }
        if self.players[idx].bankrupt {
            self.phase = Phase::Over;
            return;
        }
        if idx < self.current {
            self.round += 1;
        }
        self.current = idx;
        if let Some(max) = self.settings.max_rounds {
            if self.round >= max {
                self.end_by_rounds(events);
                return;
            }
        }
        self.phase = Phase::AwaitRoll;
        let next = self.players[idx].id;
        events.push(Event::TurnStarted { player_id: next });
    }

    /// 限时结束：净资产最高者胜。
    fn end_by_rounds(&mut self, events: &mut Vec<Event>) {
        self.phase = Phase::Over;
        let rank = self.ranking();
        let winner = rank
            .iter()
            .filter(|r| !self.player(r.player_id).map(|p| p.bankrupt).unwrap_or(true))
            .map(|r| r.player_id)
            .next()
            .or_else(|| rank.first().map(|r| r.player_id));
        if let Some(w) = winner {
            self.winner = Some(w);
            events.push(Event::GameEnded { winner: w, rank });
        }
    }

    /// 回合收尾：若仍处于掷骰阶段，则按对子规则决定是否换人。
    pub(crate) fn finish_turn_if_needed(&mut self, events: &mut Vec<Event>) {
        if self.phase != Phase::AwaitRoll {
            return;
        }
        if self.turn_ended {
            self.turn_ended = false;
            return;
        }
        if self.pending_doubles {
            self.pending_doubles = false;
        } else {
            self.advance_turn(events);
        }
    }

    // ---------- 快照 ----------

    pub fn snapshot(&self) -> GameStateDto {
        let tiles = self
            .board
            .tiles
            .iter()
            .map(|t| TileDto {
                id: t.id,
                name: t.name.clone(),
                kind: format!("{:?}", t.kind),
                group: t.group.map(|g| format!("{:?}", g)),
                price: t.price,
                owner: self.deeds.owner(t.id),
                houses: self.deeds.houses(t.id),
                mortgaged: self.deeds.is_mortgaged(t.id),
                base_rent: t.base_rent,
                rent_with_house: t.rent_with_house.to_vec(),
                hotel_rent: t.hotel_rent,
                building_cost: t.building_cost,
                mortgage_value: t.mortgage_value,
            })
            .collect();
        let players = self
            .players
            .iter()
            .map(|p| PlayerDto {
                id: p.id,
                name: p.name.clone(),
                avatar: p.avatar,
                color: p.color,
                ready: p.ready,
                cash: p.cash,
                position: p.position,
                in_jail: p.in_jail,
                jail_turns: p.jail_turns,
                out_of_jail_cards: p.out_of_jail_cards,
                bankrupt: p.bankrupt,
                net_worth: self.net_worth(p.id),
            })
            .collect();
        let (status, phase) = match self.phase {
            Phase::Lobby => ("lobby", "lobby"),
            Phase::Over => ("ended", "over"),
            Phase::AwaitRoll => ("playing", "await_roll"),
            Phase::AwaitDecision => ("playing", "decision"),
            Phase::InAuction => ("playing", "auction"),
        };
        let auction = self.auction.as_ref().map(|a| AuctionStateDto {
            tile_id: a.tile_id,
            highest_bid: a.highest_bid,
            highest_bidder: a.highest_bidder,
            current_bidder: a.current_bidder(),
            passed: a.passed_players().to_vec(),
        });
        GameStateDto {
            status: status.to_string(),
            current_player: self.current_player_id(),
            phase: phase.to_string(),
            dice_count: self.settings.dice_count,
            board_size: self.board.len(),
            go_salary: self.settings.go_salary,
            houses_available: self.deeds.houses_available(),
            hotels_available: self.deeds.hotels_available(),
            tiles,
            players,
            winner: self.winner,
            round: self.round,
            auction,
        }
    }
}

/// 昵称格式校验：非空、≤16 字符、仅中文/字母/数字（拒绝空格/标点/符号/表情）。
pub(crate) fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("昵称不能为空".into());
    }
    if name.chars().count() > 16 {
        return Err("昵称过长（最多 16 字符）".into());
    }
    if !name.chars().all(|c| c.is_alphanumeric()) {
        return Err("昵称只能包含中文、字母、数字".into());
    }
    Ok(())
}

/// 生成骰子点数。
pub(crate) fn roll_dice(count: u8, rng: &mut dyn RngSource) -> Vec<u8> {
    (0..count).map(|_| 1 + rng.next_below(6) as u8).collect()
}

/// 判断是否为对子。
pub(crate) fn is_double(dice: &[u8]) -> bool {
    dice.len() >= 2 && dice.iter().all(|&d| d == dice[0])
}

pub(crate) fn dice_sum(dice: &[u8]) -> u32 {
    dice.iter().map(|&d| d as u32).sum()
}

impl Default for GameEngine {
    fn default() -> Self {
        GameEngine::new(RoomSettings::default()).expect("默认配置必须合法")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use monopoly_common::command::Command;
    use monopoly_common::room::RoomSettings;
    use uuid::Uuid;

    /// 固定序列随机源：按序返回 `val % bound`，越界循环复用。
    struct SeqRng {
        vals: Vec<u32>,
        i: usize,
    }

    impl SeqRng {
        fn new(vals: Vec<u32>) -> Self {
            Self { vals, i: 0 }
        }
    }

    impl RngSource for SeqRng {
        fn next_below(&mut self, bound: u32) -> u32 {
            let v = self.vals[self.i % self.vals.len()];
            self.i += 1;
            v % bound
        }
    }

    fn two_player() -> (GameEngine, Uuid, Uuid) {
        let settings = RoomSettings {
            player_count_max: 2,
            ..Default::default()
        };
        let mut e = GameEngine::new(settings).unwrap();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let (avatar_a, color_a) = crate::tests::support::combo(0);
        let (avatar_b, color_b) = crate::tests::support::combo(1);
        e.add_player(a, "Alice".to_string(), avatar_a, color_a)
            .unwrap();
        e.add_player(b, "Bob".to_string(), avatar_b, color_b)
            .unwrap();
        crate::tests::support::mark_all_ready(&mut e);
        (e, a, b)
    }

    #[test]
    fn start_requires_all_ready() {
        let mut e = GameEngine::new(RoomSettings::default()).unwrap();
        let (avatar_a, color_a) = crate::tests::support::combo(0);
        let (avatar_b, color_b) = crate::tests::support::combo(1);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        e.add_player(a, "Alice".to_string(), avatar_a, color_a)
            .unwrap();
        e.add_player(b, "Bob".to_string(), avatar_b, color_b)
            .unwrap();

        // 无人就绪 → 拒绝开局。
        assert!(e.start(&mut SeqRng::new(vec![0])).is_err());
        // 仅一人就绪 → 仍拒绝。
        e.set_ready(a, true).unwrap();
        assert!(e.start(&mut SeqRng::new(vec![0])).is_err());
        // 全员就绪 → 开局成功（系统抽签决定行动顺序）。
        e.set_ready(b, true).unwrap();
        assert!(e.start(&mut SeqRng::new(vec![0])).is_ok());
    }

    #[test]
    fn set_ready_reflects_in_snapshot_and_is_lobby_only() {
        let (mut e, a, b) = two_player();
        // 取消全部就绪 → 快照反映。
        e.set_ready(a, false).unwrap();
        e.set_ready(b, false).unwrap();
        assert!(e.snapshot().players.iter().all(|p| !p.ready));

        e.set_ready(a, true).unwrap();
        assert!(e.player(a).unwrap().ready);
        assert!(!e.player(b).unwrap().ready);

        // 全员就绪后开局；开局后就绪状态不可再修改。
        e.set_ready(b, true).unwrap();
        e.start(&mut SeqRng::new(vec![0])).unwrap();
        assert!(e.set_ready(a, false).is_err());
    }

    #[test]
    fn update_profile_validates_and_applies() {
        let (mut e, a, b) = two_player();
        let bob = e.player(b).unwrap().clone();

        // 只改昵称：合法并生效。
        e.update_profile(a, Some("小明".to_string()), None, None)
            .unwrap();
        assert_eq!(e.player(a).unwrap().name, "小明");

        // 特殊字符/超长昵称被拒绝。
        assert!(e
            .update_profile(a, Some("小明!".to_string()), None, None)
            .is_err());
        assert!(e
            .update_profile(a, Some("a".repeat(17)), None, None)
            .is_err());

        // 与其它玩家重名被拒绝。
        assert!(e
            .update_profile(a, Some(bob.name.clone()), None, None)
            .is_err());

        // 形象+颜色组合与其它玩家冲突被拒绝。
        assert!(e
            .update_profile(a, None, Some(bob.avatar), Some(bob.color))
            .is_err());

        // 同形象不同颜色允许（组合(5) 取同形象、不同颜色）。
        let (_, other_color) = crate::tests::support::combo(5);
        assert!(other_color != bob.color);
        assert!(e
            .update_profile(a, None, Some(bob.avatar), Some(other_color))
            .is_ok());
        assert_eq!(e.player(a).unwrap().avatar, bob.avatar);
        assert_eq!(e.player(a).unwrap().color, other_color);
    }

    #[test]
    fn start_requires_at_least_two_players() {
        let mut e = GameEngine::new(RoomSettings::default()).unwrap();
        let (avatar, color) = crate::tests::support::combo(0);
        e.add_player(Uuid::new_v4(), "Alice".to_string(), avatar, color)
            .unwrap();
        assert!(e.start(&mut SeqRng::new(vec![0])).is_err());
    }

    #[test]
    fn player_name_must_be_unique() {
        let mut e = GameEngine::new(RoomSettings::default()).unwrap();
        let (avatar, color) = crate::tests::support::combo(0);
        e.add_player(Uuid::new_v4(), "Alice".to_string(), avatar, color)
            .unwrap();

        // 同名玩家被拒绝（即使形象组合不同）。
        let (avatar2, color2) = crate::tests::support::combo(1);
        assert!(e
            .add_player(Uuid::new_v4(), "Alice".to_string(), avatar2, color2)
            .is_err());

        // 不同名玩家正常加入。
        assert!(e
            .add_player(Uuid::new_v4(), "Bob".to_string(), avatar2, color2)
            .is_ok());
    }

    #[test]
    fn avatar_color_combo_must_be_unique() {
        let mut e = GameEngine::new(RoomSettings::default()).unwrap();
        let (avatar, color) = crate::tests::support::combo(0);
        e.add_player(Uuid::new_v4(), "Alice".to_string(), avatar, color)
            .unwrap();

        // 完全相同的组合被拒绝。
        assert!(e
            .add_player(Uuid::new_v4(), "Bob".to_string(), avatar, color)
            .is_err());

        // 同形象不同颜色：允许（红色狗 vs 绿色狗）。
        let (_, other_color) = crate::tests::support::combo(5);
        assert!(other_color != color);
        assert!(e
            .add_player(Uuid::new_v4(), "Carol".to_string(), avatar, other_color)
            .is_ok());

        // 不同形象同颜色：允许。
        let (other_avatar, _) = crate::tests::support::combo(1);
        assert!(other_avatar != avatar);
        assert!(e
            .add_player(Uuid::new_v4(), "Dave".to_string(), other_avatar, color)
            .is_ok());
    }

    #[test]
    fn buy_property_then_turn_passes() {
        let (mut e, a, b) = two_player();
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();
        let first = e.current_player_id().unwrap();
        let other = if first == a { b } else { a };

        let events = e
            .handle(first, Command::RollDice, &mut SeqRng::new(vec![0, 3]))
            .unwrap();
        assert!(events
            .iter()
            .any(|ev| matches!(ev, Event::LandedOn { tile_id: 5, .. })));
        let ev = e
            .handle(first, Command::BuyProperty, &mut SeqRng::new(vec![]))
            .unwrap();
        assert!(ev
            .iter()
            .any(|x| matches!(x, Event::PropertyBought { tile_id: 5, .. })));
        assert_eq!(e.current_player_id(), Some(other));
        assert_eq!(e.deeds.owner(5), Some(first));
        assert_eq!(e.player_cash(first), 15_000_000 - 2_000_000);
    }

    #[test]
    fn auction_flow() {
        let (mut e, a, b) = two_player();
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();
        let first = e.current_player_id().unwrap();
        let other = if first == a { b } else { a };

        let _ = e
            .handle(first, Command::RollDice, &mut SeqRng::new(vec![0, 3]))
            .unwrap();
        let _ = e
            .handle(first, Command::PassProperty, &mut SeqRng::new(vec![]))
            .unwrap();
        assert_eq!(e.phase, Phase::InAuction);

        let _ = e
            .handle(
                first,
                Command::AuctionBid { amount: 1_000_000 },
                &mut SeqRng::new(vec![]),
            )
            .unwrap();
        let _ = e
            .handle(
                other,
                Command::AuctionBid { amount: 1_500_000 },
                &mut SeqRng::new(vec![]),
            )
            .unwrap();
        let _ = e
            .handle(
                first,
                Command::AuctionBid { amount: 2_000_000 },
                &mut SeqRng::new(vec![]),
            )
            .unwrap();
        let _ = e
            .handle(other, Command::AuctionPass, &mut SeqRng::new(vec![]))
            .unwrap();

        assert_eq!(e.deeds.owner(5), Some(first));
        assert_eq!(e.player_cash(first), 15_000_000 - 2_000_000);
        assert_ne!(e.phase, Phase::InAuction);
    }

    #[test]
    fn rent_bankruptcy_ends_game() {
        let (mut e, a, b) = two_player();
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();
        let first = e.current_player_id().unwrap();
        let other = if first == a { b } else { a };

        let mut ev = Vec::new();
        e.advance_turn(&mut ev); // 轮到 other
        e.deeds.assign(5, first); // first 拥有铁路 5（租金 250K）
        e.change_cash(other, -14_990_000, "test", &mut ev); // other 只剩 100_000

        let events = e
            .handle(other, Command::RollDice, &mut SeqRng::new(vec![0, 3]))
            .unwrap();
        assert!(events
            .iter()
            .any(|x| matches!(x, Event::PlayerBroke { .. })));
        assert!(events.iter().any(|x| matches!(x, Event::GameEnded { .. })));
        assert_eq!(e.winner(), Some(first));
        assert!(e.is_over());
        assert!(e.player(other).unwrap().bankrupt);
    }

    /// 官方规则：欠银行破产 → 全部地产（房屋归还库存、抵押解除）进入拍卖，逐块成交。
    #[test]
    fn bankrupt_to_bank_auctions_properties() {
        let (mut e, ids) = crate::tests::support::engine_with_players(3, None);
        let (a, _b, _c) = (ids[0], ids[1], ids[2]);
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();

        // a 拥有棕组（1、3 号），在 1 号建 1 栋房，3 号抵押。
        e.deeds.assign(1, a);
        e.deeds.assign(3, a);
        let _ = e.deeds.build(&e.board, 1).unwrap();
        let _ = e.deeds.mortgage(&e.board, 3).unwrap();
        assert_eq!(e.deeds.houses_available(), 31);

        // 欠银行巨额债务 → 破产，地产进入拍卖。
        let mut all_ev = Vec::new();
        e.pay_to(a, None, 20_000_000, "test", &mut all_ev);
        assert!(e.player(a).unwrap().bankrupt);
        assert!(all_ev
            .iter()
            .any(|x| matches!(x, Event::PlayerBroke { .. })));
        // 地块无主、房屋归还银行库存、抵押解除，进入第一场拍卖。
        assert_eq!(e.phase(), Phase::InAuction);
        assert_eq!(e.deeds.owner(1), None);
        assert_eq!(e.deeds.owner(3), None);
        assert_eq!(e.deeds.houses_available(), 32);
        assert!(!e.deeds.is_mortgaged(3));

        // 中途存档/恢复应保留待拍卖队列（重启续玩）。
        let restored = GameEngine::restore(&e.archive()).unwrap();
        assert_eq!(e.archive(), restored.archive());

        // 第一场（1 号）：当前竞拍者出价 $1，另一存活玩家弃权 → 成交并自动衔接下一场。
        let bidder = e.auction_current_bidder().unwrap();
        e.handle(
            bidder,
            Command::AuctionBid { amount: 1 },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
        let passer = e.auction_current_bidder().unwrap();
        let ev = e
            .handle(passer, Command::AuctionPass, &mut SeqRng::new(vec![]))
            .unwrap();
        all_ev.extend(ev);
        assert!(e.deeds.owner(1).is_some());
        assert_eq!(e.phase(), Phase::InAuction); // 排队中的 3 号已自动开启

        // 第二场（3 号）：同样流程，成交后回到掷骰阶段。
        let bidder = e.auction_current_bidder().unwrap();
        e.handle(
            bidder,
            Command::AuctionBid { amount: 1 },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
        let passer = e.auction_current_bidder().unwrap();
        let ev = e
            .handle(passer, Command::AuctionPass, &mut SeqRng::new(vec![]))
            .unwrap();
        all_ev.extend(ev);
        assert!(e.deeds.owner(3).is_some());
        assert_eq!(e.phase(), Phase::AwaitRoll);
        assert_eq!(
            all_ev
                .iter()
                .filter(|x| matches!(x, Event::AuctionStarted { .. }))
                .count(),
            2
        );
        assert_eq!(
            all_ev
                .iter()
                .filter(|x| matches!(x, Event::AuctionEnded { .. }))
                .count(),
            2
        );
        // 全部成交后回到掷骰阶段，两块地归存活玩家且无人破产。
        assert_eq!(e.phase(), Phase::AwaitRoll);
        for tile in [1, 3] {
            let owner = e.deeds.owner(tile).unwrap();
            assert!(!e.player(owner).unwrap().bankrupt);
        }
    }

    /// 官方规则：欠玩家破产 → 房屋/旅馆半价变卖归债权人，地产连同抵押状态移交。
    #[test]
    fn bankrupt_to_player_transfers_estate() {
        let (mut e, ids) = crate::tests::support::engine_with_players(3, None);
        let (a, b, _c) = (ids[0], ids[1], ids[2]);
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();

        e.deeds.assign(1, a);
        e.deeds.assign(3, a);
        // 注意：测试直接操作 deeds 层建/押，不通过 do_build/do_mortgage，a 的现金不变。
        let _ = e.deeds.build(&e.board, 1).unwrap();
        let _ = e.deeds.mortgage(&e.board, 3).unwrap();
        assert_eq!(e.deeds.houses_available(), 31);

        // a 欠 b 巨额债务 → 破产：a 全部现金（15M）、房屋变卖所得（250K）归 b，地产移交 b。
        let b_cash_before = e.player_cash(b);
        let mut ev = Vec::new();
        e.pay_to(a, Some(b), 20_000_000, "test", &mut ev);

        assert!(e.player(a).unwrap().bankrupt);
        assert_eq!(e.player_cash(a), 0);
        assert!(ev.iter().any(|x| matches!(x, Event::PlayerBroke { .. })));
        assert!(!ev.iter().any(|x| matches!(x, Event::AuctionStarted { .. })));
        assert_eq!(e.player_cash(b), b_cash_before + 15_000_000 + 250_000);
        // 地产（含抵押状态）移交 b，房屋已变卖且库存归还。
        assert_eq!(e.deeds.owner(1), Some(b));
        assert_eq!(e.deeds.owner(3), Some(b));
        assert!(e.deeds.is_mortgaged(3));
        assert_eq!(e.deeds.houses(1), 0);
        assert_eq!(e.deeds.houses_available(), 32);
        assert_ne!(e.phase(), Phase::InAuction);
    }
}
