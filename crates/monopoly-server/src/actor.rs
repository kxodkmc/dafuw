//! 房间 Actor：每房间一个 tokio 任务，独占引擎、串行消费指令并广播事件。
//!
//! 断线托管：玩家无活跃 WS 连接时进入托管，服务器自动「掷骰/移动」；
//! 托管不买地（放弃购买）、不参与拍卖出价；玩家重连后立即恢复人工操作。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use monopoly_common::command::Command;
use monopoly_common::event::Event;
use monopoly_common::room::RoomSettings;
use monopoly_common::snapshot::GameStateDto;
use monopoly_core::engine::{GameEngine, Phase};
use monopoly_core::rng::StdRngSource;

/// 会话身份：玩家（持有玩家令牌）或房主（持有房主令牌）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    Player(Uuid),
    Owner,
}

/// 发给房间 Actor 的消息。
pub enum ActorMsg {
    Command {
        session: Session,
        cmd: Command,
    },
    Join {
        name: String,
        reply: oneshot::Sender<Result<(Uuid, String), String>>,
    },
    Start {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Query {
        reply: oneshot::Sender<GameStateDto>,
    },
    Connected {
        session: Session,
    },
    Disconnected {
        session: Session,
    },
    Shutdown,
}

/// 启动房间 Actor 任务。
pub fn spawn_actor(
    settings: RoomSettings,
    rx: mpsc::Receiver<ActorMsg>,
    tx: broadcast::Sender<Event>,
    player_tokens: Arc<RwLock<HashMap<String, Uuid>>>,
) {
    tokio::spawn(async move {
        let mut actor = RoomActor::new(settings, tx, player_tokens);
        actor.run(rx).await;
    });
}

struct RoomActor {
    engine: GameEngine,
    tx: broadcast::Sender<Event>,
    player_tokens: Arc<RwLock<HashMap<String, Uuid>>>,
    /// 玩家当前活跃连接数；无连接即视为「托管」。
    connected: HashMap<Uuid, usize>,
    /// 房间内持久随机源，保证对局可复现、可测试。
    rng: StdRngSource,
}

impl RoomActor {
    fn new(
        settings: RoomSettings,
        tx: broadcast::Sender<Event>,
        player_tokens: Arc<RwLock<HashMap<String, Uuid>>>,
    ) -> Self {
        Self {
            engine: GameEngine::new(settings).expect("房间配置必须合法"),
            tx,
            player_tokens,
            connected: HashMap::new(),
            rng: StdRngSource::from_os_rng(),
        }
    }

    async fn run(&mut self, mut rx: mpsc::Receiver<ActorMsg>) {
        while let Some(msg) = rx.recv().await {
            match msg {
                ActorMsg::Command { session, cmd } => self.command(session, cmd),
                ActorMsg::Join { name, reply } => {
                    let _ = reply.send(self.join(name));
                }
                ActorMsg::Start { reply } => {
                    let _ = reply.send(self.start_game());
                }
                ActorMsg::Query { reply } => {
                    let _ = reply.send(self.engine.snapshot());
                }
                ActorMsg::Connected { session } => {
                    self.on_connected(session);
                    // 重连后若轮到断线 bot，继续托管推进到该玩家。
                    self.auto_drive();
                }
                ActorMsg::Disconnected { session } => {
                    self.on_disconnected(session);
                    // 若断线者正轮到其行动，立即接管为托管，避免对局卡住。
                    self.auto_drive();
                }
                ActorMsg::Shutdown => break,
            }
        }
    }

    // ---------- 连接与托管 ----------

    fn on_connected(&mut self, session: Session) {
        if let Session::Player(id) = session {
            *self.connected.entry(id).or_insert(0) += 1;
        }
    }

    fn on_disconnected(&mut self, session: Session) {
        if let Session::Player(id) = session {
            if let Some(count) = self.connected.get_mut(&id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.connected.remove(&id);
                }
            }
        }
    }

    /// 玩家是否处于托管（无活跃连接）。
    fn is_bot(&self, id: Uuid) -> bool {
        !self.connected.contains_key(&id)
    }

    /// 当前需要玩家行动的玩家：拍卖中取当前出价者，否则取回合玩家（测试辅助）。
    #[cfg(test)]
    fn pending_player(&self) -> Option<Uuid> {
        if self.engine.phase() == Phase::InAuction {
            self.engine.auction_current_bidder()
        } else {
            self.engine.current_player_id()
        }
    }

    /// 驱动断线托管：把所有需要行动的 bot 依次自动处理，直到轮到人类或对局结束。
    ///
    /// 若房间内没有任何玩家在线则暂停（托管不买地 → 经济停摆 → 无限对局），
    /// 等有人重连后继续。
    fn auto_drive(&mut self) {
        if self.connected.is_empty() {
            return;
        }
        loop {
            // 拍卖中：轮到哪个 bot 就替它弃权（托管不出价）。
            if self.engine.phase() == Phase::InAuction {
                match self.engine.auction_current_bidder() {
                    Some(bidder) if self.is_bot(bidder) => {
                        if !self.bot_act(bidder, Command::AuctionPass) {
                            break;
                        }
                        continue;
                    }
                    _ => break, // 等待人类出价
                }
            }
            let Some(cur) = self.engine.current_player_id() else {
                break;
            };
            if !self.is_bot(cur) {
                break;
            }
            let cmd = match self.engine.phase() {
                Phase::AwaitRoll => Command::RollDice,
                // 托管仅移动、不买地：放弃购买 → 银行拍卖。
                Phase::AwaitDecision => Command::PassProperty,
                _ => break,
            };
            if !self.bot_act(cur, cmd) {
                break;
            }
        }
    }

