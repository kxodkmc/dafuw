//! 对局存档与恢复：把引擎导出为可序列化的 [`GameArchive`]，并据此精确重建。
//!
//! 棋盘、租金表、地产登记与卡组内容均由配置确定性重建，再覆盖归属/房屋/
//! 抵押、玩家与回合内部状态（含拍卖、交易、卡组洗牌顺序），保证重启后续玩
//! 分毫不差。独立成模块，避免撑大 [`GameEngine`] 状态机文件。

use monopoly_common::archive::{ArchiveExtra, GameArchive};

use crate::auction::Auction;
use crate::engine::{GameEngine, Phase};
use crate::player::Player;
use crate::trade::Trade;

impl GameEngine {
    /// 导出完整存档（快照 + 内部状态），供持久化。
    pub fn archive(&self) -> GameArchive {
        GameArchive {
            settings: self.settings.clone(),
            state: self.snapshot(),
            extra: ArchiveExtra {
                dice: self.dice.clone(),
                doubles_count: self.doubles_count,
                pending_doubles: self.pending_doubles,
                turn_ended: self.turn_ended,
                auction: self.auction.as_ref().map(Auction::to_dto),
                pending_trades: self.pending_trades.iter().map(Trade::to_dto).collect(),
                chance_order: self.chance.order_clone(),
                chance_draw_index: self.chance.draw_index(),
                fate_order: self.fate.order_clone(),
                fate_draw_index: self.fate.draw_index(),
            },
        }
    }

    /// 从存档精确重建一局（服务器重启续玩用）。
    pub fn restore(archive: &GameArchive) -> Result<Self, String> {
        archive.settings.validate()?;
        let mut engine = GameEngine::new(archive.settings.clone())?;
        engine.restore_from(archive);
        Ok(engine)
    }

    fn restore_from(&mut self, archive: &GameArchive) {
        let state = &archive.state;
        let extra = &archive.extra;
        self.players = state.players.iter().map(Player::restore).collect();
        self.deeds
            .restore(&state.tiles, state.houses_available, state.hotels_available);
        self.chance
            .restore(&extra.chance_order, extra.chance_draw_index);
        self.fate.restore(&extra.fate_order, extra.fate_draw_index);
        self.current = state
            .current_player
            .and_then(|id| self.players.iter().position(|p| p.id == id))
            .unwrap_or(0);
        self.round = state.round;
        self.winner = state.winner;
        self.phase = match state.phase.as_str() {
            "lobby" => Phase::Lobby,
            "decision" => Phase::AwaitDecision,
            "auction" => Phase::InAuction,
            "over" => Phase::Over,
            _ => Phase::AwaitRoll,
        };
        self.dice = extra.dice.clone();
        self.doubles_count = extra.doubles_count;
        self.pending_doubles = extra.pending_doubles;
        self.turn_ended = extra.turn_ended;
        self.auction = extra.auction.as_ref().map(Auction::restore);
        self.pending_trades = extra.pending_trades.iter().map(Trade::restore).collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::support::{engine_with_players, SeqRng};
    use monopoly_common::command::Command;

    #[test]
    fn archive_restore_roundtrip_preserves_state() {
        let (mut e, ids) = engine_with_players(2, None);
        let (a, b) = (ids[0], ids[1]);
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();
        let first = e.current_player_id().unwrap();
        let other = if first == a { b } else { a };

        // 若干操作：掷骰、买地（含资金/归属变化）。
        let _ = e
            .handle(first, Command::RollDice, &mut SeqRng::new(vec![0, 3]))
            .unwrap();
        let _ = e
            .handle(first, Command::BuyProperty, &mut SeqRng::new(vec![]))
            .unwrap();
        assert_eq!(e.current_player_id(), Some(other));

        let archived = e.archive();
        let restored = GameEngine::restore(&archived).unwrap();
        // 存档 → 恢复 → 再存档，应完全一致（精确续玩）。
        assert_eq!(archived, restored.archive());
        assert_eq!(restored.current_player_id(), e.current_player_id());
        assert_eq!(restored.phase(), e.phase());
        assert_eq!(restored.player_cash(first), e.player_cash(first));
    }

    #[test]
    fn archive_restore_preserves_mid_auction() {
        let (mut e, ids) = engine_with_players(2, None);
        let (a, b) = (ids[0], ids[1]);
        let _ = e.start(&mut SeqRng::new(vec![0])).unwrap();
        let first = e.current_player_id().unwrap();
        let other = if first == a { b } else { a };

        // 放弃购买 → 银行拍卖，双方各出价一次，停留在拍卖中途。
        let _ = e
            .handle(first, Command::RollDice, &mut SeqRng::new(vec![0, 3]))
            .unwrap();
        let _ = e
            .handle(first, Command::PassProperty, &mut SeqRng::new(vec![]))
            .unwrap();
        assert_eq!(e.phase(), Phase::InAuction);
        let _ = e
            .handle(
                first,
                Command::AuctionBid { amount: 100 },
                &mut SeqRng::new(vec![]),
            )
            .unwrap();
        let _ = e
            .handle(
                other,
                Command::AuctionBid { amount: 150 },
                &mut SeqRng::new(vec![]),
            )
            .unwrap();

        let archived = e.archive();
        let restored = GameEngine::restore(&archived).unwrap();
        assert_eq!(archived, restored.archive());
        assert_eq!(restored.phase(), Phase::InAuction);
        // 拍卖中间状态（当前出价者）应原样恢复。
        assert_eq!(
            restored.auction_current_bidder(),
            e.auction_current_bidder()
        );
    }
}
