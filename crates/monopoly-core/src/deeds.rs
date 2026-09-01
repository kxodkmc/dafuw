use monopoly_common::snapshot::TileDto;
use uuid::Uuid;

use crate::board::{Board, Group, TileKind};

/// 银行地产库存默认值。
pub const DEFAULT_HOUSES: u16 = 32;
pub const DEFAULT_HOTELS: u16 = 12;

/// 铁路按持有条数收租：1/2/3/4 条。
const RAILROAD_RENTS: [u32; 4] = [25, 50, 100, 200];

/// 地产登记簿：所有权、房屋、抵押状态，以及租金/建设/抵押等规则计算。
#[derive(Debug, Clone)]
pub struct Deeds {
    owner: Vec<Option<Uuid>>,
    houses: Vec<u8>,
    mortgaged: Vec<bool>,
    houses_available: u16,
    hotels_available: u16,
}

impl Deeds {
    pub fn new(size: usize, houses: u16, hotels: u16) -> Self {
        Self {
            owner: vec![None; size],
            houses: vec![0; size],
            mortgaged: vec![false; size],
            houses_available: houses,
            hotels_available: hotels,
        }
    }

    pub fn owner(&self, tile: usize) -> Option<Uuid> {
        self.owner[tile % self.owner.len()]
    }

    pub fn houses(&self, tile: usize) -> u8 {
        self.houses[tile % self.houses.len()]
    }

    pub fn is_mortgaged(&self, tile: usize) -> bool {
        self.mortgaged[tile % self.mortgaged.len()]
    }

    pub fn houses_available(&self) -> u16 {
        self.houses_available
    }

    pub fn hotels_available(&self) -> u16 {
        self.hotels_available
    }

    /// 变更所有权（购地 / 交易 / 破产移交）。
    pub fn assign(&mut self, tile: usize, player: Uuid) {
        let idx = tile % self.owner.len();
        self.owner[idx] = Some(player);
    }

    fn group_tiles(&self, board: &Board, group: Group) -> Vec<usize> {
        board
            .tiles
            .iter()
            .filter(|t| t.group == Some(group))
            .map(|t| t.id)
            .collect()
    }

    /// 整组是否被同一玩家集齐（垄断）。
    fn group_fully_owned(&self, board: &Board, group: Group, player: Uuid) -> bool {
        self.group_tiles(board, group)
            .iter()
            .all(|&t| self.owner[t] == Some(player))
    }

    /// 建房前置校验：所属、未抵押、垄断、均匀建房、银行库存。
    pub fn can_build(&self, board: &Board, tile: usize) -> Result<(), String> {
        let owner = self.owner(tile).ok_or("该地块无人所有")?;
        if self.mortgaged[tile % self.mortgaged.len()] {
            return Err("抵押中的地块不能建房".into());
        }
        let t = board.tile(tile);
        let group = t.group.ok_or("该地块不可建房")?;
        if !self.group_fully_owned(board, group, owner) {
            return Err("集齐整组地产才能建房（垄断）".into());
        }
        if self.houses(tile) >= 5 {
            return Err("该地块已建满".into());
        }
        for &other in &self.group_tiles(board, group) {
            if self.houses[other] < self.houses[tile % self.houses.len()] {
                return Err("需先在组内其他地块均匀建房".into());
            }
        }
        Ok(())
    }

    /// 建一栋房（或第 4 栋后升级旅馆），返回建造成本。
    pub fn build(&mut self, board: &Board, tile: usize) -> Result<u32, String> {
        self.can_build(board, tile)?;
        let idx = tile % self.houses.len();
        let current = self.houses[idx];
        let cost = board.tile(tile).building_cost;
        if current < 4 {
            if self.houses_available == 0 {
                return Err("银行房屋库存已耗尽".into());
            }
            self.houses[idx] += 1;
            self.houses_available -= 1;
        } else {
            if self.hotels_available == 0 {
                return Err("银行旅馆库存已耗尽".into());
            }
            self.houses[idx] = 5; // 旅馆标记
            self.houses_available += 4; // 回收 4 栋房
            self.hotels_available -= 1;
        }
        Ok(cost)
    }