    /// 执行一次托管动作；成功返回 `true`，出错返回 `false`（防止死循环）。
    fn bot_act(&mut self, pid: Uuid, cmd: Command) -> bool {
        let name = self
            .engine
            .player(pid)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        match self.engine.handle(pid, cmd, &mut self.rng) {
            Ok(events) => {
                let _ = self.tx.send(Event::Message {
                    text: format!("「{name}」托管中，自动行动"),
                });
                self.broadcast_events_and_snapshot(&events);
                true
            }
            Err(_) => false,
        }
    }

    // ---------- 生命周期 ----------

    /// 玩家入座：加入引擎大厅，签发玩家令牌。
    fn join(&mut self, name: String) -> Result<(Uuid, String), String> {
        let id = Uuid::new_v4();
        self.engine.add_player(id, name.clone())?;
        let token = Uuid::new_v4().to_string();
        self.player_tokens
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token.clone(), id);
        self.broadcast(vec![
            Event::PlayerJoined {
                player_id: id,
                name,
            },
            Event::RoomSnapshot(self.engine.snapshot()),
        ]);
        Ok((id, token))
    }

    fn start_game(&mut self) -> Result<(), String> {
        let events = self.engine.start(&mut self.rng)?;
        self.broadcast_events_and_snapshot(&events);
        self.auto_drive();
        Ok(())
    }

    fn command(&mut self, session: Session, cmd: Command) {
        match session {
            Session::Owner => match cmd {
                Command::StartGame => {
                    if let Err(e) = self.start_game() {
                        self.send_error(e);
                    }
                }
                other => self.send_error(format!("房主不能代替玩家操作：{other:?}")),
            },
            Session::Player(pid) => {
                match self.engine.handle(pid, cmd, &mut self.rng) {
                    Ok(events) => {
                        self.broadcast_events_and_snapshot(&events);
                        // 人类行动后可能轮到断线玩家 → 继续托管推进。
                        self.auto_drive();
                    }
                    Err(e) => self.send_error(e),
                }
            }
        }
    }

    // ---------- 广播 ----------

    /// 广播事件流，并始终附上一份最新快照，保证客户端状态一致。
    fn broadcast_events_and_snapshot(&self, events: &[Event]) {
        for e in events {
            let _ = self.tx.send(e.clone());
        }
        let _ = self.tx.send(Event::RoomSnapshot(self.engine.snapshot()));
    }

    fn broadcast(&self, events: Vec<Event>) {
        for e in events {
            let _ = self.tx.send(e);
        }
    }

    fn send_error(&self, message: String) {
        let _ = self.tx.send(Event::Error {
            code: "bad_request".to_string(),
            message,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn seeded_actor(max_rounds: Option<u32>) -> (RoomActor, tokio::sync::broadcast::Sender<Event>) {
        let settings = RoomSettings {
            player_count_max: 2,
            max_rounds,
            ..Default::default()
        };
        let (tx, _rx) = broadcast::channel::<Event>(1024);
        let tokens = Arc::new(RwLock::new(HashMap::new()));
        let actor = RoomActor {
            engine: GameEngine::new(settings).expect("配置必须合法"),
            tx: tx.clone(),
            player_tokens: tokens,
            connected: HashMap::new(),
            rng: StdRngSource(StdRng::seed_from_u64(7)),
        };
        (actor, tx)
    }

    #[test]
    fn all_disconnected_game_pauses_without_crash() {
        let (mut actor, _tx) = seeded_actor(None);
        let (a, _) = actor.join("Alice".to_string()).unwrap();
        let (b, _) = actor.join("Bob".to_string()).unwrap();
        assert!(actor.is_bot(a));
        assert!(actor.is_bot(b));
        // 全部断线 → 托管暂停（不买地会导致经济停摆、对局无限，故不自动自走）。
        actor.start_game().unwrap();
        assert!(!actor.engine.is_over());
        assert_eq!(actor.engine.phase(), Phase::AwaitRoll);
    }

    #[test]
    fn bot_turns_autopiloted_and_reconnect_resumes() {
        let (mut actor, _tx) = seeded_actor(None);
        let (a, _) = actor.join("Alice".to_string()).unwrap();
        let _ = actor.join("Bob".to_string()).unwrap();
        actor.on_connected(Session::Player(a));

        // 有 Alice 在线 → 轮到 Bob（bot）时自动托管推进，最终停在需要 Alice 的地方。
        actor.start_game().unwrap();
        assert!(!actor.engine.is_over());
        let waiting = actor.pending_player().expect("应等待某位玩家行动");
        assert_eq!(waiting, a);

        // Alice 也断线 → 全部离线 → 暂停，不崩溃。
        actor.on_disconnected(Session::Player(a));
        assert!(!actor.engine.is_over());

        // Alice 重连 → 继续托管推进，仍停在需要 Alice 的地方。
        actor.on_connected(Session::Player(a));
        let waiting = actor.pending_player().expect("应等待某位玩家行动");
        assert_eq!(waiting, a);
        assert!(!actor.engine.is_over());
    }
}
