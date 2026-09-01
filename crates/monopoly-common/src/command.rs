use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::avatar::{Avatar, Color};

/// 交易筹码：现金 + 地块 + 出狱卡。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AssetList {
    pub cash: i64,
    pub tiles: Vec<usize>,
    pub out_of_jail_cards: u8,
}

/// 客户端 → 服务器的指令。
///
/// 指令本身不携带玩家身份；身份由服务器根据会话令牌解析后注入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    /// 房主开局（由服务器在房间层拦截处理）。
    StartGame,
    /// 掷骰移动。
    RollDice,
    /// 支付 $50 保释出狱。
    PayBail,
    /// 使用出狱卡。
    UseOutOfJailCard,
    /// 购买停留的空地。
    BuyProperty,
    /// 放弃购买，触发银行拍卖。
    PassProperty,
    /// 拍卖出价。
    AuctionBid { amount: u32 },
    /// 拍卖弃权。
    AuctionPass,
    /// 建房。
    BuildHouse { tile_id: usize },
    /// 卖房。
    SellHouse { tile_id: usize },
    /// 抵押地产。
    Mortgage { tile_id: usize },
    /// 赎回抵押（抵押价 + 10% 利息）。
    Unmortgage { tile_id: usize },
    /// 提出交易（双方筹码可含现金/地块/出狱卡）。
    ProposeTrade {
        target: Uuid,
        offer: AssetList,
        demand: AssetList,
    },
    /// 接受交易。
    AcceptTrade { trade_id: Uuid },
    /// 拒绝交易。
    DeclineTrade { trade_id: Uuid },
    /// 大厅中更新玩家资料（昵称/形象/颜色，未提供的字段保持不变）。
    UpdateProfile {
        name: Option<String>,
        avatar: Option<Avatar>,
        color: Option<Color>,
    },
    /// 大厅中标记/取消准备就绪。
    SetReady { ready: bool },
    /// 房间内发送聊天消息（房间层处理，不进引擎）。
    Chat { text: String },
}
