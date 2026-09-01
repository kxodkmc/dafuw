//! 路由装配与共享状态。

use std::sync::Arc;
use std::time::Instant;

use axum::routing::{get, post};
use axum::Router;

use monopoly_persistence::GameRepository;

use crate::http::{
    create_room, delete_room, get_room, health, join_room, list_rooms, start_game,
};
use crate::room_manager::RoomManager;
use crate::ws::ws_handler;

/// 全局共享状态。
#[derive(Clone)]
pub struct AppState {
    pub manager: Arc<RoomManager>,
    /// 进程启动时刻，用于就绪探针的存活时长。
    pub started_at: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            manager: Arc::new(RoomManager::new()),
            started_at: Instant::now(),
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 带持久化仓库的状态：启用自动保存，可在启动时恢复进行中的对局。
    pub fn with_repository(repo: Arc<dyn GameRepository>) -> Self {
        Self {
            manager: Arc::new(RoomManager::with_repo(repo)),
            started_at: Instant::now(),
        }
    }
}

/// 组装路由。
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/rooms", get(list_rooms).post(create_room))
        .route("/api/rooms/{code}/join", post(join_room))
        .route("/api/rooms/{code}", get(get_room).delete(delete_room))
        .route("/api/rooms/{code}/start", post(start_game))
        .route("/ws/rooms/{code}", get(ws_handler))
        .with_state(state)
}
