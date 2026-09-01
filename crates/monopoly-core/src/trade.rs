use monopoly_common::archive::TradeDto;
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

impl Trade {
    /// 导出交易中间状态（对局存档用）。
    pub(crate) fn to_dto(&self) -> TradeDto {
        TradeDto {
            id: self.id,
            from: self.from,
            to: self.to,
            offer: self.offer.clone(),
            demand: self.demand.clone(),
        }
    }

    /// 从 DTO 恢复交易中间状态（对局恢复用）。
    pub(crate) fn restore(dto: &TradeDto) -> Self {
        Self {
            id: dto.id,
            from: dto.from,
            to: dto.to,
            offer: dto.offer.clone(),
            demand: dto.demand.clone(),
        }
    }
}

/// 兼容别名（供外部引用交易结构时使用）。
pub type TradeAssets = AssetList;
