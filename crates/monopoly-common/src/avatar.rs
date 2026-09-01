//! 玩家形象与颜色：入座时自选，房间内「形象 + 颜色」组合唯一。

use serde::{Deserialize, Serialize};

/// 玩家形象（棋子造型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Avatar {
    Dog,
    Car,
    Bicycle,
    Tree,
    Bed,
}

impl Avatar {
    /// 全部可选形象（供客户端渲染选择器）。
    pub const ALL: [Avatar; 5] = [
        Avatar::Dog,
        Avatar::Car,
        Avatar::Bicycle,
        Avatar::Tree,
        Avatar::Bed,
    ];
}

impl std::fmt::Display for Avatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Avatar::Dog => "狗",
            Avatar::Car => "轿车",
            Avatar::Bicycle => "单车",
            Avatar::Tree => "大树",
            Avatar::Bed => "床",
        })
    }
}

/// 玩家颜色。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Red,
    Yellow,
    Green,
    Blue,
    Pink,
}

impl Color {
    /// 全部可选颜色（供客户端渲染选择器）。
    pub const ALL: [Color; 5] = [
        Color::Red,
        Color::Yellow,
        Color::Green,
        Color::Blue,
        Color::Pink,
    ];
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Color::Red => "红色",
            Color::Yellow => "黄色",
            Color::Green => "绿色",
            Color::Blue => "蓝色",
            Color::Pink => "粉色",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_and_display() {
        for avatar in Avatar::ALL {
            let json = serde_json::to_string(&avatar).unwrap();
            let back: Avatar = serde_json::from_str(&json).unwrap();
            assert_eq!(avatar, back);
            assert!(!avatar.to_string().is_empty());
        }
        for color in Color::ALL {
            let json = serde_json::to_string(&color).unwrap();
            let back: Color = serde_json::from_str(&json).unwrap();
            assert_eq!(color, back);
            assert!(!color.to_string().is_empty());
        }
    }

    #[test]
    fn wire_format_is_snake_case() {
        assert_eq!(serde_json::to_string(&Avatar::Dog).unwrap(), "\"dog\"");
        assert_eq!(serde_json::to_string(&Avatar::Bicycle).unwrap(), "\"bicycle\"");
        assert_eq!(serde_json::to_string(&Color::Pink).unwrap(), "\"pink\"");
    }

    #[test]
    fn invalid_variant_is_rejected() {
        assert!(serde_json::from_str::<Avatar>("\"dragon\"").is_err());
        assert!(serde_json::from_str::<Color>("\"purple\"").is_err());
    }
}
