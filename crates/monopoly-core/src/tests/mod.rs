//! 规则引擎综合测试（crate 内测试模块）。
//!
//! 说明：为了可控地驱动引擎（如指定当前玩家、现金、地产、入狱），
//! 本模块直接访问 `pub(crate)` 内部状态，因此放在 crate 内部而非集成测试。

mod auction_trade;
mod rules;
mod stability;
pub mod support;
