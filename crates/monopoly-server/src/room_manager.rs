use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use monopoly_common::command::Command;
use monopoly_common::event::Event;
use monopoly_common::room::{RoomCode, RoomSettings};
use monopoly_common::snapshot::GameStateDto;
use monopoly_core::engine::GameEngine;
use monopoly_persistence::GameRepository;

use crate::actor::{spawn_actor, ActorMsg, Session};
use crate::error::ApiError;

/// 一个房间的句柄：向 Actor 发指令、读配置、校验令牌。
#[derive(Debug)]
pub struct RoomHandle {
    pub code: RoomCode,
    /// 房间配置（含名称/人数上限，供列表与恢复展示）。
    pub settings: RoomSettings,
    pub owner_token: String,
    pub sender: mpsc::Sender<ActorMsg>,
    pub player_tokens: Arc<RwLock<HashMap<String, Uuid>>>,
    pub broadcast: broadcast::Sender<Event>,
}

/// 房间注册表：`8 位房号 → 房间句柄`，负责创建、查找与销毁。
#[derive(Default)]
pub struct RoomManager {
    rooms: RwLock<HashMap<RoomCode, Arc<RoomHandle>>>,
    /// 持久化仓库；`None` 表示不落盘（测试/演示）。
    repo: Option<Arc<dyn GameRepository>>,
}

impl RoomManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 绑定持久化仓库（启用自动保存与启动恢复）。
    pub fn with_repo(repo: Arc<dyn GameRepository>) -> Self {
        Self {
            repo: Some(repo),
            ..Self::default()
        }
    }

    /// 读取房间表；锁中毒时沿用已被污染的状态，避免 panic 拖垮服务器。
    fn rooms_read(&self) -> std::sync::RwLockReadGuard<'_, HashMap<RoomCode, Arc<RoomHandle>>> {
        self.rooms.read().unwrap_or_else(|e| e.into_inner())
    }

    fn rooms_write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<RoomCode, Arc<RoomHandle>>> {
        self.rooms.write().unwrap_or_else(|e| e.into_inner())
    }

    pub fn get(&self, code: RoomCode) -> Option<Arc<RoomHandle>> {
        self.rooms_read().get(&code).cloned()
    }

    /// 当前全部房间句柄（局域网发现/列表用）。
    pub fn all(&self) -> Vec<Arc<RoomHandle>> {
        self.rooms_read().values().cloned().collect()
    }

    /// 当前房间总数（用于健康检查等观测）。
    pub fn room_count(&self) -> usize {
        self.rooms_read().len()
    }

    /// 创建房间，返回 `(房号, 房主令牌)`。
    pub fn create(&self, settings: RoomSettings) -> Result<(RoomCode, String), ApiError> {
        settings.validate().map_err(ApiError::bad_request)?;
        let code = self.generate_code();
        let owner_token = Uuid::new_v4().to_string();
        let (sender, rx) = mpsc::channel::<ActorMsg>(64);
        let (broadcast, _) = broadcast::channel::<Event>(256);
        let player_tokens = Arc::new(RwLock::new(HashMap::new()));
        let engine = GameEngine::new(settings.clone()).map_err(ApiError::internal)?;

        self.register_room(
            code,
            settings.clone(),
            owner_token.clone(),
            player_tokens.clone(),
            sender.clone(),
            broadcast.clone(),
        );
        spawn_actor(
            code,
            owner_token.clone(),
            engine,
            rx,
            broadcast,
            player_tokens,
            self.repo.clone(),
        );
        Ok((code, owner_token))
    }

    /// 服务器启动时从仓库恢复所有未结束的对局，重新注册房间并拉起 Actor。
    ///
    /// 返回成功恢复的房间数；单局恢复失败会整体中止（启动即失败更安全，避免半恢复状态）。
    pub fn restore_all(&self) -> Result<usize, String> {
        let Some(repo) = &self.repo else {
            return Ok(0);
        };
        let games = repo.list_active_games()?;
        let mut restored = 0;
        for game in games {
            if self.rooms_read().contains_key(&game.code) {
                continue;
            }
            let engine = GameEngine::restore(&game.archive)
                .map_err(|e| format!("恢复房间 {} 失败: {e}", game.code))?;
            let (sender, rx) = mpsc::channel::<ActorMsg>(64);
            let (broadcast, _) = broadcast::channel::<Event>(256);
            let tokens: HashMap<String, Uuid> = game.player_tokens.iter().cloned().collect();
            let player_tokens = Arc::new(RwLock::new(tokens));

            self.register_room(
                game.code,
                game.archive.settings.clone(),
                game.owner_token.clone(),
                player_tokens.clone(),
                sender.clone(),
                broadcast.clone(),
            );
            spawn_actor(
                game.code,
                game.owner_token.clone(),
                engine,
                rx,
                broadcast,
                player_tokens,
                self.repo.clone(),
            );
            restored += 1;
        }
        Ok(restored)
    }

    /// 登记房间句柄并加入注册表。
    fn register_room(
        &self,
        code: RoomCode,
        settings: RoomSettings,
        owner_token: String,
        player_tokens: Arc<RwLock<HashMap<String, Uuid>>>,
        sender: mpsc::Sender<ActorMsg>,
        broadcast: broadcast::Sender<Event>,
    ) {
        let handle = Arc::new(RoomHandle {
            code,
            settings,
            owner_token,
            sender,
            player_tokens,
            broadcast,
        });
        self.rooms_write().insert(code, handle);
    }

    /// 生成不重复的 8 位数字房号。
    fn generate_code(&self) -> RoomCode {
        let mut rng = rand::thread_rng();
        loop {
            let n = rand::Rng::gen_range(&mut rng, RoomCode::MIN..=RoomCode::MAX);
            if let Ok(code) = RoomCode::new(n) {
                if !self.rooms_read().contains_key(&code) {
                    return code;
                }
            }
        }
    }

    /// 解散房间并通知 Actor 退出。
    pub fn remove(&self, code: RoomCode) -> Option<Arc<RoomHandle>> {
        let handle = self.rooms_write().remove(&code);
        if let Some(h) = &handle {
            let _ = h.sender.try_send(ActorMsg::Shutdown);
            let _ = h.broadcast.send(Event::RoomClosed);
        }
        handle
    }
}

