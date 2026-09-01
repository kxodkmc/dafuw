use crate::rng::RngSource;

/// 卡牌效果原语。所有效果都是数据驱动的，加卡不需要改引擎逻辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardEffect {
    /// 从银行获得现金。
    Collect(u32),
    /// 向银行支付现金。
    Pay(u32),
    /// 强制移动到指定格（绝对位置）。
    MoveTo(usize),
    /// 相对位移（可负）。
    MoveBy(i32),
    /// 立即入狱。
    GoToJail,
    /// 获得出狱卡。
    GetOutOfJail,
    /// 房屋维修：每栋房 / 每间旅馆。
    HouseRepair { per_house: u32, per_hotel: u32 },
    /// 前往最近的铁路。
    NearestRailroad,
    /// 前往最近的公用事业。
    NearestUtility,
    /// 每位其他玩家向你支付。
    CollectFromEach(u32),
    /// 你向每位其他玩家支付。
    PayEach(u32),
}

/// 单张卡牌。
#[derive(Debug, Clone)]
pub struct Card {
    pub id: u32,
    pub text: String,
    pub effect: CardEffect,
}

/// 卡组：抽卡 + 洗牌，抽空后重新洗牌。
#[derive(Debug, Clone)]
pub struct Deck {
    cards: Vec<Card>,
    order: Vec<usize>,
    draw_index: usize,
}

impl Deck {
    pub fn new(cards: Vec<Card>) -> Self {
        let order: Vec<usize> = (0..cards.len()).collect();
        Self {
            cards,
            order,
            draw_index: 0,
        }
    }

    pub fn shuffle(&mut self, rng: &mut dyn RngSource) {
        let n = self.order.len();
        for i in (1..n).rev() {
            let j = rng.next_below((i + 1) as u32) as usize;
            self.order.swap(i, j);
        }
        self.draw_index = 0;
    }

    /// 抽一张卡；卡组为空返回 `None`（正常对局卡组恒非空）。
    pub fn draw(&mut self) -> Option<Card> {
        if self.order.is_empty() {
            return None;
        }
        let idx = self.order[self.draw_index % self.order.len()];
        // 取模自绕，避免极端长对局中 usize 溢出。
        self.draw_index = (self.draw_index + 1) % self.order.len();
        Some(self.cards[idx].clone())
    }

    pub fn remaining(&self) -> usize {
        let len = self.order.len();
        if len == 0 {
            0
        } else {
            len - (self.draw_index % len)
        }
    }

    pub fn len(&self) -> usize {
        self.cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    /// 当前洗牌顺序（对局存档用）。
    pub(crate) fn order_clone(&self) -> Vec<usize> {
        self.order.clone()
    }

    /// 当前抽卡位置（对局存档用）。
    pub(crate) fn draw_index(&self) -> usize {
        self.draw_index
    }

    /// 恢复洗牌顺序与抽卡位置（对局恢复用）。
    ///
    /// 仅在顺序长度与卡组一致时生效，防御脏数据。
    pub(crate) fn restore(&mut self, order: &[usize], draw_index: usize) {
        if order.len() == self.cards.len() {
            self.order = order.to_vec();
            self.draw_index = draw_index;
        }
    }

    /// 标准「机会」卡组。
    pub fn chance() -> Self {
        Self::new(vec![
            card(1, "前进至起点 GO，领取薪水", CardEffect::MoveTo(0)),
            card(2, "退回 3 格", CardEffect::MoveBy(-3)),
            card(3, "前往最近的铁路", CardEffect::NearestRailroad),
            card(4, "银行错误，得 $200", CardEffect::Collect(200)),
            card(
                5,
                "房屋维修：每栋房 $25，每间旅馆 $100",
                CardEffect::HouseRepair {
                    per_house: 25,
                    per_hotel: 100,
                },
            ),
            card(6, "立即入狱", CardEffect::GoToJail),
            card(7, "前往最近的公用事业", CardEffect::NearestUtility),
            card(8, "获得出狱卡", CardEffect::GetOutOfJail),
            card(9, "每位玩家向你支付 $50", CardEffect::CollectFromEach(50)),
            card(10, "支付学费 $150", CardEffect::Pay(150)),
        ])
    }

    /// 标准「命运」卡组。
    pub fn fate() -> Self {
        Self::new(vec![
            card(11, "退回起点 GO", CardEffect::MoveTo(0)),
            card(12, "立即入狱", CardEffect::GoToJail),
            card(13, "获得出狱卡", CardEffect::GetOutOfJail),
            card(14, "缴税 $100", CardEffect::Pay(100)),
            card(15, "银行付你股息 $50", CardEffect::Collect(50)),
            card(16, "每位玩家向你支付 $50", CardEffect::CollectFromEach(50)),
            card(17, "你向每位玩家支付 $50", CardEffect::PayEach(50)),
            card(
                18,
                "房屋维修：每栋房 $40，每间旅馆 $115",
                CardEffect::HouseRepair {
                    per_house: 40,
                    per_hotel: 115,
                },
            ),
            card(19, "前进 3 格", CardEffect::MoveBy(3)),
            card(20, "遗产所得 $100", CardEffect::Collect(100)),
        ])
    }
}

fn card(id: u32, text: &str, effect: CardEffect) -> Card {
    Card {
        id,
        text: text.to_string(),
        effect,
    }
}
