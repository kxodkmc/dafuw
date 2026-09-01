//! 路由装配与共享状态。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use axum::routing::{get, post};
use axum::Router;
use uuid::Uuid;

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
    /// 全局玩家身份注册表：player_uid → secret（内存表，重启后先到先得重新登记）。
    pub identities: Arc<RwLock<HashMap<Uuid, Uuid>>>,
    /// 进程启动时刻，用于就绪探针的存活时长。
    pub started_at: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            manager: Arc::new(RoomManager::new()),
            identities: Arc::new(RwLock::new(HashMap::new())),
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
            ..Self::default()
        }
    }
}

/// 组装路由。
///
/// CORS 放开：桌面客户端（file:// 来源）与局域网网页均需跨域访问本服务。
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/rooms", get(list_rooms).post(create_room))
        .route("/api/rooms/{code}/join", post(join_room))
        .route("/api/rooms/{code}", get(get_room).delete(delete_room))
        .route("/api/rooms/{code}/start", post(start_game))
        .route("/ws/rooms/{code}", get(ws_handler))
        .layer(tower_http::cors::CorsLayer::permissive())
        .with_state(state)
}
