//! REST 处理器：房间生命周期管理。

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use monopoly_common::avatar::{Avatar, Color};
use monopoly_common::error::ApiError as CommonError;
use monopoly_common::event::Event;
use monopoly_common::room::{RoomCode, RoomSettings, RoomSummary};
use monopoly_common::snapshot::GameStateDto;

use crate::actor::{JoinResult, PlayerIdentity};
use crate::app::AppState;
use crate::error::{ApiError, ApiResult};

#[derive(Debug, Deserialize)]
pub struct CreateRoomReq {
    #[serde(default)]
    pub settings: Option<RoomSettings>,
    /// 房间名称（可选，最多 30 字符）。
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CreateRoomRes {
    pub room_code: u32,
    pub owner_token: String,
}

#[derive(Debug, Deserialize)]
pub struct JoinReq {
    /// 全局玩家身份（可选；不提供则服务器代生成并随响应返回）。
    #[serde(default)]
    pub player_uid: Option<Uuid>,
    #[serde(default)]
    pub secret: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct JoinRes {
    pub player_id: Uuid,
    pub player_token: String,
    /// 本请求生效的全局身份（与请求一致，或服务器代生成）。
    pub player_uid: Uuid,
    pub secret: Uuid,
    /// 系统随机分配的昵称。
    pub name: String,
    /// 系统随机分配的形象。
    pub avatar: Avatar,
    /// 系统随机分配的颜色。
    pub color: Color,
}

#[derive(Debug, Deserialize)]
pub struct OwnerReq {
    pub owner_token: String,
}

#[derive(Debug, Serialize)]
pub struct StartRes {
    pub ok: bool,
}

/// 就绪检查项。
#[derive(Debug, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub ok: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthRes {
    /// ready / not_ready。
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub uptime_secs: u64,
    pub rooms: usize,
    pub checks: Vec<Check>,
}

/// 创建房间：本地/云端同入口，返回 8 位房号与房主令牌。
pub async fn create_room(
    State(state): State<AppState>,
    Json(req): Json<CreateRoomReq>,
) -> ApiResult<Json<CreateRoomRes>> {
    let mut settings = req.settings.unwrap_or_default();
    if let Some(name) = req.name {
        settings.name = name.trim().to_string();
    }
    settings.validate().map_err(CommonError::BadRequest)?;
    let (room_code, owner_token) = state.manager.create(settings)?;
    Ok(Json(CreateRoomRes {
        room_code: room_code.as_u32(),
        owner_token,
    }))
}

/// 列出所有房间概要（局域网发现用，无需鉴权）。
pub async fn list_rooms(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<RoomSummary>>> {
    let mut out = Vec::new();
    for handle in state.manager.all() {
        let snap = handle.query_snapshot().await?;
        out.push(RoomSummary {
            code: handle.code.as_u32(),
            name: handle.settings.name.clone(),
            status: snap.status,
            players: snap.players.len(),
            player_count_max: handle.settings.player_count_max,
        });
    }
    Ok(Json(out))
}

/// 解析/登记全局玩家身份：已有映射严格校验 secret；未登记的先到先得。
fn resolve_identity(
    state: &AppState,
    player_uid: Option<Uuid>,
    secret: Option<Uuid>,
) -> Result<PlayerIdentity, ApiError> {
    match (player_uid, secret) {
        (Some(uid), Some(sec)) => {
            let mut map = state
                .identities
                .write()
                .unwrap_or_else(|e| e.into_inner());
            match map.get(&uid) {
                Some(registered) if *registered != sec => {
                    Err(ApiError::forbidden("身份码校验失败"))
                }
                Some(_) => Ok(PlayerIdentity { player_uid: uid, secret: sec }),
                None => {
                    map.insert(uid, sec);
                    Ok(PlayerIdentity { player_uid: uid, secret: sec })
                }
            }
        }
        // 未提供身份：服务器代生成并登记
        _ => {
            let uid = Uuid::new_v4();
            let sec = Uuid::new_v4();
            state
                .identities
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .insert(uid, sec);
            Ok(PlayerIdentity { player_uid: uid, secret: sec })
        }
    }
}

/// 加入房间（入座）：系统随机分配昵称/形象/颜色，返回玩家 ID、令牌与分配结果。
///
/// 携带全局身份（player_uid + secret）时：同 uid 再次加入任意房间即认领原座位，
/// 跨天/重进/换设备均可恢复原角色。
pub async fn join_room(
    State(state): State<AppState>,
    Path(code): Path<RoomCode>,
    Json(req): Json<JoinReq>,
) -> ApiResult<Json<JoinRes>> {
    let handle = state
        .manager
        .get(code)
        .ok_or_else(|| ApiError::not_found("房间"))?;
    let identity = resolve_identity(&state, req.player_uid, req.secret)?;
    let JoinResult {
        player_id,
        token,
        name,
        avatar,
        color,
        secret,
    } = handle.request_join(identity).await?;
    Ok(Json(JoinRes {
        player_id,
        player_token: token,
        player_uid: identity.player_uid,
        secret,
        name,
        avatar,
        color,
    }))
}

/// 拉取房间快照（用于断线重连/观战）。
pub async fn get_room(
    State(state): State<AppState>,
    Path(code): Path<RoomCode>,
) -> ApiResult<Json<GameStateDto>> {
    let handle = state
        .manager
        .get(code)
        .ok_or_else(|| ApiError::not_found("房间"))?;
    let snap = handle.query_snapshot().await?;
    Ok(Json(snap))
}

/// 房主开局。
pub async fn start_game(
    State(state): State<AppState>,
    Path(code): Path<RoomCode>,
    Json(req): Json<OwnerReq>,
) -> ApiResult<Json<StartRes>> {
    let handle = state
        .manager
        .get(code)
        .ok_or_else(|| ApiError::not_found("房间"))?;
    if req.owner_token != handle.owner_token {
        return Err(ApiError::forbidden("仅房主可以开局"));
    }
    handle.start().await?;
    Ok(Json(StartRes { ok: true }))
}

/// 房主解散房间。
pub async fn delete_room(
    State(state): State<AppState>,
    Path(code): Path<RoomCode>,
    Json(req): Json<OwnerReq>,
) -> ApiResult<StatusCode> {
    let handle = state
        .manager
        .get(code)
        .ok_or_else(|| ApiError::not_found("房间"))?;
    if req.owner_token != handle.owner_token {
        return Err(ApiError::forbidden("仅房主可以解散房间"));
    }
    let _ = handle.broadcast.send(Event::RoomClosed);
    state.manager.remove(code);
    Ok(StatusCode::NO_CONTENT)
}

/// 就绪探针：一切依赖就绪时返回 200，否则返回 503。
/// 供负载均衡 / nginx / 编排系统探测；后续接入 SQLite 后在 `readiness_checks` 中追加数据库检查。
pub async fn health(State(state): State<AppState>) -> Response {
    let checks = readiness_checks();
    let ready = checks.iter().all(|c| c.ok);
    let body = HealthRes {
        status: if ready { "ready" } else { "not_ready" },
        service: "monopoly-server",
        version: env!("CARGO_PKG_VERSION"),
        uptime_secs: state.started_at.elapsed().as_secs(),
        rooms: state.manager.room_count(),
        checks,
    };
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(body)).into_response()
}

/// 外部依赖就绪检查；当前无外部依赖，SQLite 接入后在此追加 `database` 检查。
fn readiness_checks() -> Vec<Check> {
    vec![]
}