    /// 拆除一栋房（或旅馆降级），返回卖出价（成本一半）。
    pub fn sell_house(&mut self, board: &Board, tile: usize) -> Result<u32, String> {
        self.owner(tile).ok_or("该地块无人所有")?;
        let idx = tile % self.houses.len();
        let current = self.houses[idx];
        if current == 0 {
            return Err("该地块没有房屋".into());
        }
        let group = board.tile(tile).group.ok_or("该地块不可拆房")?;
        for &other in &self.group_tiles(board, group) {
            if self.houses[other] > current {
                return Err("需先在组内其他地块均匀拆房".into());
            }
        }
        let value = board.tile(tile).building_cost / 2;
        if current == 5 {
            self.houses[idx] = 4;
            self.houses_available += 4;
            self.hotels_available += 1;
        } else {
            self.houses[idx] -= 1;
            self.houses_available += 1;
        }
        Ok(value)
    }

    /// 抵押地产，返回抵押价。
    pub fn mortgage(&mut self, board: &Board, tile: usize) -> Result<u32, String> {
        let idx = tile % self.mortgaged.len();
        if self.mortgaged[idx] {
            return Err("该地块已抵押".into());
        }
        if self.houses[tile % self.houses.len()] > 0 {
            return Err("有房屋的地块需先卖房再抵押".into());
        }
        self.owner(tile).ok_or("该地块无人所有")?;
        self.mortgaged[idx] = true;
        Ok(board.tile(tile).mortgage_value)
    }

    /// 赎回抵押，返回所需金额（抵押价 + 10% 利息）。
    pub fn unmortgage(&mut self, board: &Board, tile: usize) -> Result<u32, String> {
        let idx = tile % self.mortgaged.len();
        if !self.mortgaged[idx] {
            return Err("该地块未抵押".into());
        }
        let value = board.tile(tile).mortgage_value;
        self.mortgaged[idx] = false;
        Ok(value + value / 10)
    }

    pub fn railroad_count(&self, board: &Board, player: Uuid) -> usize {
        board
            .tiles
            .iter()
            .filter(|t| t.kind == TileKind::Railroad && self.owner[t.id] == Some(player))
            .count()
    }

    pub fn utility_count(&self, board: &Board, player: Uuid) -> usize {
        board
            .tiles
            .iter()
            .filter(|t| t.kind == TileKind::Utility && self.owner[t.id] == Some(player))
            .count()
    }

    /// 计算某格应收租金（调用方需保证落在地块上）。
    pub fn rent(&self, board: &Board, tile: usize, dice_sum: u32) -> u32 {
        let idx = tile % board.tiles.len();
        let t = &board.tiles[idx];
        let owner = match self.owner[idx] {
            Some(o) => o,
            None => return 0,
        };
        match t.kind {
            TileKind::Railroad => {
                let n = self.railroad_count(board, owner).clamp(1, 4);
                RAILROAD_RENTS[n - 1]
            }
            TileKind::Utility => {
                let n = self.utility_count(board, owner);
                if n >= 2 {
                    dice_sum * 10
                } else {
                    dice_sum * 4
                }
            }
            TileKind::Property => {
                let h = self.houses[idx];
                if h >= 5 {
                    t.hotel_rent
                } else if h > 0 {
                    t.rent_with_house[(h as usize) - 1]
                } else {
                    let group = t.group.unwrap();
                    if self.group_fully_owned(board, group, owner) {
                        t.base_rent * 2 // 垄断双倍
                    } else {
                        t.base_rent
                    }
                }
            }
            _ => 0,
        }
    }

    /// 玩家拥有的地块列表。
    pub fn owned_tiles(&self, player: Uuid) -> Vec<usize> {
        self.owner
            .iter()
            .enumerate()
            .filter(|(_, o)| **o == Some(player))
            .map(|(i, _)| i)
            .collect()
    }

    /// 把 `from` 的全部地产（含房屋/抵押状态）移交给 `to`（破产/交易用）。
    pub fn transfer_player(&mut self, from: Uuid, to: Uuid) {
        for tile in 0..self.owner.len() {
            if self.owner[tile] == Some(from) {
                self.owner[tile] = Some(to);
            }
        }
    }

