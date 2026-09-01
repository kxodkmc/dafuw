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
    /// 税额（所得税/奢侈品税），0 表示该格非税格。
    pub tax: u32,
    /// 是否为所得税（$200 或总资产 10% 取较高者）。
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

/// 构建棋盘。当前版本仅支持经典 40 格；自定义尺寸在后续版本通过此入口扩展。
pub fn build_board(size: usize) -> Result<Board, String> {
    if size == 40 {
        Ok(classic_board())
    } else {
        Err(format!("暂不支持 {size} 格棋盘，当前仅支持经典 40 格"))
    }
}

/// 经典 40 格棋盘（位置、价格、租金参考标准大富翁）。
pub fn classic_board() -> Board {
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
            price: 200,
            base_rent: 25,
            rent_with_house: [0; 4],
            hotel_rent: 0,
            building_cost: 0,
            mortgage_value: 100,
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
            price: 150,
            base_rent: 0,
            rent_with_house: [0; 4],
            hotel_rent: 0,
            building_cost: 0,
            mortgage_value: 75,
            tax: 0,
            income_tax: false,
        }
    }

    let mut tiles = vec![
        simple(0, "起点 GO", TileKind::Go),
        prop(
            1,
            "地中海大道",
            Group::Brown,
            60,
            2,
            [10, 30, 90, 160],
            250,
            50,
        ),
        simple(2, "命运", TileKind::Fate),
        prop(
            3,
            "波罗的海大道",
            Group::Brown,
            60,
            4,
            [20, 60, 180, 320],
            450,
            50,
        ),
        tax(4, "所得税", 200, true),
        railroad(5, "国王十字车站"),
        prop(
            6,
            "东方大道",
            Group::LightBlue,
            100,
            6,
            [30, 90, 270, 400],
            550,
            50,
        ),
        simple(7, "机会", TileKind::Chance),
        prop(
            8,
            "佛蒙特大道",
            Group::LightBlue,
            100,
            6,
            [30, 90, 270, 400],
            550,
            50,
        ),
        prop(
            9,
            "康涅狄格大道",
            Group::LightBlue,
            120,
            8,
            [40, 100, 300, 450],
            600,
            50,
        ),
        simple(10, "监狱 / 探访", TileKind::Jail),
        prop(
            11,
            "圣查尔斯大道",
            Group::Pink,
            140,
            10,
            [50, 150, 450, 625],
            750,
            100,
        ),
        utility(12, "电力公司"),
        prop(
            13,
            "弗吉尼亚大道",
            Group::Pink,
            140,
            10,
            [50, 150, 450, 625],
            750,
            100,
        ),
        prop(
            14,
            "圣詹姆斯大道",
            Group::Pink,
            160,
            12,
            [60, 180, 500, 700],
            900,
            100,
        ),
        railroad(15, "宾夕法尼亚车站"),
        prop(
            16,
            "田纳西大道",
            Group::Orange,
            180,
            14,
            [70, 200, 550, 750],
            950,
            100,
        ),
        simple(17, "命运", TileKind::Fate),
        prop(
            18,
            "纽约大道",
            Group::Orange,
            180,
            14,
            [70, 200, 550, 750],
            950,
            100,
        ),
        prop(
            19,
            "肯塔基大道",
            Group::Orange,
            200,
            16,
            [80, 220, 600, 800],
            1000,
            100,
        ),
        simple(20, "免费停车", TileKind::FreeParking),
        prop(
            21,
            "印第安纳大道",
            Group::Red,
            220,
            18,
            [90, 250, 700, 875],
            1050,
            150,
        ),
        simple(22, "机会", TileKind::Chance),
        prop(
            23,
            "伊利诺伊大道",
            Group::Red,
            240,
            20,
            [100, 300, 750, 925],
            1100,
            150,
        ),
        prop(
            24,
            "大西洋大道",
            Group::Red,
            260,
            22,
            [110, 330, 800, 975],
            1150,
            150,
        ),
        railroad(25, "B&O 车站"),
        prop(
            26,
            "文特诺大道",
            Group::Yellow,
            260,
            22,
            [110, 330, 800, 975],
            1150,
            150,
        ),
        prop(
            27,
            "马尔文花园大道",
            Group::Yellow,
            260,
            22,
            [110, 330, 800, 975],
            1150,
            150,
        ),
        utility(28, "水厂"),
        prop(
            29,
            "太平洋大道",
            Group::Yellow,
            280,
            24,
            [120, 360, 850, 1025],
            1200,
            150,
        ),
        simple(30, "入狱", TileKind::GoToJail),
        prop(
            31,
            "北卡罗来纳大道",
            Group::Green,
            300,
            26,
            [130, 390, 900, 1100],
            1275,
            200,
        ),
        prop(
            32,
            "宾夕法尼亚大道",
            Group::Green,
            300,
            26,
            [130, 390, 900, 1100],
            1275,
            200,
        ),
        simple(33, "命运", TileKind::Fate),
        prop(
            34,
            "太平洋大道",
            Group::Green,
            320,
            28,
            [150, 450, 1000, 1200],
            1400,
            200,
        ),
        railroad(35, "短线车站"),
        simple(36, "机会", TileKind::Chance),
        prop(
            37,
            "公园大道",
            Group::DarkBlue,
            350,
            35,
            [175, 500, 1100, 1300],
            1500,
            200,
        ),
        tax(38, "奢侈品税", 100, false),
        prop(
            39,
            "五月花大道",
            Group::DarkBlue,
            400,
            50,
            [200, 600, 1400, 1700],
            2000,
            200,
        ),
    ];

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

    // 修正 id 与下标一致（上面 `tax` 借用顺序问题，此处重新赋值）。
    for (i, t) in tiles.iter_mut().enumerate() {
        t.id = i;
    }

    Board { tiles }
}
