use serde::{Deserialize, Serialize};

/// 地产色组（垄断判定的依据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Group {
    Brown,
    LightBlue,
    Pink,
    Orange,
    Red,
    Yellow,
    Green,
    DarkBlue,
}

/// 格子类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TileKind {
    Go,
    Property,
    Railroad,
    Utility,
    Chance,
    Fate,
    Tax,
    Jail,
    GoToJail,
    FreeParking,
}

/// 单格定义（数据化，便于自定义地图扩展）。
#[derive(Debug, Clone)]
pub struct Tile {
    pub id: usize,
    pub name: String,
    pub kind: TileKind,
    pub group: Option<Group>,
    pub price: u32,
    pub base_rent: u32,
    pub rent_with_house: [u32; 4],
    pub hotel_rent: u32,
    pub building_cost: u32,
    pub mortgage_value: u32,
    /// 税额（所得税/超级税），0 表示该格非税格。
    pub tax: u32,
    /// 是否为所得税（$2M 或总资产 10% 取较高者）。
    pub income_tax: bool,
}

/// 环形棋盘。
#[derive(Debug, Clone)]
pub struct Board {
    pub tiles: Vec<Tile>,
}

impl Board {
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    pub fn tile(&self, id: usize) -> &Tile {
        &self.tiles[id % self.tiles.len()]
    }

    pub fn jail_index(&self) -> usize {
        self.tiles
            .iter()
            .find(|t| t.kind == TileKind::Jail)
            .map(|t| t.id)
            .unwrap_or(10)
    }

    /// 从 `from` 起向前找第一个满足条件的格子。
    pub fn nearest_ahead(&self, from: usize, pred: impl Fn(&Tile) -> bool) -> usize {
        let n = self.tiles.len();
        for step in 1..=n {
            let idx = (from + step) % n;
            if pred(&self.tiles[idx]) {
                return idx;
            }
        }
        from
    }

    pub fn nearest_railroad(&self, from: usize) -> usize {
        self.nearest_ahead(from, |t| t.kind == TileKind::Railroad)
    }

    pub fn nearest_utility(&self, from: usize) -> usize {
        self.nearest_ahead(from, |t| t.kind == TileKind::Utility)
    }
}

/// 构建棋盘。支持 40 格（经典世界版）与 56 格（扩展世界版）。
pub fn build_board(size: usize) -> Result<Board, String> {
    match size {
        40 => Ok(world_board()),
        56 => Ok(world_board_56()),
        _ => Err(format!("暂不支持 {size} 格棋盘，当前支持 40 / 56 格")),
    }
}

const K: u32 = 10_000; // 经典版金额 → 官方世界版金额的倍率

#[allow(clippy::too_many_arguments)]
fn prop(
    id: usize,
    name: &str,
    group: Group,
    price: u32,
    base: u32,
    houses: [u32; 4],
    hotel: u32,
    cost: u32,
) -> Tile {
    Tile {
        id,
        name: name.to_string(),
        kind: TileKind::Property,
        group: Some(group),
        price,
        base_rent: base,
        rent_with_house: houses,
        hotel_rent: hotel,
        building_cost: cost,
        mortgage_value: price / 2,
        tax: 0,
        income_tax: false,
    }
}

fn simple(id: usize, name: &str, kind: TileKind) -> Tile {
    Tile {
        id,
        name: name.to_string(),
        kind,
        group: None,
        price: 0,
        base_rent: 0,
        rent_with_house: [0; 4],
        hotel_rent: 0,
        building_cost: 0,
        mortgage_value: 0,
        tax: 0,
        income_tax: false,
    }
}

fn railroad(id: usize, name: &str) -> Tile {
    Tile {
        id,
        name: name.to_string(),
        kind: TileKind::Railroad,
        group: None,
        price: 200 * K,
        base_rent: 0,
        rent_with_house: [0; 4],
        hotel_rent: 0,
        building_cost: 0,
        mortgage_value: 100 * K,
        tax: 0,
        income_tax: false,
    }
}

fn utility(id: usize, name: &str) -> Tile {
    Tile {
        id,
        name: name.to_string(),
        kind: TileKind::Utility,
        group: None,
        price: 150 * K,
        base_rent: 0,
        rent_with_house: [0; 4],
        hotel_rent: 0,
        building_cost: 0,
        mortgage_value: 75 * K,
        tax: 0,
        income_tax: false,
    }
}

fn tax(id: usize, name: &str, amount: u32, income: bool) -> Tile {
    Tile {
        id,
        name: name.to_string(),
        kind: TileKind::Tax,
        group: None,
        price: 0,
        base_rent: 0,
        rent_with_house: [0; 4],
        hotel_rent: 0,
        building_cost: 0,
        mortgage_value: 0,
        tax: amount,
        income_tax: income,
    }
}

