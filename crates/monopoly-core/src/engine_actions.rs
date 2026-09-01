//! 引擎的资产操作实现：购买/拍卖/建房/抵押/保释/交易/破产清算。
//!
//! 所有方法都是 `GameEngine` 的 `impl` 块，通过 `pub(crate)` 字段与 engine.rs 共享状态。

use monopoly_common::command::AssetList;
use monopoly_common::event::Event;
use uuid::Uuid;

use crate::auction::Auction;
use crate::engine::{GameEngine, Phase};

impl GameEngine {
    /// 支付 `amount` 给银行（`to == None`）或某玩家；资金不足则直接破产清算。
    pub(crate) fn pay_to(
        &mut self,
        from: Uuid,
        to: Option<Uuid>,
        amount: u32,
        events: &mut Vec<Event>,
    ) {
        if amount == 0 {
            return;
        }
        let cash = self.player_cash(from);
        if cash >= amount as i64 {
            self.change_cash(from, -(amount as i64), events);
            if let Some(t) = to {
                self.change_cash(t, amount as i64, events);
            }
            return;
        }
        // 资金不足：尽力支付后破产，全部资产移交给债权人。
        if cash > 0 {
            self.change_cash(from, -cash, events);
            if let Some(t) = to {
                self.change_cash(t, cash, events);
            }
        }
        self.bankrupt(from, to, events);
    }

    /// 破产清算：资产移交债权人；若债权人是银行则地产回归市场。
    pub(crate) fn bankrupt(&mut self, pid: Uuid, creditor: Option<Uuid>, events: &mut Vec<Event>) {
        if let Some(cred) = creditor {
            self.deeds.transfer_player(pid, cred);
            let cards = self.player(pid).map(|p| p.out_of_jail_cards).unwrap_or(0);
            if let Some(p) = self.player_mut(pid) {
                p.out_of_jail_cards = 0;
            }
            if let Some(p) = self.player_mut(cred) {
                p.out_of_jail_cards += cards;
            }
        } else {
            self.deeds.clear_player(pid);
        }
        if let Some(p) = self.player_mut(pid) {
            p.bankrupt = true;
        }
        events.push(Event::PlayerBroke {
            player_id: pid,
            creditor,
        });

        let alive = self.alive_players();
        match alive.len() {
            0 => self.phase = Phase::Over,
            1 => {
                self.phase = Phase::Over;
                let winner = alive[0];
                self.winner = Some(winner);
                let rank = self.ranking();
                events.push(Event::GameEnded { winner, rank });
            }
            _ => {
                // 当前玩家破产：直接换人，避免 finish_turn 二次推进。
                if self.current_player_id() == Some(pid) {
                    self.turn_ended = true;
                    self.advance_turn(events);
                }
            }
        }
    }

    /// 支付 $50 保释。
    pub(crate) fn do_pay_bail(&mut self, pid: Uuid, events: &mut Vec<Event>) -> Result<(), String> {
        let in_jail = self.player(pid).map(|p| p.in_jail).unwrap_or(false);
        if !in_jail {
            return Err("你不在狱中".into());
        }
        self.pay_to(pid, None, 50, events);
        if !self.player(pid).map(|p| p.bankrupt).unwrap_or(true) {
            if let Some(p) = self.player_mut(pid) {
                p.in_jail = false;
                p.jail_turns = 0;
            }
            events.push(Event::JailReleased { player_id: pid });
        }
        Ok(())
    }

    /// 使用出狱卡。
    pub(crate) fn do_use_card(&mut self, pid: Uuid, events: &mut Vec<Event>) -> Result<(), String> {
        let p = self.player(pid).ok_or("玩家不存在")?;
        if !p.in_jail {
            return Err("你不在狱中".into());
        }
        if p.out_of_jail_cards == 0 {
            return Err("你没有出狱卡".into());
        }
        if let Some(p) = self.player_mut(pid) {
            p.out_of_jail_cards -= 1;
            p.in_jail = false;
            p.jail_turns = 0;
        }
        events.push(Event::JailReleased { player_id: pid });
        Ok(())
    }

    /// 购买停留的空地。
    pub(crate) fn do_buy(&mut self, pid: Uuid, events: &mut Vec<Event>) -> Result<(), String> {
        if self.phase != Phase::AwaitDecision {
            return Err("当前无法购买".into());
        }
        let pos = self.player_position(pid);
        let tile = self.board.tile(pos).clone();
        if tile.price == 0 {
            return Err("该地块不可购买".into());
        }
        if self.deeds.owner(pos).is_some() {
            return Err("该地块已被拥有".into());
        }
        if self.player_cash(pid) < tile.price as i64 {
            return Err("资金不足，可先出售/抵押资产，或放弃购买触发拍卖".into());
        }
        self.change_cash(pid, -(tile.price as i64), events);
        self.deeds.assign(pos, pid);
        events.push(Event::PropertyBought {
            player_id: pid,
            tile_id: pos,
            price: tile.price,
        });
        self.phase = Phase::AwaitRoll;
        self.finish_turn_if_needed(events);
        Ok(())
    }

