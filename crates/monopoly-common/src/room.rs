use serde::{Deserialize, Serialize};

/// 8 位数字房间号（10000000..=99999999）。
///
/// 统一以 `u32` 存储，展示时按 8 位补零，保证协议与日志中格式一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomCode(u32);

impl RoomCode {
    pub const MIN: u32 = 10_000_000;
    pub const MAX: u32 = 99_999_999;

    pub fn new(code: u32) -> Result<Self, String> {
        if (Self::MIN..=Self::MAX).contains(&code) {
            Ok(Self(code))
        } else {
            Err(format!("房间号必须是 8 位数字，收到: {code}"))
        }
    }

    pub fn as_u32(self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for RoomCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:08}", self.0)
    }
}

/// 房间自定义配置。后续在此扩展：地图大小、玩法开关等。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RoomSettings {
    /// 房间人数上限（2..=8）。
    pub player_count_max: u8,
    /// 每位玩家初始资金。
    pub initial_money: u32,
    /// 骰子数量（默认 2）。
    pub dice_count: u8,
    /// 棋盘格数（当前版本仅支持经典 40 格，接口为后续自定义地图预留）。
    pub board_size: u16,
    /// 经过/停留 GO 领取的薪水。
    pub go_salary: u32,
    /// 限时回合数；`None` 表示不限。
    pub max_rounds: Option<u32>,
}

impl Default for RoomSettings {
    fn default() -> Self {
        Self {
            player_count_max: 8,
            initial_money: 1500,
            dice_count: 2,
            board_size: 40,
            go_salary: 200,
            max_rounds: None,
        }
    }
}

impl RoomSettings {
    /// 开局前的参数校验，非法配置直接拒绝建房。
    pub fn validate(&self) -> Result<(), String> {
        if !(2..=8).contains(&self.player_count_max) {
            return Err("玩家人数需在 2..=8 之间".to_string());
        }
        if self.initial_money == 0 {
            return Err("初始资金不能为 0".to_string());
        }
        if self.dice_count == 0 {
            return Err("骰子数量不能为 0".to_string());
        }
        if self.go_salary == 0 {
            return Err("GO 薪水不能为 0".to_string());
        }
        if self.board_size != 40 {
            return Err("当前版本仅支持经典 40 格棋盘，自定义地图将在后续版本开放".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_code_accepts_8_digit_range() {
        assert!(RoomCode::new(10_000_000).is_ok());
        assert!(RoomCode::new(99_999_999).is_ok());
    }

    #[test]
    fn room_code_rejects_out_of_range() {
        assert!(RoomCode::new(0).is_err());
        assert!(RoomCode::new(9_999_999).is_err());
        assert!(RoomCode::new(100_000_000).is_err());
    }

    #[test]
    fn room_code_displays_zero_padded() {
        assert_eq!(RoomCode::new(12_345_678).unwrap().to_string(), "12345678");
        assert_eq!(RoomCode::new(10_000_000).unwrap().to_string(), "10000000");
        assert_eq!(RoomCode::new(10_000_000).unwrap().as_u32(), 10_000_000);
    }

    #[test]
    fn settings_serde_with_partial_fields_defaults() {
        // 缺省字段自动取默认值。
        let s: RoomSettings = serde_json::from_str(r#"{"player_count_max":4}"#).unwrap();
        assert_eq!(s.player_count_max, 4);
        assert_eq!(s.initial_money, 1500);
        assert_eq!(s.dice_count, 2);
        assert_eq!(s.board_size, 40);

        // 空对象 = 全默认。
        let s: RoomSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, RoomSettings::default());

        // 序列化后能完整往返。
        let json = serde_json::to_string(&s).unwrap();
        let back: RoomSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn settings_validation_rules() {
        let mut s = RoomSettings::default();
        assert!(s.validate().is_ok());

        s.player_count_max = 1;
        assert!(s.validate().is_err());
        s.player_count_max = 9;
        assert!(s.validate().is_err());
        s.player_count_max = 8;
        assert!(s.validate().is_ok());

        s.initial_money = 0;
        assert!(s.validate().is_err());
        s.initial_money = 1000;
        assert!(s.validate().is_ok());

        s.dice_count = 0;
        assert!(s.validate().is_err());
        s.dice_count = 3;
        assert!(s.validate().is_ok());

        s.go_salary = 0;
        assert!(s.validate().is_err());
        s.go_salary = 200;
        assert!(s.validate().is_ok());

        s.board_size = 80;
        assert!(s.validate().is_err());
        s.board_size = 40;
        assert!(s.validate().is_ok());
    }
}
