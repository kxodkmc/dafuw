//! WebSocket 集成测试：真实 TCP 建连验证鉴权、事件广播与多房间隔离。
//!
//! 约定：HTTP 请求复用同一 `AppState` 上的 Router（oneshot 直连），
//! WebSocket 走真实监听的 TCP 端口，保证全链路真实。

use std::net::SocketAddr;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use monopoly_common::command::Command;
use monopoly_common::event::Event;
use monopoly_server::app::{build_router, AppState};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tower::ServiceExt;

type WsStream =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct Server {
    /// WebSocket 基础地址（ws://127.0.0.1:port）。
    ws_base: String,
    /// 共享 AppState 的路由副本，用于 HTTP 请求。
    router: Router,
}

/// 启动真实监听服务器，返回 WS 地址与共享路由。
async fn spawn_server() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    // 同一 AppState：HTTP 与 WS 必须看到同一批房间。
    let server_router = build_router(AppState::new());
    let client_router = server_router.clone();
    tokio::spawn(async move {
        let _ = axum::serve(listener, server_router).await;
    });
    Server {
        ws_base: format!("ws://{addr}"),
        router: client_router,
    }
}

impl Server {
    async fn json(&self, method: &str, uri: &str, body: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::builder().uri(uri).method(method);
        if body.is_some() {
            req = req.header("content-type", "application/json");
        }
        let res = self
            .router
            .clone()
            .oneshot(
                req.body(Body::from(body.unwrap_or("").to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(Value::Null)
        };
        (status, json)
    }

    async fn create_room(&self, max_players: u8) -> (u32, String) {
        let body = format!(r#"{{"settings":{{"player_count_max":{max_players}}}}}"#);
        let (status, v) = self.json("POST", "/api/rooms", Some(&body)).await;
        assert_eq!(status, StatusCode::OK, "建房失败: {v}");
        (
            v["room_code"].as_u64().unwrap() as u32,
            v["owner_token"].as_str().unwrap().to_string(),
        )
    }

    /// 入座（空请求体，系统分配身份），返回 (玩家令牌, 系统分配的昵称)。
    async fn join_room(&self, code: u32) -> (String, String) {
        let (status, v) = self.json("POST", &format!("/api/rooms/{code}/join"), Some("{}")).await;
        assert_eq!(status, StatusCode::OK, "入座失败: {v}");
        (
            v["player_token"].as_str().unwrap().to_string(),
            v["name"].as_str().unwrap().to_string(),
        )
    }

    async fn ws(
        &self,
        code: u32,
        token: &str,
    ) -> Result<WsStream, tokio_tungstenite::tungstenite::Error> {
        let url = format!("{}/ws/rooms/{code}?token={token}", self.ws_base);
        let (socket, _) = tokio_tungstenite::connect_async(&url).await?;
        Ok(socket)
    }
}

/// 等待满足条件的广播事件（上限 64 条 / 每条 3s 超时）。
async fn expect_event_until(
    socket: &mut WsStream,
    pred: impl Fn(&Event) -> bool,
) -> Result<Event, String> {
    for _ in 0..64 {
        match tokio::time::timeout(Duration::from_secs(3), socket.next()).await {
            Ok(Some(Ok(WsMessage::Text(text)))) => {
                if let Ok(ev) = serde_json::from_str::<Event>(&text) {
                    if pred(&ev) {
                        return Ok(ev);
                    }
                }
            }
            Ok(Some(Ok(_))) => continue,
            other => return Err(format!("连接提前结束: {other:?}")),
        }
    }
    Err("超时未等到目标事件".into())
}

/// 在 socket 上直接发一条指令 JSON。
async fn send_cmd(socket: &mut WsStream, cmd: &Command) {
    socket
        .send(WsMessage::Text(serde_json::to_string(cmd).unwrap().into()))
        .await
        .unwrap();
}

#[tokio::test]
async fn ws_rejects_invalid_token() {
    let srv = spawn_server().await;
    let (code, _) = srv.create_room(2).await;
    assert!(
        srv.ws(code, "bogus_token").await.is_err(),
        "无效令牌必须拒绝连接"
    );
    assert!(srv.ws(code, "").await.is_err(), "空令牌必须拒绝连接");
}

#[tokio::test]
async fn ws_full_game_turn_flow() {
    let srv = spawn_server().await;
    let (code, owner) = srv.create_room(2).await;
    let (token_a, name_a) = srv.join_room(code).await;
    let (token_b, name_b) = srv.join_room(code).await;

    let mut ws_a = srv.ws(code, &token_a).await.expect("Alice 连接失败");
    expect_event_until(&mut ws_a, |e| matches!(e, Event::RoomSnapshot(_)))
        .await
        .expect("连接即收到快照");

    let mut ws_b = srv.ws(code, &token_b).await.expect("Bob 连接失败");
    expect_event_until(&mut ws_b, |e| matches!(e, Event::RoomSnapshot(_)))
        .await
        .expect("连接即收到快照");

    // 双方准备就绪。
    send_cmd(&mut ws_a, &Command::SetReady { ready: true }).await;
    send_cmd(&mut ws_b, &Command::SetReady { ready: true }).await;
    expect_event_until(&mut ws_a, |e| matches!(e, Event::PlayerReady { ready: true, .. }))
        .await
        .expect("应广播就绪事件");

    // 房主通过 WS 开局（全员就绪后成功）。
    let mut ws_owner = srv.ws(code, &owner).await.expect("房主连接失败");
    send_cmd(&mut ws_owner, &Command::StartGame).await;

    // 双端应收到开局与回合事件（broadcast）。
    expect_event_until(&mut ws_a, |e| matches!(e, Event::GameStarted { .. }))
        .await
        .unwrap();
    expect_event_until(&mut ws_a, |e| matches!(e, Event::TurnStarted { .. }))
        .await
        .unwrap();

    // 从快照解析当前回合玩家，并确认其身份令牌。
    let snap = expect_event_until(
        &mut ws_a,
        |e| matches!(e, Event::RoomSnapshot(s) if s.status == "playing"),
    )
    .await
    .unwrap();
    let Event::RoomSnapshot(snap) = snap else {
        unreachable!()
    };
    let current = snap.current_player.expect("对局中应存在当前玩家");
    let current_name = snap
        .players
        .iter()
        .find(|p| p.id == current)
        .map(|p| p.name.as_str())
        .unwrap_or("");
    let cur_token = if current_name == name_a {
        token_a.clone()
    } else {
        assert_eq!(current_name, name_b);
        token_b.clone()
    };

    // 当前玩家建连并发起掷骰。
    let mut ws_cur = srv.ws(code, &cur_token).await.expect("当前玩家连接失败");
    send_cmd(&mut ws_cur, &Command::RollDice).await;

    // 其他玩家应收到 DiceRolled 广播与随后的 playing 快照。
    expect_event_until(&mut ws_a, |e| matches!(e, Event::DiceRolled { .. }))
        .await
        .expect("另一玩家应收到 DiceRolled");
    expect_event_until(
        &mut ws_a,
        |e| matches!(e, Event::RoomSnapshot(s) if s.status == "playing"),
    )
    .await
    .expect("广播后应追加 fresh 快照");

    // 房主不能代替玩家操作。
    send_cmd(&mut ws_owner, &Command::RollDice).await;
    expect_event_until(
        &mut ws_a,
        |e| matches!(e, Event::Error { code, .. } if code == "bad_request"),
    )
    .await
    .expect("房主代投应被拒绝并广播错误");
    let _ = ws_b;
    let _ = ws_owner;
    let _ = current_name;
}

#[tokio::test]
async fn ws_rooms_are_isolated() {
    let srv = spawn_server().await;
    let (code1, _) = srv.create_room(2).await;
    let (code2, _) = srv.create_room(2).await;
    let (tok1, _) = srv.join_room(code1).await;
    let (tok2, _) = srv.join_room(code2).await;

    let mut ws1 = srv.ws(code1, &tok1).await.expect("房 1 连接失败");
    let _ = expect_event_until(&mut ws1, |e| matches!(e, Event::RoomSnapshot(_))).await;

    let mut ws2 = srv.ws(code2, &tok2).await.expect("房 2 连接失败");
    let _ = expect_event_until(&mut ws2, |e| matches!(e, Event::RoomSnapshot(_))).await;

    // 房 1 的令牌不能用于房 2。
    assert!(srv.ws(code2, &tok1).await.is_err(), "跨房令牌必须失效");

    // 向房 1 发指令：房 2 不应收到任何事件。
    send_cmd(&mut ws1, &Command::RollDice).await;
    let leaked = tokio::time::timeout(Duration::from_millis(300), ws2.next())
        .await
        .ok()
        .flatten();
    assert!(leaked.is_none(), "房 2 不应收到房 1 的事件: {leaked:?}");
}

#[tokio::test]
async fn ws_disconnected_player_is_autopiloted() {
    let srv = spawn_server().await;
    let (code, owner) = srv.create_room(2).await;
    let (token_a, name_a) = srv.join_room(code).await;
    let (token_b, name_b) = srv.join_room(code).await;

    // 双方先建连并确认各自就绪事件（保证就绪已生效）。
    let mut ws_b = srv.ws(code, &token_b).await.expect("Bob 连接失败");
    let _ = expect_event_until(&mut ws_b, |e| matches!(e, Event::RoomSnapshot(_))).await;
    send_cmd(&mut ws_b, &Command::SetReady { ready: true }).await;
    expect_event_until(&mut ws_b, |e| matches!(e, Event::PlayerReady { ready: true, .. }))
        .await
        .expect("Bob 就绪应广播");

    let mut ws_a = srv.ws(code, &token_a).await.expect("Alice 连接失败");
    let _ = expect_event_until(&mut ws_a, |e| matches!(e, Event::RoomSnapshot(_))).await;
    send_cmd(&mut ws_a, &Command::SetReady { ready: true }).await;
    expect_event_until(&mut ws_a, |e| matches!(e, Event::PlayerReady { ready: true, .. }))
        .await
        .expect("Alice 就绪应广播");

    // 此时 Alice 已订阅广播；Bob 断开 → 广播 PlayerLeft，Alice 必然收到。
    drop(ws_b);
    expect_event_until(&mut ws_a, |e| matches!(e, Event::PlayerLeft { .. }))
        .await
        .expect("应广播 Bob 离开事件");

    // 房主开局：Alice 在线、Bob 离线（全员曾就绪）。
    let mut ws_owner = srv.ws(code, &owner).await.expect("房主连接失败");
    send_cmd(&mut ws_owner, &Command::StartGame).await;

    // 对局进入 playing；无论起始玩家是谁，最终会轮到 Alice（Bob 的回合被自动托管）。
    let snap = expect_event_until(
        &mut ws_a,
        |e| matches!(e, Event::RoomSnapshot(s) if s.status == "playing"),
    )
    .await
    .unwrap();
    let Event::RoomSnapshot(snap) = snap else {
        unreachable!()
    };
    assert!(
        snap.players.iter().all(|p| !p.bankrupt) || snap.winner.is_some(),
        "对局不应因 Bob 离线而异常"
    );

    // Alice 掷骰并驱动其回合（买地/放弃/拍卖弃权），直到观察到 Bob 被托管自动移动。
    let mut saw_bob_moved = false;
    send_cmd(&mut ws_a, &Command::RollDice).await;
    for _ in 0..12 {
        let snap = expect_event_until(&mut ws_a, |e| matches!(e, Event::RoomSnapshot(_)))
            .await
            .unwrap();
        let Event::RoomSnapshot(snap) = snap else {
            unreachable!()
        };
        let Some(alice) = snap.players.iter().find(|p| p.name == name_a) else {
            break;
        };
        let Some(bob) = snap.players.iter().find(|p| p.name == name_b) else {
            break;
        };
        if bob.position > 0 || bob.cash < 15_000_000 {
            saw_bob_moved = true;
            break;
        }
        // 轮到 Alice 掷骰/买地/竞价 → 继续驱动；轮到 Bob 时由托管自动处理（快照自动推进）。
        if snap.current_player == Some(alice.id) {
            let cmd = match snap.phase.as_str() {
                "decision" => Command::BuyProperty,
                "await_roll" => Command::RollDice,
                "auction" => Command::AuctionPass,
                _ => break,
            };
            send_cmd(&mut ws_a, &cmd).await;
        }
    }
    assert!(saw_bob_moved, "Bob 离线后应被托管自动移动");
    let _ = ws_owner;
}
