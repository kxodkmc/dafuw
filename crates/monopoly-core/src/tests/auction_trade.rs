//! 拍卖规则边界 与 交易全流程测试。

use crate::engine::{GameEngine, Phase};
use monopoly_common::command::{AssetList, Command};
use monopoly_common::event::Event;
use uuid::Uuid;

use super::support::{engine_with_players, force_current, start_rng, SeqRng};

/// 造出「A 掷到空地 3，停在 AwaitDecision」的局面。
fn land_on_unowned(a: Uuid, e: &mut GameEngine) {
    force_current(e, a);
    let evs = e
        .handle(a, Command::RollDice, &mut SeqRng::new(vec![0, 1]))
        .unwrap(); // (1,2)=3
    assert!(evs
        .iter()
        .any(|x| matches!(x, Event::LandedOn { tile_id: 3, .. })));
    assert_eq!(e.phase(), Phase::AwaitDecision);
}

#[test]
fn auction_requires_turn_order_and_increasing_bids() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    e.start(&mut start_rng()).unwrap();
    land_on_unowned(a, &mut e);

    // 放弃购买 → 进入拍卖。
    e.handle(a, Command::PassProperty, &mut SeqRng::new(vec![0]))
        .unwrap();
    assert_eq!(e.phase(), Phase::InAuction);

    // 非当前出价者出价 → 拒绝。
    assert!(e
        .handle(
            b,
            Command::AuctionBid { amount: 50 },
            &mut SeqRng::new(vec![0])
        )
        .is_err());
    // 未轮到的人无法弃权。
    assert!(e
        .handle(b, Command::AuctionPass, &mut SeqRng::new(vec![0]))
        .is_err());

    // 合法轮流出价。
    e.handle(
        a,
        Command::AuctionBid { amount: 1_000_000 },
        &mut SeqRng::new(vec![0]),
    )
    .unwrap();
    // 未轮到的 A 再出价 → 拒绝。
    assert!(e
        .handle(
            a,
            Command::AuctionBid { amount: 2_000_000 },
            &mut SeqRng::new(vec![0])
        )
        .is_err());

    e.handle(
        b,
        Command::AuctionBid { amount: 1_500_000 },
        &mut SeqRng::new(vec![0]),
    )
    .unwrap();
    e.handle(
        a,
        Command::AuctionBid { amount: 2_000_000 },
        &mut SeqRng::new(vec![0]),
    )
    .unwrap();
    // 出价必须高于当前最高价。
    assert!(e
        .handle(
            b,
            Command::AuctionBid { amount: 2_000_000 },
            &mut SeqRng::new(vec![0])
        )
        .is_err());

    e.handle(b, Command::AuctionPass, &mut SeqRng::new(vec![0]))
        .unwrap();
    // B 已弃权，再无其他人可出价 → 拍卖结束，A 获胜。
    assert_eq!(e.deeds.owner(3), Some(a));
    assert_eq!(e.player_cash(a), 15_000_000 - 2_000_000);
    assert_ne!(e.phase(), Phase::InAuction);
}

#[test]
fn auction_all_pass_leaves_tile_unowned() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    e.start(&mut start_rng()).unwrap();
    land_on_unowned(a, &mut e);

    e.handle(a, Command::PassProperty, &mut SeqRng::new(vec![0]))
        .unwrap();
    e.handle(a, Command::AuctionPass, &mut SeqRng::new(vec![0]))
        .unwrap();
    e.handle(b, Command::AuctionPass, &mut SeqRng::new(vec![0]))
        .unwrap();

    assert_eq!(e.deeds.owner(3), None);
    assert_ne!(e.phase(), Phase::InAuction);
    assert_eq!(e.player_cash(a), 15_000_000);
}

fn extract_trade_id(evs: &[Event]) -> Uuid {
    evs.iter()
        .find_map(|x| match x {
            Event::TradeProposed { trade_id, .. } => Some(*trade_id),
            _ => None,
        })
        .expect("应有 TradeProposed 事件")
}

