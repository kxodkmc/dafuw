//! 引擎的掷骰与落地结算：移动、落地触发、卡牌效果。
//!
//! 说明（简化规则）：经卡牌强制移动落地的地块**不触发购买/付租**，只落位。

use monopoly_common::event::Event;
use uuid::Uuid;

use crate::board::TileKind;
use crate::cards::CardEffect;
use crate::engine::{dice_sum, is_double, roll_dice, GameEngine, Phase};
use crate::rng::RngSource;

impl GameEngine {
    /// 掷骰并移动当前玩家（含入狱分支与三次对子入狱）。
    pub(crate) fn do_roll(
        &mut self,
        rng: &mut dyn RngSource,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        let pid = self.current_player_id().ok_or("尚未开局")?;
        let in_jail = self.player(pid).map(|p| p.in_jail).unwrap_or(false);
        if in_jail {
            return self.roll_in_jail(pid, rng, events);
        }

        let dice = roll_dice(self.settings.dice_count, rng);
        let sum = dice_sum(&dice);
        let double = is_double(&dice);
        let from = self.player_position(pid);

        self.dice = dice.clone();
        if double {
            self.doubles_count = self.doubles_count.saturating_add(1);
        } else {
            self.doubles_count = 0;
        }

        // 连续三次对子 → 直接入狱。
        if double && self.doubles_count >= 3 {
            events.push(Event::DiceRolled {
                player_id: pid,
                dice: dice.clone(),
                doubles: true,
                doubles_count: self.doubles_count,
                new_position: from,
            });
            events.push(Event::Message {
                text: "连续三次对子，直接入狱".into(),
            });
            self.send_to_jail(pid, events);
            self.advance_turn(events);
            return Ok(());
        }

        let to = (from + sum as usize) % self.board.len();
        if let Some(p) = self.player_mut(pid) {
            p.position = to;
        }
        self.pending_doubles = double;
        events.push(Event::DiceRolled {
            player_id: pid,
            dice: dice.clone(),
            doubles: double,
            doubles_count: self.doubles_count,
            new_position: to,
        });
        events.push(Event::PlayerMoved {
            player_id: pid,
            from,
            to,
        });
        if to < from {
            let salary = self.settings.go_salary;
            self.change_cash(pid, salary as i64, events);
            events.push(Event::PassedGo {
                player_id: pid,
                salary,
            });
        }
        events.push(Event::LandedOn {
            player_id: pid,
            tile_id: to,
        });
        self.resolve_landing(pid, sum, events)?;
        self.finish_turn_if_needed(events);
        Ok(())
    }

    /// 狱中掷骰：掷对子出狱并移动；否则累计，3 回合后强制保释。
    fn roll_in_jail(
        &mut self,
        pid: Uuid,
        rng: &mut dyn RngSource,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        let dice = roll_dice(self.settings.dice_count, rng);
        let double = is_double(&dice);
        let from = self.player_position(pid);
        self.dice = dice.clone();
        events.push(Event::DiceRolled {
            player_id: pid,
            dice: dice.clone(),
            doubles: double,
            doubles_count: 0,
            new_position: from,
        });

        if double {
            if let Some(p) = self.player_mut(pid) {
                p.in_jail = false;
                p.jail_turns = 0;
            }
            events.push(Event::JailReleased { player_id: pid });
            let sum = dice_sum(&dice);
            let to = (from + sum as usize) % self.board.len();
            if let Some(p) = self.player_mut(pid) {
                p.position = to;
            }
            events.push(Event::PlayerMoved {
                player_id: pid,
                from,
                to,
            });
            if to < from {
                let salary = self.settings.go_salary;
                self.change_cash(pid, salary as i64, events);
                events.push(Event::PassedGo {
                    player_id: pid,
                    salary,
                });
            }
            events.push(Event::LandedOn {
                player_id: pid,
                tile_id: to,
            });
            self.resolve_landing(pid, sum, events)?;
            self.finish_turn_if_needed(events);
        } else {
            let turns = self
                .player(pid)
                .map(|p| p.jail_turns)
                .unwrap_or(0)
                .saturating_add(1);
            if let Some(p) = self.player_mut(pid) {
                p.jail_turns = turns;
            }
            if turns >= 3 {
                events.push(Event::Message {
                    text: "已在狱中 3 回合，强制支付 $50 出狱".into(),
                });
                self.pay_to(pid, None, 50, events);
                if !self.player(pid).map(|p| p.bankrupt).unwrap_or(true) {
                    if let Some(p) = self.player_mut(pid) {
                        p.in_jail = false;
                        p.jail_turns = 0;
                    }
                    events.push(Event::JailReleased { player_id: pid });
                }
            } else {
                events.push(Event::JailTurnSkipped { player_id: pid });
            }
            self.finish_turn_if_needed(events);
        }
        Ok(())
    }

