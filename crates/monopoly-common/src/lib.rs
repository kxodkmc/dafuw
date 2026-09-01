//! 协议层：跨 crate 共享的类型、指令、事件、快照与错误。
//!
//! 本 crate 只做数据定义，不含业务逻辑，供 core / server / 未来客户端复用。

pub mod archive;
pub mod command;
pub mod error;
pub mod event;
pub mod room;
pub mod snapshot;

pub use archive::{ArchiveExtra, AuctionDto, GameArchive, TradeDto};
pub use command::{AssetList, Command};
pub use error::{ApiError, ErrorBody, ErrorKind};
pub use event::{ChatPayload, Event, PlayerRank};
pub use room::{RoomCode, RoomSettings, RoomSummary};
pub use snapshot::{GameStateDto, PlayerDto, TileDto};
