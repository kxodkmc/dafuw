//! WebSocket 网关：转发玩家指令到房间 Actor，并把事件流推回客户端。

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::oneshot;

use monopoly_common::command::Command;
use monopoly_common::event::Event;
use monopoly_common::room::RoomCode;
use monopoly_common::snapshot::GameStateDto;

use crate::actor::{ActorMsg, Session};
use crate::app::AppState;
use crate::error::ApiError;
use crate::room_manager::RoomHandle;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    pub token: String,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Path(code): Path<RoomCode>,
    Query(q): Query<WsQuery>,
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let handle = state
        .manager
        .get(code)
        .ok_or_else(|| ApiError::not_found("房间"))?;
    let session = handle
        .resolve_token(&q.token)
        .ok_or_else(|| ApiError::forbidden("无效的会话令牌"))?;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, handle, session)))
}

async fn handle_socket(socket: WebSocket, handle: Arc<RoomHandle>, session: Session) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = handle.broadcast.subscribe();

    let _ = handle
        .sender
        .send(ActorMsg::Connected {
            session: session.clone(),
        })
        .await;

    // 连接即推送一份完整快照。
    if let Ok(snap) = query_snapshot(&handle).await {
        if let Ok(text) = serde_json::to_string(&Event::RoomSnapshot(snap)) {
            if sink.send(Message::Text(text.into())).await.is_err() {
                return;
            }
        }
    }

    // 写端：广播 → 客户端。
    let writer = tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let Ok(text) = serde_json::to_string(&ev) {
                if sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    // 读端：客户端指令 → Actor。
    while let Some(Ok(msg)) = stream.next().await {
        if let Message::Text(text) = msg {
            if let Ok(cmd) = serde_json::from_str::<Command>(&text) {
                let _ = handle
                    .sender
                    .send(ActorMsg::Command {
                        session: session.clone(),
                        cmd,
                    })
                    .await;
            }
        }
    }

    let _ = handle.sender.send(ActorMsg::Disconnected { session }).await;
    writer.abort();
}

async fn query_snapshot(handle: &Arc<RoomHandle>) -> Result<GameStateDto, ()> {
    let (tx, rx) = oneshot::channel();
    handle
        .sender
        .send(ActorMsg::Query { reply: tx })
        .await
        .map_err(|_| ())?;
    rx.await.map_err(|_| ())
}
