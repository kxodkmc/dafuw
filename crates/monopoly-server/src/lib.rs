//! 大富翁服务器：Axum HTTP + WebSocket 网关，房间 Actor 化承载规则引擎。

pub mod actor;
pub mod app;
pub mod error;
pub mod http;
pub mod room_manager;
pub mod ws;

pub use app::{build_router, AppState};
pub use error::ApiResult;
