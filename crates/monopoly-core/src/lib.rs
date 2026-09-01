//! 纯规则引擎：实现大富翁核心规则。
//!
//! 本 crate **不依赖任何 IO / 网络 / 异步**，只负责状态转移：
//! `handle(player, command, rng) -> Vec<Event>`，配合 `snapshot()` 即可被任意上层复用
//! （服务器 / 测试 / 未来的本地对战）。随机性通过 [`RngSource`] 注入，可复现可测试。

mod archive;
pub mod auction;
pub mod board;
pub mod cards;
pub mod deeds;
pub mod engine;
pub mod engine_actions;
pub mod engine_resolve;
pub mod player;
pub mod rng;
pub mod trade;

pub use auction::Auction;
pub use board::{Board, Group, Tile, TileKind};
pub use cards::{Card, CardEffect, Deck};
pub use deeds::Deeds;
pub use engine::{GameEngine, Phase};
pub use player::Player;
pub use rng::{RngSource, StdRngSource};
pub use trade::{Trade, TradeAssets};

#[cfg(test)]
pub mod tests;