/// 完全复刻官方 MONOPOLY Here & Now: The World Edition（2008，560 万全球投票）：
/// 22 座世界城市 + 大富翁铁路/航空/邮轮/航天 + 太阳能/风能电站。
/// 布局与经典 40 格完全一致；全部金额 = 经典版 ×10000（起薪 2M、初始 15M、地价 600K~4M）。
pub fn world_board() -> Board {
    // 22 城按官方投票排名升序（位置与经典版地块一一对应，租金同步 ×10000）。
    let mut tiles = vec![
        simple(0, "起点 GO", TileKind::Go),
        // 棕组：外卡城市（最便宜）
        prop(1, "格丁尼亚", Group::Brown, 60 * K, 2 * K, [10 * K, 30 * K, 90 * K, 160 * K], 250 * K, 50 * K),
        simple(2, "命运", TileKind::Fate),
        prop(3, "台北", Group::Brown, 60 * K, 4 * K, [20 * K, 60 * K, 180 * K, 320 * K], 450 * K, 50 * K),
        tax(4, "所得税", 200 * K, true),
        railroad(5, "大富翁铁路"),
        // 浅蓝组
        prop(6, "东京", Group::LightBlue, 100 * K, 6 * K, [30 * K, 90 * K, 270 * K, 400 * K], 550 * K, 50 * K),
        simple(7, "机会", TileKind::Chance),
        prop(8, "巴塞罗那", Group::LightBlue, 100 * K, 6 * K, [30 * K, 90 * K, 270 * K, 400 * K], 550 * K, 50 * K),
        prop(9, "雅典", Group::LightBlue, 120 * K, 8 * K, [40 * K, 100 * K, 300 * K, 450 * K], 600 * K, 50 * K),
        simple(10, "监狱 / 探访", TileKind::Jail),
        // 粉红组
        prop(11, "伊斯坦布尔", Group::Pink, 140 * K, 10 * K, [50 * K, 150 * K, 450 * K, 625 * K], 750 * K, 100 * K),
        utility(12, "太阳能电站"),
        prop(13, "基辅", Group::Pink, 140 * K, 10 * K, [50 * K, 150 * K, 450 * K, 625 * K], 750 * K, 100 * K),
        prop(14, "多伦多", Group::Pink, 160 * K, 12 * K, [60 * K, 180 * K, 500 * K, 700 * K], 900 * K, 100 * K),
        railroad(15, "大富翁航空"),
        // 橙色组
        prop(16, "罗马", Group::Orange, 180 * K, 14 * K, [70 * K, 200 * K, 550 * K, 750 * K], 950 * K, 100 * K),
        simple(17, "命运", TileKind::Fate),
        prop(18, "上海", Group::Orange, 180 * K, 14 * K, [70 * K, 200 * K, 550 * K, 750 * K], 950 * K, 100 * K),
        prop(19, "温哥华", Group::Orange, 200 * K, 16 * K, [80 * K, 220 * K, 600 * K, 800 * K], 1000 * K, 100 * K),
        simple(20, "免费停车", TileKind::FreeParking),
        // 红色组
        prop(21, "悉尼", Group::Red, 220 * K, 18 * K, [90 * K, 250 * K, 700 * K, 875 * K], 1050 * K, 150 * K),
        simple(22, "机会", TileKind::Chance),
        prop(23, "纽约", Group::Red, 240 * K, 20 * K, [100 * K, 300 * K, 750 * K, 925 * K], 1100 * K, 150 * K),
        prop(24, "伦敦", Group::Red, 260 * K, 22 * K, [110 * K, 330 * K, 800 * K, 975 * K], 1150 * K, 150 * K),
        railroad(25, "大富翁邮轮"),
        // 黄色组
        prop(26, "北京", Group::Yellow, 260 * K, 22 * K, [110 * K, 330 * K, 800 * K, 975 * K], 1150 * K, 150 * K),
        prop(27, "香港", Group::Yellow, 260 * K, 22 * K, [110 * K, 330 * K, 800 * K, 975 * K], 1150 * K, 150 * K),
        utility(28, "风能电站"),
        prop(29, "耶路撒冷", Group::Yellow, 280 * K, 24 * K, [120 * K, 360 * K, 850 * K, 1025 * K], 1200 * K, 150 * K),
        simple(30, "入狱", TileKind::GoToJail),
        // 绿色组
        prop(31, "巴黎", Group::Green, 300 * K, 26 * K, [130 * K, 390 * K, 900 * K, 1100 * K], 1275 * K, 200 * K),
        prop(32, "贝尔格莱德", Group::Green, 300 * K, 26 * K, [130 * K, 390 * K, 900 * K, 1100 * K], 1275 * K, 200 * K),
        simple(33, "命运", TileKind::Fate),
        prop(34, "开普敦", Group::Green, 320 * K, 28 * K, [150 * K, 450 * K, 1000 * K, 1200 * K], 1400 * K, 200 * K),
        railroad(35, "大富翁航天"),
        simple(36, "机会", TileKind::Chance),
        // 深蓝组：里加（亚军）、蒙特利尔（全球投票冠军 → 最贵）
        prop(37, "里加", Group::DarkBlue, 350 * K, 35 * K, [175 * K, 500 * K, 1100 * K, 1300 * K], 1500 * K, 200 * K),
        tax(38, "超级税", 100 * K, false),
        prop(39, "蒙特利尔", Group::DarkBlue, 400 * K, 50 * K, [200 * K, 600 * K, 1400 * K, 1700 * K], 2000 * K, 200 * K),
    ];

    // 修正 id 与下标一致。
    for (i, t) in tiles.iter_mut().enumerate() {
        t.id = i;
    }

    Board { tiles }
}