    /// 按落地格类型结算：购地决策 / 付租 / 抽卡 / 缴税 / 入狱等。
    fn resolve_landing(
        &mut self,
        pid: Uuid,
        dice_sum: u32,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        let pos = self.player_position(pid);
        match self.board.tile(pos).kind {
            TileKind::Go | TileKind::Jail | TileKind::FreeParking => Ok(()),
            TileKind::GoToJail => {
                self.send_to_jail(pid, events);
                Ok(())
            }
            TileKind::Tax => {
                let tile = self.board.tile(pos);
                let amount = if tile.income_tax {
                    // 所得税：$200 或总资产 10%，取较高者。
                    let assets = self.net_worth(pid).max(0) as u64;
                    let ten_pct = (assets / 10) as u32;
                    tile.tax.max(ten_pct)
                } else {
                    tile.tax
                };
                self.pay_to(pid, None, amount, events);
                events.push(Event::TaxPaid {
                    player_id: pid,
                    amount,
                    tile_id: pos,
                });
                Ok(())
            }
            TileKind::Chance => {
                let Some(card) = self.chance.draw() else {
                    return Ok(());
                };
                events.push(Event::CardDrawn {
                    player_id: pid,
                    card_text: card.text.clone(),
                });
                self.apply_card(pid, card, events)
            }
            TileKind::Fate => {
                let Some(card) = self.fate.draw() else {
                    return Ok(());
                };
                events.push(Event::CardDrawn {
                    player_id: pid,
                    card_text: card.text.clone(),
                });
                self.apply_card(pid, card, events)
            }
            TileKind::Property | TileKind::Railroad | TileKind::Utility => {
                if self.board.tile(pos).price == 0 {
                    return Ok(());
                }
                match self.deeds.owner(pos) {
                    None => {
                        self.phase = Phase::AwaitDecision;
                        Ok(())
                    }
                    Some(o) if o == pid => Ok(()),
                    Some(o) => {
                        if self.deeds.is_mortgaged(pos) {
                            return Ok(()); // 抵押期间不收租
                        }
                        let rent = self.deeds.rent(&self.board, pos, dice_sum);
                        if rent > 0 {
                            self.pay_to(pid, Some(o), rent, events);
                            events.push(Event::RentPaid {
                                from: pid,
                                to: o,
                                amount: rent,
                                tile_id: pos,
                            });
                        }
                        Ok(())
                    }
                }
            }
        }
    }

    /// 若地块被他人拥有且未抵押，支付租金。
    fn maybe_pay_rent(&mut self, pid: Uuid, tile: usize, dice_sum: u32, events: &mut Vec<Event>) {
        if let Some(o) = self.deeds.owner(tile) {
            if o != pid && !self.deeds.is_mortgaged(tile) {
                let rent = self.deeds.rent(&self.board, tile, dice_sum);
                if rent > 0 {
                    self.pay_to(pid, Some(o), rent, events);
                    events.push(Event::RentPaid {
                        from: pid,
                        to: o,
                        amount: rent,
                        tile_id: tile,
                    });
                }
            }
        }
    }

    /// 执行卡牌效果。
    fn apply_card(
        &mut self,
        pid: Uuid,
        card: crate::cards::Card,
        events: &mut Vec<Event>,
    ) -> Result<(), String> {
        match card.effect {
            CardEffect::Collect(amt) => {
                self.change_cash(pid, amt as i64, events);
            }
            CardEffect::Pay(amt) => {
                self.pay_to(pid, None, amt, events);
            }
            CardEffect::CollectFromEach(amt) => {
                let others: Vec<Uuid> = self
                    .alive_players()
                    .into_iter()
                    .filter(|&o| o != pid)
                    .collect();
                for o in others {
                    self.pay_to(o, Some(pid), amt, events);
                }
            }
            CardEffect::PayEach(amt) => {
                let others: Vec<Uuid> = self
                    .alive_players()
                    .into_iter()
                    .filter(|&o| o != pid)
                    .collect();
                for o in others {
                    self.pay_to(pid, Some(o), amt, events);
                }
            }
            CardEffect::MoveTo(pos) => {
                self.move_player(pid, pos, events);
            }
            CardEffect::MoveBy(delta) => {
                let from = self.player_position(pid);
                let n = self.board.len() as i32;
                let to = (((from as i32 + delta) % n) + n) % n;
                self.move_player(pid, to as usize, events);
            }
            CardEffect::GoToJail => {
                self.send_to_jail(pid, events);
            }
            CardEffect::GetOutOfJail => {
                if let Some(p) = self.player_mut(pid) {
                    p.out_of_jail_cards = p.out_of_jail_cards.saturating_add(1);
                }
            }
            CardEffect::HouseRepair {
                per_house,
                per_hotel,
            } => {
                let houses = self.deeds.total_houses(pid);
                let hotels = self.deeds.total_hotels(pid);
                let amt = houses * per_house + hotels * per_hotel;
                self.pay_to(pid, None, amt, events);
            }
            CardEffect::NearestRailroad => {
                let pos = self.player_position(pid);
                let target = self.board.nearest_railroad(pos);
                self.move_player(pid, target, events);
                self.maybe_pay_rent(pid, target, 0, events);
            }
            CardEffect::NearestUtility => {
                let pos = self.player_position(pid);
                let target = self.board.nearest_utility(pos);
                self.move_player(pid, target, events);
            }
        }
        events.push(Event::CardApplied {
            player_id: pid,
            text: card.text,
        });
        Ok(())
    }

    /// 强制移动（卡牌），经过/停留 GO 领取薪水。
    fn move_player(&mut self, pid: Uuid, to: usize, events: &mut Vec<Event>) {
        let from = self.player_position(pid);
        if let Some(p) = self.player_mut(pid) {
            p.position = to;
        }
        events.push(Event::PlayerMoved {
            player_id: pid,
            from,
            to,
        });
        if to < from {
            let salary = self.settings.go_salary;
            self.change_cash(pid, salary as i64, events);
            events.push(Event::PassedGo {
                player_id: pid,
                salary,
            });
        }
    }
}
