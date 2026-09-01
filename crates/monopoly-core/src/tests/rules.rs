//! 核心规则测试：骰子 / 监狱 / 税务 / GO / 限时 / 确定性 / 快照。

use crate::engine::{self, GameEngine, Phase};
use monopoly_common::command::Command;
use monopoly_common::event::Event;

use super::support::{
    engine_with_fixed_ids, engine_with_players, force_current, start_rng, SeqRng,
};

#[test]
fn roll_dice_bounds_count_and_double_detection() {
    let mut rng = SeqRng::new(vec![0, 5, 2]);
    let dice = engine::roll_dice(2, &mut rng);
    assert_eq!(dice.len(), 2);
    assert!(dice.iter().all(|&d| (1..=6).contains(&d)));

    let dice3 = engine::roll_dice(3, &mut rng);
    assert_eq!(dice3.len(), 3);

    assert!(engine::is_double(&[3, 3]));
    assert!(!engine::is_double(&[3, 4]));
    assert!(!engine::is_double(&[3]));
}

#[test]
fn doubles_allow_extra_roll_without_turn_change() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 8; // 8 号为自己地块
    e.deeds.assign(8, a);

    // 第一次 (1,1)：8 → 10 监狱【探访】，无效果，对子再掷。
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 0]))
        .unwrap();
    let doubles = evs.iter().find_map(|x| match x {
        Event::DiceRolled { dice, .. } => Some(dice == &vec![1, 1]),
        _ => None,
    });
    assert_eq!(doubles, Some(true));
    assert_eq!(e.current_player_id(), Some(a)); // 仍 A

    // 第二次 (1,1)：10 → 12 电力（空地）→ AwaitDecision。
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 0]))
        .unwrap();
    assert!(evs.iter().any(|x| matches!(
        x,
        Event::DiceRolled {
            new_position: 12,
            ..
        }
    )));
    assert_eq!(e.phase(), Phase::AwaitDecision);
    assert_eq!(e.current_player_id(), Some(a));
}

#[test]
fn landing_on_go_to_jail_sends_player_to_jail() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 27;
    // (1,2)=3 → 30 GoToJail → 入狱。
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 1]))
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::WentToJail { .. })));
    assert_eq!(e.player_position(a), 10);
    assert!(e.player(a).unwrap().in_jail);
}

#[test]
fn pay_bail_releases_from_jail_with_50() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    e.send_to_jail(a, &mut Vec::new());
    let evs = e
        .handle(a, Command::PayBail, &mut SeqRng::new(vec![0]))
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::JailReleased { .. })));
    assert!(!e.player(a).unwrap().in_jail);
    assert_eq!(e.player_cash(a), 1450);
}

#[test]
fn use_out_of_jail_card_consumes_one_card() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    e.player_mut(a).unwrap().out_of_jail_cards = 1;
    e.send_to_jail(a, &mut Vec::new());
    let evs = e
        .handle(a, Command::UseOutOfJailCard, &mut SeqRng::new(vec![0]))
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::JailReleased { .. })));
    assert!(!e.player(a).unwrap().in_jail);
    assert_eq!(e.player(a).unwrap().out_of_jail_cards, 0);
}

#[test]
fn forced_bail_after_three_jail_turns() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.send_to_jail(a, &mut Vec::new());
    e.send_to_jail(b, &mut Vec::new());

    // A 与 B 轮流在狱中掷非对子；第 3 次轮到 A 时强制出狱。
    for i in 0..5 {
        let pid = if i % 2 == 0 { a } else { b };
        let evs = e
            .handle(pid, Command::RollDice, &mut SeqRng::new(vec![0, 1]))
            .unwrap();
        if i < 4 {
            assert!(
                evs.iter()
                    .any(|x| matches!(x, Event::JailTurnSkipped { .. })),
                "第 {i} 次应跳过"
            );
        } else {
            assert!(
                evs.iter().any(|x| matches!(x, Event::JailReleased { .. })),
                "第 3 次应强制出狱"
            );
        }
    }
    assert!(!e.player(a).unwrap().in_jail);
    assert_eq!(e.player_cash(a), 1450);
    // 200 < salary 不影响：保释扣 50。
}

#[test]
fn jail_double_releases_and_moves() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.send_to_jail(a, &mut Vec::new());
    // 狱中掷 (1,1) 对子 → 出狱并移动 10 → 12。
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 0]))
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::JailReleased { .. })));
    assert!(!e.player(a).unwrap().in_jail);
    assert_eq!(e.player_position(a), 12);
    assert_eq!(e.phase(), Phase::AwaitDecision); // 电力公司空地可购买
}