    /// 放弃购买，发起银行拍卖。
    pub(crate) fn do_pass(&mut self, pid: Uuid, events: &mut Vec<Event>) -> Result<(), String> {
        if self.phase != Phase::AwaitDecision {
            return Err("当前无需放弃购买".into());
        }
        let pos = self.player_position(pid);
        let tile = self.board.tile(pos);
        if tile.price == 0 {
            return Err("该地块不可购买".into());
        }
        if self.deeds.owner(pos).is_some() {
            return Err("该地块已被拥有".into());
        }
        let order = self.alive_players();
        self.auction = Some(Auction::new(pos, order, pid));
        self.phase = Phase::InAuction;
        events.push(Event::AuctionStarted { tile_id: pos });
        Ok(())
    }

    pub(crate) fn do_bid(
        &mut self,
        pid: Uuid,
        amount: u32,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        if self.phase != Phase::InAuction {
            return Err("当前没有拍卖".into());
        }
        let auction = self.auction.as_mut().ok_or("当前没有拍卖")?;
        auction.bid(pid, amount)?;
        events.push(Event::AuctionBid {
            player_id: pid,
            amount,
        });
        self.try_finish_auction(events)
    }

    pub(crate) fn do_auction_pass(
        &mut self,
        pid: Uuid,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        if self.phase != Phase::InAuction {
            return Err("当前没有拍卖".into());
        }
        let auction = self.auction.as_mut().ok_or("当前没有拍卖")?;
        auction.pass(pid)?;
        events.push(Event::AuctionPassed { player_id: pid });
        self.try_finish_auction(events)
    }

    fn try_finish_auction(&mut self, events: &mut Vec<Event>) -> Result<(), String> {
        let finished = self
            .auction
            .as_ref()
            .map(|a| a.is_finished())
            .unwrap_or(false);
        if !finished {
            return Ok(());
        }
        let auction = self.auction.take().unwrap();
        let tile = auction.tile_id;
        if let Some(winner) = auction.winner() {
            let amount = auction.highest_bid;
            self.pay_to(winner, None, amount, events);
            if !self.player(winner).map(|p| p.bankrupt).unwrap_or(true) {
                self.deeds.assign(tile, winner);
                events.push(Event::AuctionEnded {
                    tile_id: tile,
                    winner: Some(winner),
                    amount,
                });
            } else {
                events.push(Event::AuctionEnded {
                    tile_id: tile,
                    winner: None,
                    amount,
                });
            }
        } else {
            events.push(Event::AuctionEnded {
                tile_id: tile,
                winner: None,
                amount: 0,
            });
        }
        // 拍卖中可能有人破产导致对局结束（phase 被设为 Over），不可再强行推进回合。
        if self.phase != Phase::Over {
            self.phase = Phase::AwaitRoll;
            self.finish_turn_if_needed(events);
        }
        Ok(())
    }

    pub(crate) fn do_build(
        &mut self,
        pid: Uuid,
        tile_id: usize,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        self.require_player(pid)?;
        if self.deeds.owner(tile_id) != Some(pid) {
            return Err("只能在自己的地块建房".into());
        }
        let cost = self.board.tile(tile_id).building_cost;
        if self.player_cash(pid) < cost as i64 {
            return Err("资金不足".into());
        }
        let cost = self.deeds.build(&self.board, tile_id)?;
        self.change_cash(pid, -(cost as i64), events);
        let houses = self.deeds.houses(tile_id);
        events.push(Event::BuildHouse {
            player_id: pid,
            tile_id,
            houses,
        });
        Ok(())
    }

    pub(crate) fn do_sell_house(
        &mut self,
        pid: Uuid,
        tile_id: usize,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        self.require_player(pid)?;
        if self.deeds.owner(tile_id) != Some(pid) {
            return Err("只能出售自己地块上的房屋".into());
        }
        let value = self.deeds.sell_house(&self.board, tile_id)?;
        self.change_cash(pid, value as i64, events);
        let houses = self.deeds.houses(tile_id);
        events.push(Event::SoldHouse {
            player_id: pid,
            tile_id,
            houses,
        });
        Ok(())
    }

    pub(crate) fn do_mortgage(
        &mut self,
        pid: Uuid,
        tile_id: usize,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        self.require_player(pid)?;
        if self.deeds.owner(tile_id) != Some(pid) {
            return Err("只能抵押自己的地块".into());
        }
        let amount = self.deeds.mortgage(&self.board, tile_id)?;
        self.change_cash(pid, amount as i64, events);
        events.push(Event::Mortgaged {
            player_id: pid,
            tile_id,
            amount,
        });
        Ok(())
    }