impl RoomHandle {
    /// 校验会话令牌：房主令牌 → `Session::Owner`，玩家令牌 → `Session::Player`。
    pub fn resolve_token(&self, token: &str) -> Option<Session> {
        if token == self.owner_token {
            return Some(Session::Owner);
        }
        let tokens = self.player_tokens.read().unwrap_or_else(|e| e.into_inner());
        tokens.get(token).copied().map(Session::Player)
    }

    /// 请求入座（走 Actor，保证串行加入）。
    pub async fn request_join(&self, name: &str) -> Result<(Uuid, String), ApiError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ActorMsg::Join {
                name: name.to_string(),
                reply: tx,
            })
            .await
            .map_err(|_| ApiError::internal("房间已关闭"))?;
        rx.await
            .map_err(|_| ApiError::internal("房间已关闭"))?
            .map_err(ApiError::bad_request)
    }

    /// 拉取当前快照。
    pub async fn query_snapshot(&self) -> Result<GameStateDto, ApiError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ActorMsg::Query { reply: tx })
            .await
            .map_err(|_| ApiError::internal("房间已关闭"))?;
        rx.await.map_err(|_| ApiError::internal("房间已关闭"))
    }

    /// 发起开局。
    pub async fn start(&self) -> Result<(), ApiError> {
        let (tx, rx) = oneshot::channel();
        self.sender
            .send(ActorMsg::Start { reply: tx })
            .await
            .map_err(|_| ApiError::internal("房间已关闭"))?;
        rx.await
            .map_err(|_| ApiError::internal("房间已关闭"))?
            .map_err(ApiError::bad_request)
    }

    /// 提交一条玩家指令（WS 网关与测试复用）。
    pub async fn command(&self, session: Session, cmd: Command) -> Result<(), ApiError> {
        self.sender
            .send(ActorMsg::Command { session, cmd })
            .await
            .map_err(|_| ApiError::internal("房间已关闭"))
    }
}