#[test]
fn income_tax_pays_higher_of_200_or_10pct() {
    // 200 分支（资产 1500，10% 仅 150）。
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 2;
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 0]))
        .unwrap(); // (1,1) → 4 税
    let tax = evs.iter().find_map(|x| match x {
        Event::TaxPaid { amount, .. } => Some(*amount),
        _ => None,
    });
    assert_eq!(tax, Some(200));

    // 10% 分支：现金 2200 + 五月花 400 = 2600 → 260 > 200。
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 2;
    e.change_cash(a, 700, &mut Vec::new());
    e.deeds.assign(39, a);
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 0]))
        .unwrap();
    let tax = evs.iter().find_map(|x| match x {
        Event::TaxPaid { amount, .. } => Some(*amount),
        _ => None,
    });
    assert_eq!(tax, Some(260));
}

#[test]
fn luxury_tax_is_flat_100() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 36;
    // (1,1) → 38 奢侈品税。
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 0]))
        .unwrap();
    let tax = evs.iter().find_map(|x| match x {
        Event::TaxPaid { amount, .. } => Some(*amount),
        _ => None,
    });
    assert_eq!(tax, Some(100));
    assert_eq!(e.player_cash(a), 1400);
}

#[test]
fn passing_go_awards_salary() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 38;
    // (1,2) → 41 % 40 = 1，跨过 GO。
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 1]))
        .unwrap();
    assert!(evs
        .iter()
        .any(|x| matches!(x, Event::PassedGo { salary: 200, .. })));
    assert_eq!(e.player_position(a), 1);
    assert_eq!(e.player_cash(a), 1700);
}

#[test]
fn max_rounds_ends_game_by_net_worth() {
    let (mut e, ids) = engine_with_players(2, Some(1));
    e.start(&mut start_rng()).unwrap();
    // 第一圈：A、B 各掷一次非对子；绕回 A（round=1 ≥ max=1）时触发限时终局。
    let a = e.current_player_id().unwrap();
    // (4,6)=10 → 落在 10【监狱】探访，无副作用。
    e.handle(a, Command::RollDice, &mut SeqRng::new(vec![3, 5]))
        .unwrap();
    let b = e.current_player_id().unwrap();
    let evs = e
        .handle(b, Command::RollDice, &mut SeqRng::new(vec![3, 5]))
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::GameEnded { .. })));
    assert_eq!(e.winner().unwrap(), a, "两人净资产相同且 A 先手，应判 A 胜");
    assert!(e.is_over());
    let _ = ids;
}

#[test]
fn deterministic_seed_produces_identical_events() {
    let run = || {
        let (mut e, ids) = engine_with_fixed_ids(2);
        e.start(&mut start_rng()).unwrap();
        let a = ids[0];
        force_current(&mut e, a);
        let evs = e
            .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 1]))
            .unwrap();
        evs.into_iter()
            .map(|x| serde_json::to_value(&x).unwrap())
            .collect::<Vec<_>>()
    };
    let v1 = run();
    let v2 = run();
    assert_eq!(v1, v2);
    assert!(!v1.is_empty());
}

#[test]
fn snapshot_matches_engine_state() {
    let (mut e, ids) = engine_with_players(2, None);
    assert_eq!(e.snapshot().status, "lobby");
    assert_eq!(e.snapshot().tiles.len(), 40);

    e.start(&mut start_rng()).unwrap();
    let a = ids[0];
    force_current(&mut e, a);
    e.player_mut(a).unwrap().position = 3;
    e.handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 1]))
        .unwrap(); // (1,2)=3 → 6 东方大道空地
    e.handle(a, Command::BuyProperty, &mut SeqRng::new(vec![0]))
        .unwrap();

    let s = e.snapshot();
    assert_eq!(s.status, "playing");
    assert_eq!(s.tiles[6].owner, Some(a));
    assert_eq!(s.players.iter().find(|p| p.id == a).unwrap().cash, 1400);
    assert_eq!(s.players.iter().find(|p| p.id == a).unwrap().position, 6);
    assert_eq!(s.houses_available, 32);
    assert_eq!(s.hotels_available, 12);
}

#[test]
fn owning_no_land_and_no_cash_is_allowed_state() {
    // 空状态不 panic 的冒烟：未开局直接查快照 / 净值。
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    e.start(&mut start_rng()).unwrap();
    assert_eq!(e.net_worth(a), 1500);
    let _ = GameEngine::new(Default::default()).unwrap();
}