    /// 把 `from` 的地产回归市场（清空房屋与抵押），返回地块列表。
    pub fn clear_player(&mut self, from: Uuid) -> Vec<usize> {
        let mut tiles = Vec::new();
        for tile in 0..self.owner.len() {
            if self.owner[tile] == Some(from) {
                self.owner[tile] = None;
                self.houses[tile] = 0;
                self.mortgaged[tile] = false;
                tiles.push(tile);
            }
        }
        tiles
    }

    /// 从快照覆盖地产登记：归属 / 房屋 / 抵押，并恢复银行库存（对局恢复用）。
    pub(crate) fn restore(
        &mut self,
        tiles: &[TileDto],
        houses_available: u16,
        hotels_available: u16,
    ) {
        for t in tiles {
            let idx = t.id % self.owner.len();
            self.owner[idx] = t.owner;
            self.houses[idx] = t.houses;
            self.mortgaged[idx] = t.mortgaged;
        }
        self.houses_available = houses_available;
        self.hotels_available = hotels_available;
    }

    /// 玩家地产总估值（净资产的房产部分）。
    pub fn total_value(&self, board: &Board, player: Uuid) -> i64 {
        self.owned_tiles(player)
            .iter()
            .map(|&t| {
                let tile = board.tile(t);
                (tile.price + self.houses[t % self.houses.len()] as u32 * tile.building_cost) as i64
            })
            .sum()
    }

    pub fn total_houses(&self, player: Uuid) -> u32 {
        self.owned_tiles(player)
            .iter()
            .filter(|&&t| self.houses[t] > 0 && self.houses[t] < 5)
            .map(|&t| self.houses[t] as u32)
            .sum()
    }

    pub fn total_hotels(&self, player: Uuid) -> u32 {
        self.owned_tiles(player)
            .iter()
            .filter(|&&t| self.houses[t] == 5)
            .count() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::classic_board;
    use uuid::Uuid;

    fn deeds() -> Deeds {
        Deeds::new(classic_board().len(), DEFAULT_HOUSES, DEFAULT_HOTELS)
    }

    #[test]
    fn even_build_rule() {
        let board = classic_board();
        let mut d = deeds();
        let p = Uuid::new_v4();
        d.assign(1, p);
        d.assign(3, p); // 集齐棕组
        assert!(d.can_build(&board, 1).is_ok());
        assert_eq!(d.build(&board, 1).unwrap(), 50);
        // 3 号还没建，1 号不能再建第二栋
        assert!(d.can_build(&board, 1).is_err());
        d.build(&board, 3).unwrap();
        assert!(d.can_build(&board, 1).is_ok());
        assert_eq!(d.houses(1), 1);
    }

    #[test]
    fn monopoly_doubles_rent() {
        let board = classic_board();
        let mut d = deeds();
        let p = Uuid::new_v4();
        d.assign(1, p);
        // 未垄断：原价
        assert_eq!(d.rent(&board, 1, 0), 2);
        // 集齐棕组：翻倍
        d.assign(3, p);
        assert_eq!(d.rent(&board, 1, 0), 4);
        // 1 栋房
        d.build(&board, 1).unwrap();
        assert_eq!(d.rent(&board, 1, 0), 10);
    }

    #[test]
    fn railroad_and_utility_rent() {
        let board = classic_board();
        let mut d = deeds();
        let p = Uuid::new_v4();
        d.assign(5, p); // 国王十字车站
        assert_eq!(d.rent(&board, 5, 0), 25);
        d.assign(15, p);
        assert_eq!(d.rent(&board, 5, 0), 50);
        d.assign(12, p); // 电力公司
        assert_eq!(d.rent(&board, 12, 7), 7 * 4);
        d.assign(28, p); // 水厂 → 两公用事业 10 倍
        assert_eq!(d.rent(&board, 12, 7), 7 * 10);
    }

    #[test]
    fn mortgage_flow() {
        let board = classic_board();
        let mut d = deeds();
        let p = Uuid::new_v4();
        d.assign(1, p);
        assert_eq!(d.mortgage(&board, 1).unwrap(), 30);
        assert!(d.is_mortgaged(1));
        assert_eq!(d.unmortgage(&board, 1).unwrap(), 33); // 30 + 10%
        assert!(!d.is_mortgaged(1));
    }
}