#[test]
fn trade_cash_for_property_flow() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    e.deeds.assign(1, a);
    e.deeds.assign(3, b);
    let (a0, b0) = (e.player_cash(a), e.player_cash(b));

    // A 提议：现金 100 ↔ B 的 3 号地。
    let evs = e
        .handle(
            a,
            Command::ProposeTrade {
                target: b,
                offer: AssetList {
                    cash: 100,
                    tiles: vec![],
                    out_of_jail_cards: 0,
                },
                demand: AssetList {
                    cash: 0,
                    tiles: vec![3],
                    out_of_jail_cards: 0,
                },
            },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
    let trade_id = extract_trade_id(&evs);

    // 非接收方（A 自己）接受 → 拒绝；未提出的 id → 拒绝。
    assert!(e
        .handle(
            a,
            Command::AcceptTrade { trade_id },
            &mut SeqRng::new(vec![0])
        )
        .is_err());
    assert!(e
        .handle(
            b,
            Command::AcceptTrade {
                trade_id: Uuid::new_v4()
            },
            &mut SeqRng::new(vec![0])
        )
        .is_err());

    // B 接受 → 3 号地归 A，现金按筹码转移。
    let evs = e
        .handle(
            b,
            Command::AcceptTrade { trade_id },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::TradeAccepted { .. })));
    assert_eq!(e.deeds.owner(3), Some(a));
    assert_eq!(e.player_cash(a), a0 - 100);
    assert_eq!(e.player_cash(b), b0 + 100);
}

#[test]
fn trade_rejected_when_cash_insufficient() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    // B 现金只有 15M，要求 20M 现金 → 接受时拒绝。
    let evs = e
        .handle(
            a,
            Command::ProposeTrade {
                target: b,
                offer: AssetList::default(),
                demand: AssetList {
                    cash: 20_000_000,
                    tiles: vec![],
                    out_of_jail_cards: 0,
                },
            },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
    let trade_id = extract_trade_id(&evs);
    assert!(e
        .handle(
            b,
            Command::AcceptTrade { trade_id },
            &mut SeqRng::new(vec![0])
        )
        .is_err());
    // 交易仍挂起（未移除），可正常拒绝。
    assert!(e
        .handle(
            b,
            Command::DeclineTrade { trade_id },
            &mut SeqRng::new(vec![0])
        )
        .is_ok());
}

#[test]
fn trade_with_house_rejected_before_execution() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    // B 集齐棕组并建 1 栋房 → 含房地块不可交易。
    e.deeds.assign(1, b);
    e.deeds.assign(3, b);
    e.deeds.build(&e.board, 1).unwrap();
    assert_eq!(e.deeds.houses(1), 1);

    let res = e.handle(
        a,
        Command::ProposeTrade {
            target: b,
            offer: AssetList::default(),
            demand: AssetList {
                cash: 0,
                tiles: vec![1],
                out_of_jail_cards: 0,
            },
        },
        &mut SeqRng::new(vec![0]),
    );
    assert!(res.is_err());
}

#[test]
fn trade_out_of_jail_card_moves() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    e.player_mut(a).unwrap().out_of_jail_cards = 2;

    let evs = e
        .handle(
            a,
            Command::ProposeTrade {
                target: b,
                offer: AssetList {
                    cash: 0,
                    tiles: vec![],
                    out_of_jail_cards: 1,
                },
                demand: AssetList {
                    cash: 500_000,
                    tiles: vec![],
                    out_of_jail_cards: 0,
                },
            },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
    let trade_id = extract_trade_id(&evs);
    e.handle(
        b,
        Command::AcceptTrade { trade_id },
        &mut SeqRng::new(vec![0]),
    )
    .unwrap();

    assert_eq!(e.player(a).unwrap().out_of_jail_cards, 1);
    assert_eq!(e.player(b).unwrap().out_of_jail_cards, 1);
    assert_eq!(e.player_cash(a), 15_000_000 + 500_000);
    assert_eq!(e.player_cash(b), 15_000_000 - 500_000);
}

#[test]
fn trade_can_be_declined_and_removed() {
    let (mut e, ids) = engine_with_players(2, None);
    let a = ids[0];
    let b = ids[1];
    let evs = e
        .handle(
            a,
            Command::ProposeTrade {
                target: b,
                offer: AssetList::default(),
                demand: AssetList::default(),
            },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
    let trade_id = extract_trade_id(&evs);

    let evs = e
        .handle(
            b,
            Command::DeclineTrade { trade_id },
            &mut SeqRng::new(vec![0]),
        )
        .unwrap();
    assert!(evs.iter().any(|x| matches!(x, Event::TradeDeclined { .. })));
    // 已移除 → 再接受失败。
    assert!(e
        .handle(
            b,
            Command::AcceptTrade { trade_id },
            &mut SeqRng::new(vec![0])
        )
        .is_err());
}