/// 56 格扩展世界版：每色组新增 2 座城市（共 38 城），15x15 网格。
/// 角格位于 0/14/28/42；机会/命运/税务/车站/电站按经典节奏分布于四边。
/// 金额量纲与 40 格版一致；组内城市沿用该色组的地价/租金阶梯。
pub fn world_board_56() -> Board {
    let mut tiles = vec![
        simple(0, "起点 GO", TileKind::Go),
        // ── 底边 1..14：棕组 ×4 + 浅蓝组 ×5 ──
        prop(1, "格丁尼亚", Group::Brown, 60 * K, 2 * K, [10 * K, 30 * K, 90 * K, 160 * K], 250 * K, 50 * K),
        prop(2, "河内", Group::Brown, 60 * K, 2 * K, [10 * K, 30 * K, 90 * K, 160 * K], 250 * K, 50 * K),
        simple(3, "命运", TileKind::Fate),
        prop(4, "台北", Group::Brown, 60 * K, 4 * K, [20 * K, 60 * K, 180 * K, 320 * K], 450 * K, 50 * K),
        prop(5, "内罗毕", Group::Brown, 60 * K, 4 * K, [20 * K, 60 * K, 180 * K, 320 * K], 450 * K, 50 * K),
        tax(6, "所得税", 200 * K, true),
        railroad(7, "大富翁铁路"),
        prop(8, "东京", Group::LightBlue, 100 * K, 6 * K, [30 * K, 90 * K, 270 * K, 400 * K], 550 * K, 50 * K),
        simple(9, "机会", TileKind::Chance),
        prop(10, "巴塞罗那", Group::LightBlue, 100 * K, 6 * K, [30 * K, 90 * K, 270 * K, 400 * K], 550 * K, 50 * K),
        prop(11, "雅典", Group::LightBlue, 120 * K, 8 * K, [40 * K, 100 * K, 300 * K, 450 * K], 600 * K, 50 * K),
        prop(12, "曼谷", Group::LightBlue, 120 * K, 8 * K, [40 * K, 100 * K, 300 * K, 450 * K], 600 * K, 50 * K),
        prop(13, "里斯本", Group::LightBlue, 120 * K, 8 * K, [40 * K, 100 * K, 300 * K, 450 * K], 600 * K, 50 * K),
        simple(14, "监狱 / 探访", TileKind::Jail),
        // ── 左边 15..28：粉红组 ×5 + 橙色组 ×5 ──
        prop(15, "伊斯坦布尔", Group::Pink, 140 * K, 10 * K, [50 * K, 150 * K, 450 * K, 625 * K], 750 * K, 100 * K),
        prop(16, "基辅", Group::Pink, 140 * K, 10 * K, [50 * K, 150 * K, 450 * K, 625 * K], 750 * K, 100 * K),
        utility(17, "太阳能电站"),
        prop(18, "多伦多", Group::Pink, 140 * K, 10 * K, [50 * K, 150 * K, 450 * K, 625 * K], 750 * K, 100 * K),
        prop(19, "布达佩斯", Group::Pink, 160 * K, 12 * K, [60 * K, 180 * K, 500 * K, 700 * K], 900 * K, 100 * K),
        prop(20, "华沙", Group::Pink, 160 * K, 12 * K, [60 * K, 180 * K, 500 * K, 700 * K], 900 * K, 100 * K),
        railroad(21, "大富翁航空"),
        prop(22, "罗马", Group::Orange, 180 * K, 14 * K, [70 * K, 200 * K, 550 * K, 750 * K], 950 * K, 100 * K),
        simple(23, "命运", TileKind::Fate),
        prop(24, "上海", Group::Orange, 180 * K, 14 * K, [70 * K, 200 * K, 550 * K, 750 * K], 950 * K, 100 * K),
        prop(25, "米兰", Group::Orange, 180 * K, 14 * K, [70 * K, 200 * K, 550 * K, 750 * K], 950 * K, 100 * K),
        prop(26, "温哥华", Group::Orange, 200 * K, 16 * K, [80 * K, 220 * K, 600 * K, 800 * K], 1000 * K, 100 * K),
        prop(27, "新加坡", Group::Orange, 200 * K, 16 * K, [80 * K, 220 * K, 600 * K, 800 * K], 1000 * K, 100 * K),
        simple(28, "免费停车", TileKind::FreeParking),
        // ── 顶边 29..42：红色组 ×5 + 黄色组 ×5 ──
        prop(29, "悉尼", Group::Red, 220 * K, 18 * K, [90 * K, 250 * K, 700 * K, 875 * K], 1050 * K, 150 * K),
        prop(30, "芝加哥", Group::Red, 220 * K, 18 * K, [90 * K, 250 * K, 700 * K, 875 * K], 1050 * K, 150 * K),
        simple(31, "机会", TileKind::Chance),
        prop(32, "纽约", Group::Red, 240 * K, 20 * K, [100 * K, 300 * K, 750 * K, 925 * K], 1100 * K, 150 * K),
        prop(33, "旧金山", Group::Red, 240 * K, 20 * K, [100 * K, 300 * K, 750 * K, 925 * K], 1100 * K, 150 * K),
        prop(34, "伦敦", Group::Red, 260 * K, 22 * K, [110 * K, 330 * K, 800 * K, 975 * K], 1150 * K, 150 * K),
        railroad(35, "大富翁邮轮"),
        prop(36, "北京", Group::Yellow, 260 * K, 22 * K, [110 * K, 330 * K, 800 * K, 975 * K], 1150 * K, 150 * K),
        prop(37, "香港", Group::Yellow, 260 * K, 22 * K, [110 * K, 330 * K, 800 * K, 975 * K], 1150 * K, 150 * K),
        utility(38, "风能电站"),
        prop(39, "耶路撒冷", Group::Yellow, 280 * K, 24 * K, [120 * K, 360 * K, 850 * K, 1025 * K], 1200 * K, 150 * K),
        prop(40, "慕尼黑", Group::Yellow, 280 * K, 24 * K, [120 * K, 360 * K, 850 * K, 1025 * K], 1200 * K, 150 * K),
        prop(41, "首尔", Group::Yellow, 280 * K, 24 * K, [120 * K, 360 * K, 850 * K, 1025 * K], 1200 * K, 150 * K),
        simple(42, "入狱", TileKind::GoToJail),
        // ── 右边 43..56：绿色组 ×5 + 深蓝组 ×4 ──
        prop(43, "巴黎", Group::Green, 300 * K, 26 * K, [130 * K, 390 * K, 900 * K, 1100 * K], 1275 * K, 200 * K),
        prop(44, "柏林", Group::Green, 300 * K, 26 * K, [130 * K, 390 * K, 900 * K, 1100 * K], 1275 * K, 200 * K),
        prop(45, "贝尔格莱德", Group::Green, 300 * K, 26 * K, [130 * K, 390 * K, 900 * K, 1100 * K], 1275 * K, 200 * K),
        prop(46, "迪拜", Group::Green, 320 * K, 28 * K, [150 * K, 450 * K, 1000 * K, 1200 * K], 1400 * K, 200 * K),
        prop(47, "开普敦", Group::Green, 320 * K, 28 * K, [150 * K, 450 * K, 1000 * K, 1200 * K], 1400 * K, 200 * K),
        simple(48, "命运", TileKind::Fate),
        railroad(49, "大富翁航天"),
        simple(50, "机会", TileKind::Chance),
        // 深蓝组：里加（亚军）、蒙特利尔（冠军 → 最贵）
        prop(51, "里加", Group::DarkBlue, 350 * K, 35 * K, [175 * K, 500 * K, 1100 * K, 1300 * K], 1500 * K, 200 * K),
        prop(52, "摩纳哥", Group::DarkBlue, 350 * K, 35 * K, [175 * K, 500 * K, 1100 * K, 1300 * K], 1500 * K, 200 * K),
        tax(53, "超级税", 100 * K, false),
        prop(54, "洛杉矶", Group::DarkBlue, 400 * K, 50 * K, [200 * K, 600 * K, 1400 * K, 1700 * K], 2000 * K, 200 * K),
        prop(55, "蒙特利尔", Group::DarkBlue, 400 * K, 50 * K, [200 * K, 600 * K, 1400 * K, 1700 * K], 2000 * K, 200 * K),
    ];

    // 修正 id 与下标一致。
    for (i, t) in tiles.iter_mut().enumerate() {
        t.id = i;
    }

    Board { tiles }
}
