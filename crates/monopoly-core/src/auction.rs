use uuid::Uuid;

/// 银行拍卖状态机。
///
/// 规则：所有未破产玩家按固定顺序出价；轮到某玩家时出价或弃权；
/// 出价必须高于当前最高价；一旦弃权不可再参与。
/// 当除最高出价者外没有可出价的玩家时，拍卖结束，最高出价者获胜（无人出价则流拍）。
#[derive(Debug, Clone)]
pub struct Auction {
    pub tile_id: usize,
    order: Vec<Uuid>,
    turn: usize,
    pub highest_bid: u32,
    pub highest_bidder: Option<Uuid>,
    passed: Vec<Uuid>,
}

impl Auction {
    pub fn new(tile_id: usize, order: Vec<Uuid>, starter: Uuid) -> Self {
        let turn = order.iter().position(|&p| p == starter).unwrap_or(0);
        Self {
            tile_id,
            order,
            turn,
            highest_bid: 0,
            highest_bidder: None,
            passed: Vec::new(),
        }
    }

    /// 当前应出价/弃权的玩家（跳过已弃权者与当前最高出价者）。
    fn current(&self) -> Option<Uuid> {
        let n = self.order.len();
        for step in 0..n {
            let idx = (self.turn + step) % n;
            let pid = self.order[idx];
            if self.passed.contains(&pid) {
                continue;
            }
            if self.highest_bidder == Some(pid) {
                continue;
            }
            return Some(pid);
        }
        None
    }

    /// 出价。
    pub fn bid(&mut self, pid: Uuid, amount: u32) -> Result<(), String> {
        if self.current() != Some(pid) {
            return Err("还没轮到你出价".into());
        }
        if amount == 0 {
            return Err("出价必须大于 0".into());
        }
        if amount <= self.highest_bid {
            return Err("出价需高于当前最高价".into());
        }
        self.highest_bid = amount;
        self.highest_bidder = Some(pid);
        self.advance_turn();
        Ok(())
    }

    /// 弃权。
    pub fn pass(&mut self, pid: Uuid) -> Result<(), String> {
        if self.current() != Some(pid) {
            return Err("还没轮到你".into());
        }
        self.passed.push(pid);
        self.advance_turn();
        Ok(())
    }

    fn advance_turn(&mut self) {
        let n = self.order.len();
        for step in 1..=n {
            let idx = (self.turn + step) % n;
            let pid = self.order[idx];
            if self.passed.contains(&pid) {
                continue;
            }
            if self.highest_bidder == Some(pid) {
                continue;
            }
            self.turn = idx;
            return;
        }
    }

    pub fn is_finished(&self) -> bool {
        self.current().is_none()
    }

    pub fn winner(&self) -> Option<Uuid> {
        self.highest_bidder
    }

    /// 当前应出价/弃权的玩家（用于服务器托管自动处理）。
    pub fn current_bidder(&self) -> Option<Uuid> {
        self.current()
    }
}
