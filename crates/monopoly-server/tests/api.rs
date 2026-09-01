//! 服务器集成测试：通过 Axum Router 直接发请求验证 REST 流程。
//! 覆盖：健康检查 / 建房 / 入座校验 / 满员 / 权限 / 房间隔离 / 解散。

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use monopoly_server::app::{build_router, AppState};
use serde_json::Value;
use tower::ServiceExt;

fn router() -> Router {
    build_router(AppState::new())
}

/// 便捷请求：返回 JSON `Value`，失败时 panic（附状态码）。
async fn json_request(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, Value) {
    let mut req = Request::builder().uri(uri).method(method);
    if body.is_some() {
        req = req.header("content-type", "application/json");
    }
    let res = app
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

/// 建房并返回 (房号, 房主令牌)。
async fn create_room(app: &Router, body: &str) -> (u32, String) {
    let (status, json) = json_request(app, "POST", "/api/rooms", Some(body)).await;
    assert_eq!(status, StatusCode::OK, "建房应成功: {json}");
    (
        json["room_code"].as_u64().unwrap() as u32,
        json["owner_token"].as_str().unwrap().to_string(),
    )
}

/// 入座并返回玩家令牌。
async fn join_room(app: &Router, code: u32, name: &str, avatar: &str, color: &str) -> String {
    let (status, json) = json_request(
        app,
        "POST",
        &format!("/api/rooms/{code}/join"),
        Some(&format!(
            r#"{{"name":"{name}","avatar":"{avatar}","color":"{color}"}}"#
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "入座应成功: {json}");
    json["player_token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn health_ok() {
    let app = router();
    let (status, json) = json_request(&app, "GET", "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "ready");
    assert_eq!(json["service"], "monopoly-server");
    assert_eq!(json["version"], "0.1.0");
    assert!(json["uptime_secs"].as_u64().is_some());
    assert!(json["rooms"].as_u64().is_some());
    assert!(json["checks"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn create_room_returns_8_digit_code() {
    let app = router();
    let (status, json) = json_request(
        &app,
        "POST",
        "/api/rooms",
        Some(r#"{"settings":{"player_count_max":4}}"#),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let code = json["room_code"].as_u64().unwrap();
    assert!((10_000_000..=99_999_999).contains(&code));
    assert!(json["owner_token"].as_str().is_some());
}

#[tokio::test]
async fn create_room_rejects_bad_settings() {
    let app = router();
    let (status, _) = json_request(
        &app,
        "POST",
        "/api/rooms",
        Some(r#"{"settings":{"player_count_max":99}}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 未序列化字段非法也要拒绝（如 initial_money=0）。
    let (status, _) = json_request(
        &app,
        "POST",
        "/api/rooms",
        Some(r#"{"settings":{"initial_money":0}}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn join_and_query_room() {
    let app = router();
    let (code, _owner) = create_room(&app, r#"{"settings":{"player_count_max":2}}"#).await;

    let token = join_room(&app, code, "Alice", "dog", "red").await;
    assert!(!token.is_empty());

    let (status, json) = json_request(&app, "GET", &format!("/api/rooms/{code}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "lobby");
    assert_eq!(json["players"][0]["name"], "Alice");
    assert_eq!(json["tiles"].as_array().unwrap().len(), 40);
}

#[tokio::test]
async fn join_validates_name() {
    let app = router();
    let (code, _) = create_room(&app, "{}").await;

    let (status, _) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/join"),
        Some(r#"{"name":"   ","avatar":"dog","color":"red"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空昵称应被拒绝");

    let long = "a".repeat(17);
    let (status, _) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/join"),
        Some(&format!(
            r#"{{"name":"{long}","avatar":"dog","color":"red"}}"#
        )),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "超长昵称应被拒绝");
}

#[tokio::test]
async fn join_rejects_duplicate_avatar_color_combo() {
    let app = router();
    let (code, _) = create_room(&app, r#"{"settings":{"player_count_max":4}}"#).await;

    // 红色狗已被占用 → 再选红色狗应被拒绝。
    join_room(&app, code, "Alice", "dog", "red").await;
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/join"),
        Some(r#"{"name":"Bob","avatar":"dog","color":"red"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "重复组合应被拒绝: {json}");

    // 同形象不同颜色（红色狗 vs 绿色狗）允许。
    join_room(&app, code, "Bob", "dog", "green").await;

    // 快照应携带玩家的形象与颜色。
    let (_, json) = json_request(&app, "GET", &format!("/api/rooms/{code}"), None).await;
    assert_eq!(json["players"][0]["avatar"], "dog");
    assert_eq!(json["players"][0]["color"], "red");
    assert_eq!(json["players"][1]["avatar"], "dog");
    assert_eq!(json["players"][1]["color"], "green");
}

#[tokio::test]
async fn room_full_rejects_new_joins() {
    let app = router();
    let (code, _) = create_room(&app, r#"{"settings":{"player_count_max":2}}"#).await;
    join_room(&app, code, "Alice", "dog", "red").await;
    join_room(&app, code, "Bob", "car", "yellow").await;

    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/join"),
        Some(r#"{"name":"Eve","avatar":"dog","color":"green"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "满员应被拒绝: {json}");
}

#[tokio::test]
async fn get_unknown_room_is_404() {
    let app = router();
    let (status, _) = json_request(&app, "GET", "/api/rooms/99999999", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn start_requires_owner_token_and_min_players() {
    let app = router();
    let (code, owner) = create_room(&app, "{}").await;

    // 错误令牌 → 403。
    let (status, _) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/start"),
        Some(r#"{"owner_token":"wrong"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 正确令牌但人数不足 → 400。
    let (status, _) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/start"),
        Some(&format!(r#"{{"owner_token":"{owner}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 补足人数后开局成功。
    join_room(&app, code, "Alice", "dog", "red").await;
    join_room(&app, code, "Bob", "car", "yellow").await;
    let (status, json) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code}/start"),
        Some(&format!(r#"{{"owner_token":"{owner}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");

    let (status, json) = json_request(&app, "GET", &format!("/api/rooms/{code}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "playing");
}

#[tokio::test]
async fn two_rooms_are_isolated() {
    let app = router();
    let (code_a, owner_a) = create_room(&app, r#"{"settings":{"player_count_max":2}}"#).await;
    let (code_b, owner_b) = create_room(&app, r#"{"settings":{"player_count_max":2}}"#).await;
    assert_ne!(code_a, code_b, "两次建房房号应不同");

    // 房间 A 满员不影响房间 B 入座。
    join_room(&app, code_a, "Alice", "dog", "red").await;
    join_room(&app, code_a, "Bob", "car", "yellow").await;
    join_room(&app, code_b, "Carol", "dog", "red").await;

    // A 房主令牌不能开局 B 房；B 也要独立令牌。
    let (status, _) = json_request(
        &app,
        "POST",
        &format!("/api/rooms/{code_b}/start"),
        Some(&format!(r#"{{"owner_token":"{owner_a}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "跨房令牌必须无效");

    let (status, _) = json_request(&app, "GET", &format!("/api/rooms/{code_a}"), None).await;
    let (status_b, _) = json_request(&app, "GET", &format!("/api/rooms/{code_b}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(status_b, StatusCode::OK);
    let (_, json_b) = json_request(&app, "GET", &format!("/api/rooms/{code_b}"), None).await;
    let (_, json_a) = json_request(&app, "GET", &format!("/api/rooms/{code_a}"), None).await;
    let _ = owner_b;
    assert_eq!(json_a["players"][0]["name"], "Alice");
    assert_eq!(json_a["players"].as_array().unwrap().len(), 2);
    assert_eq!(json_b["players"][0]["name"], "Carol");
    assert_eq!(json_b["players"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn delete_room_requires_owner_and_removes_room() {
    let app = router();
    let (code, owner) = create_room(&app, "{}").await;

    let (status, _) = json_request(
        &app,
        "DELETE",
        &format!("/api/rooms/{code}"),
        Some(r#"{"owner_token":"wrong"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = json_request(
        &app,
        "DELETE",
        &format!("/api/rooms/{code}"),
        Some(&format!(r#"{{"owner_token":"{owner}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = json_request(&app, "GET", &format!("/api/rooms/{code}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "解散后房间应消失");
}

#[tokio::test]
async fn list_rooms_returns_summaries() {
    let app = router();
    // 空列表：无房间时返回空数组。
    let (status, json) = json_request(&app, "GET", "/api/rooms", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json.as_array().unwrap().len(), 0);

    // 建两个房间（一个带名字，一个未命名），再入座验证人数。
    let (code_named, _) = create_room(&app, r#"{"name":"周末大富翁"}"#).await;
    let (code_plain, _) = create_room(&app, "{}").await;
    join_room(&app, code_named, "Alice", "dog", "red").await;

    let (_, json) = json_request(&app, "GET", "/api/rooms", None).await;
    let rooms = json.as_array().unwrap();
    assert_eq!(rooms.len(), 2);

    let named = rooms
        .iter()
        .find(|r| r["code"].as_u64().unwrap() as u32 == code_named)
        .unwrap();
    assert_eq!(named["name"], "周末大富翁");
    assert_eq!(named["status"], "lobby");
    assert_eq!(named["players"], 1);
    assert_eq!(named["player_count_max"], 8);

    let plain = rooms
        .iter()
        .find(|r| r["code"].as_u64().unwrap() as u32 == code_plain)
        .unwrap();
    assert_eq!(plain["name"], "");
}

#[tokio::test]
async fn create_room_rejects_overlong_name() {
    let app = router();
    let long = "房".repeat(31);
    let (status, _) = json_request(
        &app,
        "POST",
        "/api/rooms",
        Some(&format!(r#"{{"name":"{long}"}}"#)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "超长房间名应被拒绝");
}
