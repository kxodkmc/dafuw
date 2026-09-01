//! 本地开服入口：默认监听 127.0.0.1（本地主机开服，房主自己也入座即可）。
//!
//! 云端开服时用 `--bind 0.0.0.0`，配反向代理提供 TLS 即可，无需改代码。

use std::net::SocketAddr;
use std::sync::Arc;

use monopoly_persistence::SqliteRepository;
use monopoly_server::app::{build_router, AppState};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("monopoly_server=info")),
        )
        .init();

    let (host, port) = parse_args(std::env::args().skip(1).collect());
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("无效的监听地址，示例: --host 127.0.0.1 --port 8080");

    // 持久化：数据库路径可用环境变量 `MONOPOLY_DB` 覆盖，默认 `data/monopoly.db`。
    let db_path =
        std::env::var("MONOPOLY_DB").unwrap_or_else(|_| "data/monopoly.db".to_string());
    let repo = Arc::new(SqliteRepository::open(&db_path).expect("初始化数据库失败"));
    let state = AppState::with_repository(repo);
    match state.manager.restore_all() {
        Ok(n) => {
            if n > 0 {
                tracing::info!("已恢复 {n} 局进行中的对局，玩家可凭原房号与令牌继续");
            }
        }
        Err(e) => tracing::error!("恢复进行中的对局失败: {e}"),
    }

    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("监听端口失败，可能已被占用");
    tracing::info!("大富翁服务器已启动: http://{addr}，存档数据库: {db_path}");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("服务器异常退出: {e}");
    }
}

/// 解析启动参数：`--host/--bind <addr>` 与 `--port <port>`。
fn parse_args(args: Vec<String>) -> (String, String) {
    let mut host = "127.0.0.1".to_string();
    let mut port = "8080".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--host" | "--bind" | "-h" => {
                if let Some(v) = args.get(i + 1) {
                    host = v.clone();
                }
                i += 2;
            }
            "--port" | "-p" => {
                if let Some(v) = args.get(i + 1) {
                    port = v.clone();
                }
                i += 2;
            }
            other => {
                tracing::warn!("忽略未知参数: {other}");
                i += 1;
            }
        }
    }
    (host, port)
}