    pub(crate) fn do_unmortgage(
        &mut self,
        pid: Uuid,
        tile_id: usize,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        self.require_player(pid)?;
        if self.deeds.owner(tile_id) != Some(pid) {
            return Err("只能赎回自己的地块".into());
        }
        let cost = self.deeds.unmortgage(&self.board, tile_id)?;
        if self.player_cash(pid) < cost as i64 {
            return Err("资金不足，无法赎回".into());
        }
        self.change_cash(pid, -(cost as i64), events);
        events.push(Event::Unmortgaged {
            player_id: pid,
            tile_id,
            cost,
        });
        Ok(())
    }

    // ---------- 交易 ----------

    pub(crate) fn do_propose_trade(
        &mut self,
        pid: Uuid,
        target: Uuid,
        offer: AssetList,
        demand: AssetList,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        self.require_player(pid)?;
        self.require_player(target)?;
        if target == pid {
            return Err("不能和自己交易".into());
        }
        if offer.cash < 0 || demand.cash < 0 {
            return Err("交易金额不能为负".into());
        }
        if self.player_cash(pid) < offer.cash {
            return Err("出价现金超出你的余额".into());
        }
        self.validate_assets(pid, &offer)?;
        self.validate_assets(target, &demand)?;
        let id = Uuid::new_v4();
        self.pending_trades.push(crate::trade::Trade {
            id,
            from: pid,
            to: target,
            offer: offer.clone(),
            demand: demand.clone(),
        });
        events.push(Event::TradeProposed {
            trade_id: id,
            from: pid,
            to: target,
            offer,
            demand,
        });
        Ok(())
    }

    fn validate_assets(&self, player: Uuid, assets: &AssetList) -> Result<(), String> {
        let max_cards = self
            .player(player)
            .map(|p| p.out_of_jail_cards)
            .unwrap_or(0);
        if assets.out_of_jail_cards > max_cards {
            return Err("出狱卡数量不足".into());
        }
        for &t in &assets.tiles {
            if self.deeds.owner(t) != Some(player) {
                return Err("筹码中的地块不属于该玩家".into());
            }
            if self.deeds.houses(t) > 0 || self.deeds.is_mortgaged(t) {
                return Err("含房屋或抵押的地块需先处理后再交易".into());
            }
        }
        Ok(())
    }

    pub(crate) fn do_accept_trade(
        &mut self,
        pid: Uuid,
        trade_id: Uuid,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        let idx = self
            .pending_trades
            .iter()
            .position(|t| t.id == trade_id && t.to == pid)
            .ok_or("交易不存在或不属于你")?;
        // 先完整校验再移除：接受失败时交易保持挂起，仍可拒绝。
        let trade = self.pending_trades[idx].clone();
        self.require_player(trade.from)?;
        self.require_player(trade.to)?;
        if self.player_cash(trade.to) < trade.demand.cash {
            return Err("你现金不足，无法接受该交易".into());
        }
        // 出价方现金在等待期间可能已被消耗，需在成交前复核。
        if self.player_cash(trade.from) < trade.offer.cash {
            return Err("对方现金不足以完成该交易".into());
        }
        self.validate_assets(trade.from, &trade.offer)?;
        self.validate_assets(trade.to, &trade.demand)?;
        self.pending_trades.remove(idx);

        // 执行 offer：from → to
        if trade.offer.cash > 0 {
            self.pay_to(trade.from, Some(trade.to), trade.offer.cash as u32, events);
        }
        for &t in &trade.offer.tiles {
            self.deeds.assign(t, trade.to);
        }
        if trade.offer.out_of_jail_cards > 0 {
            if let Some(p) = self.player_mut(trade.from) {
                p.out_of_jail_cards -= trade.offer.out_of_jail_cards;
            }
            if let Some(p) = self.player_mut(trade.to) {
                p.out_of_jail_cards += trade.offer.out_of_jail_cards;
            }
        }
        // 执行 demand：to → from
        if trade.demand.cash > 0 {
            self.pay_to(trade.to, Some(trade.from), trade.demand.cash as u32, events);
        }
        for &t in &trade.demand.tiles {
            self.deeds.assign(t, trade.from);
        }
        if trade.demand.out_of_jail_cards > 0 {
            if let Some(p) = self.player_mut(trade.to) {
                p.out_of_jail_cards -= trade.demand.out_of_jail_cards;
            }
            if let Some(p) = self.player_mut(trade.from) {
                p.out_of_jail_cards += trade.demand.out_of_jail_cards;
            }
        }
        events.push(Event::TradeAccepted { trade_id });
        Ok(())
    }

    pub(crate) fn do_decline_trade(
        &mut self,
        pid: Uuid,
        trade_id: Uuid,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        let idx = self
            .pending_trades
            .iter()
            .position(|t| t.id == trade_id && t.to == pid)
            .ok_or("交易不存在或不属于你")?;
        self.pending_trades.remove(idx);
        events.push(Event::TradeDeclined { trade_id });
        Ok(())
    }
}
