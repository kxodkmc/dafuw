//! 大富翁局域网开服 / 找房工具。
//!
//! - 无子命令或 `serve`：启动服务器（局域网开服用 `--bind 0.0.0.0`，他人即可访问）。
//! - `list <ip>[:port]`：输入房主 IP，查询该主机上的房间列表。
//! - `scan [--port <port>]`：扫描局域网固定端口，聚合列出所有主机的房间。

mod discover;

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
                .unwrap_or_else(|_| EnvFilter::new("monopoly_cli=info")),
        )
        .init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("list") => run_list(&args[1..]),
        Some("scan") => run_scan(&args[1..]),
        Some("serve") => run_server(&args[1..]).await,
        None => run_server(&args).await,
        Some(other) => {
            eprintln!("未知命令: {other}\n");
            print_usage();
            std::process::exit(2);
        }
    }
}

/// 启动服务器。
async fn run_server(args: &[String]) {
    let (host, port) = parse_server_args(args);
    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .expect("无效的监听地址，示例: --host 0.0.0.0 --port 8080");

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
    tracing::info!(
        "大富翁服务器已启动: http://{addr}，存档数据库: {db_path}\n局域网开服请确认已用 --bind 0.0.0.0，他人可用 `monopoly-cli scan` 或 `monopoly-cli list <本机IP>:<端口>` 找房"
    );
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("服务器异常退出: {e}");
    }
}

/// 输入房主 IP，列出该主机房间。
fn run_list(args: &[String]) {
    let Some(raw) = args.first() else {
        eprintln!("用法: monopoly-cli list <ip>[:port]（port 默认 8080）");
        std::process::exit(2);
    };
    let (host, port) = parse_host_port(raw);
    match discover::list_host(&host, port) {
        Ok(rooms) => {
            println!("{host}:{port} 上的房间：");
            discover::print_list(&rooms);
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

/// 扫描局域网固定端口，聚合所有房间。
fn run_scan(args: &[String]) {
    let port = parse_opt_port(args);
    println!("正在扫描局域网端口 {port}（本机 /24 网段）...");
    let found = discover::scan(port);
    if found.is_empty() {
        println!("未发现任何大富翁服务器（请确认房主用 --bind 0.0.0.0 开服且在同一局域网）");
        return;
    }
    println!("发现 {} 台服务器：", found.len());
    for h in &found {
        discover::print_host(h);
    }
}

/// 解析服务器参数：`--host/--bind <addr>` 与 `--port <port>`。
fn parse_server_args(args: &[String]) -> (String, String) {
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

/// 解析 `ip[:port]`，端口默认 8080。
fn parse_host_port(raw: &str) -> (String, u16) {
    if let Some((h, p)) = raw.rsplit_once(':') {
        (h.to_string(), p.parse().unwrap_or(8080))
    } else {
        (raw.to_string(), 8080)
    }
}

/// 解析 `--port <port>`，默认 8080。
fn parse_opt_port(args: &[String]) -> u16 {
    let mut port = 8080;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--port" || args[i] == "-p" {
            if let Some(v) = args.get(i + 1) {
                port = v.parse().unwrap_or(8080);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    port
}

fn print_usage() {
    println!(
        "用法:\n  monopoly-cli [serve] [--bind <ip>] [--port <port>]   启动服务器（默认 127.0.0.1:8080；局域网开服用 --bind 0.0.0.0）\n  monopoly-cli list <ip>[:port]                             查询指定主机的房间列表\n  monopoly-cli scan [--port <port>]                         扫描局域网固定端口，列出所有房间"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_port_parsing() {
        assert_eq!(parse_host_port("192.168.1.5"), ("192.168.1.5".into(), 8080));
        assert_eq!(
            parse_host_port("192.168.1.5:9090"),
            ("192.168.1.5".into(), 9090)
        );
    }

    #[test]
    fn opt_port_parsing() {
        assert_eq!(parse_opt_port(&[]), 8080);
        assert_eq!(parse_opt_port(&["--port".into(), "7070".into()]), 7070);
    }
}
