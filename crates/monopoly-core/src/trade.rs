use monopoly_common::command::AssetList;
use uuid::Uuid;

/// 待处理交易。`offer` 为 `from` 支付给 `to` 的筹码，`demand` 为 `to` 支付给 `from` 的筹码。
#[derive(Debug, Clone)]
pub struct Trade {
    pub id: Uuid,
    pub from: Uuid,
    pub to: Uuid,
    pub offer: AssetList,
    pub demand: AssetList,
}

/// 兼容别名（供外部引用交易结构时使用）。
pub type TradeAssets = AssetList;
