//! 稳定性测试：随机长对局不 panic、必然终局、资金/破产不变量成立。
//! 以及卡组的安全边界。

use crate::cards::Deck;
use crate::engine::{GameEngine, Phase};
use crate::rng::{RngSource, StdRngSource};
use crate::tests::support::combo;
use monopoly_common::command::Command;
use monopoly_common::room::RoomSettings;
use rand::rngs::StdRng;
use rand::SeedableRng;
use uuid::Uuid;

fn run_random_game(seed: u64, players: usize, max_rounds: u32) {
    let settings = RoomSettings {
        player_count_max: players as u8,
        max_rounds: Some(max_rounds),
        ..Default::default()
    };
    let mut e = GameEngine::new(settings).expect("配置必须合法");
    for i in 0..players {
        let (avatar, color) = combo(i);
        e.add_player(
            Uuid::from_u128(100 + i as u128 + seed as u128 * 32),
            format!("P{i}"),
            avatar,
            color,
        )
        .unwrap();
    }
    let mut rng = StdRngSource(StdRng::seed_from_u64(seed));
    e.start(&mut rng).unwrap();

    let mut steps = 0usize;
    let mut last_advance_round = 0u32;
    while !e.is_over() && steps < 8000 {
        steps += 1;
        let pid = if e.phase() == Phase::InAuction {
            e.auction_current_bidder()
        } else {
            e.current_player_id()
        };
        let Some(pid) = pid else {
            e.advance_turn(&mut Vec::new());
            continue;
        };

        let cmd = match e.phase() {
            Phase::AwaitRoll => Command::RollDice,
            Phase::AwaitDecision => {
                if rng.next_below(100) < 50 {
                    Command::BuyProperty
                } else {
                    Command::PassProperty
                }
            }
            Phase::InAuction => {
                if rng.next_below(100) < 40 {
                    Command::AuctionBid {
                        amount: rng.next_below(500) + 1,
                    }
                } else {
                    Command::AuctionPass
                }
            }
            _ => break,
        };
        let _ = e.handle(pid, cmd, &mut rng);

        // 不变量：破产玩家现金 0；其余玩家现金不为负。
        for p in e.players.iter() {
            if p.bankrupt {
                assert_eq!(p.cash, 0, "seed {seed}: 破产玩家现金应为 0");
            } else {
                assert!(p.cash >= 0, "seed {seed}: 未破产玩家现金不能为负");
            }
        }
        // 防停滞：回合号必须前进。
        if e.round > last_advance_round {
            last_advance_round = e.round;
        }
    }
    assert!(
        e.is_over(),
        "seed {seed}: 对局应在限时内结束 (steps={steps})"
    );
    assert!(e.winner().is_some(), "seed {seed}: 应有胜者");
}

#[test]
fn random_long_games_terminate_with_invariants() {
    for seed in 0..15 {
        run_random_game(seed, 4, 30);
    }
}

#[test]
fn two_player_random_games_also_terminate() {
    for seed in 0..8 {
        run_random_game(100 + seed, 2, 20);
    }
}

#[test]
fn deck_draw_cycles_and_empty_is_safe() {
    let mut d = Deck::chance();
    assert_eq!(d.len(), 10);
    for _ in 0..100 {
        assert!(d.draw().is_some());
    }

    // 空卡组：draw 返回 None，remaining 为 0，不 panic。
    let mut empty = Deck::new(vec![]);
    assert!(empty.draw().is_none());
    assert_eq!(empty.remaining(), 0);
    assert!(empty.is_empty());
}

#[test]
fn deck_covers_all_cards_with_text() {
    let mut d = Deck::chance();
    let mut ids = std::collections::HashSet::new();
    for _ in 0..d.len() {
        let c = d.draw().unwrap();
        ids.insert(c.id);
        assert!(!c.text.is_empty());
    }
    assert_eq!(ids.len(), 10, "抽一轮应覆盖全部 10 张卡");

    let mut f = Deck::fate();
    let mut ids = std::collections::HashSet::new();
    for _ in 0..f.len() {
        ids.insert(f.draw().unwrap().id);
    }
    assert_eq!(ids.len(), 10);
}
