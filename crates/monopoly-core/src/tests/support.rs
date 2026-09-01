//! 测试公共辅助：可复现随机源 与 可控引擎构造。

use crate::engine::GameEngine;
use crate::rng::RngSource;
use monopoly_common::avatar::{Avatar, Color};
use monopoly_common::room::RoomSettings;
use uuid::Uuid;

/// 按序号取互不重复的「形象 + 颜色」组合（5×5=25 种，覆盖房间人数上限 8）。
pub fn combo(i: usize) -> (Avatar, Color) {
    (Avatar::ALL[i % 5], Color::ALL[(i / 5) % 5])
}

/// 把所有玩家标记为准备就绪（开局前置条件）。
pub fn mark_all_ready(e: &mut GameEngine) {
    let ids: Vec<Uuid> = e.players.iter().map(|p| p.id).collect();
    for id in ids {
        e.set_ready(id, true).expect("大厅中应可设置准备状态");
    }
}

/// 固定序列随机源：按序返回 `val % bound`，越界循环复用。
/// 注意：需保证 `vals` 非空。
pub struct SeqRng {
    pub vals: Vec<u32>,
    pub i: usize,
}

impl SeqRng {
    pub fn new(vals: Vec<u32>) -> Self {
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

/// 充足的全 0 随机序列（开局洗牌等仅需消耗，值固定 0）。
pub fn start_rng() -> SeqRng {
    SeqRng::new(vec![0; 64])
}

/// 指定玩家数的引擎（玩家 ID 随机），config.max_rounds 透传。
pub fn engine_with_players(n: usize, max_rounds: Option<u32>) -> (GameEngine, Vec<Uuid>) {
    let settings = RoomSettings {
        player_count_max: n as u8,
        max_rounds,
        ..Default::default()
    };
    let mut e = GameEngine::new(settings).expect("默认配置必须合法");
    let mut ids = Vec::new();
    for i in 0..n {
        let id = Uuid::new_v4();
        let (avatar, color) = combo(i);
        e.add_player(id, format!("P{i}"), avatar, color).unwrap();
        ids.push(id);
    }
    mark_all_ready(&mut e);
    (e, ids)
}

/// 固定 ID 版本的引擎（用于确定性测试：事件序列可比较）。
pub fn engine_with_fixed_ids(n: usize) -> (GameEngine, Vec<Uuid>) {
    let settings = RoomSettings {
        player_count_max: n as u8,
        ..Default::default()
    };
    let mut e = GameEngine::new(settings).expect("默认配置必须合法");
    let ids: Vec<Uuid> = (1..=n).map(|i| Uuid::from_u128(i as u128)).collect();
    for (i, id) in ids.iter().enumerate() {
        let (avatar, color) = combo(i);
        e.add_player(*id, format!("P{i}"), avatar, color).unwrap();
    }
    mark_all_ready(&mut e);
    (e, ids)
}

/// 把某位玩家设为当前回合玩家（开局洗牌后座位顺序不定，需按 ID 定位）。
pub fn force_current(e: &mut GameEngine, id: Uuid) {
    let idx = e
        .players
        .iter()
        .position(|p| p.id == id)
        .expect("玩家必须在场");
    e.current = idx;
}
