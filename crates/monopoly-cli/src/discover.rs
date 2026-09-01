//! 局域网房间发现：向指定主机查询房间列表，或扫描本机 /24 子网的固定端口聚合列表。
//!
//! 协议：`GET /api/rooms` 返回 `Vec<RoomSummary>`（见 monopoly-common）。

use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use monopoly_common::room::RoomSummary;

/// 扫描时单个主机返回的响应。
#[derive(Debug, Clone)]
pub struct HostRooms {
    pub host: Ipv4Addr,
    pub rooms: Vec<RoomSummary>,
}

/// 单个请求的超时（扫描多台主机时控制总耗时）。
const REQUEST_TIMEOUT: Duration = Duration::from_millis(300);

/// 构造带连接超时的 HTTP 客户端（默认连接超时 30s 对扫描不可用）。
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(REQUEST_TIMEOUT)
        .build()
}

/// 向指定主机查询房间列表。
pub fn list_host(host: &str, port: u16) -> Result<Vec<RoomSummary>, String> {
    let url = format!("http://{host}:{port}/api/rooms");
    let body = agent()
        .get(&url)
        .timeout(Duration::from_millis(1500))
        .call()
        .map_err(|e| format!("{host}:{port} 不可达或非大富翁服务器: {e}"))?
        .into_string()
        .map_err(|e| format!("读取响应失败: {e}"))?;
    serde_json::from_str(&body).map_err(|e| format!("解析房间列表失败: {e}"))
}

/// 枚举本机 IPv4 的 /24 网段，排除回环/链路本地，去重。
///
/// 统一取 /24（而非网卡真实掩码）：家庭/小型局域网多为 /24；即便处在
/// 更大的网段（如 /16、/12），扫描本机所在 /24 也是最可能命中邻居的取值，
/// 同时避免扫描上万台主机的长耗时。
pub fn local_subnets() -> Vec<Ipv4Addr> {
    let mut subnets: Vec<Ipv4Addr> = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() || iface.is_link_local() {
                continue;
            }
            let if_addrs::IfAddr::V4(v4) = iface.addr else {
                continue;
            };
            let o = v4.ip.octets();
            let net = Ipv4Addr::new(o[0], o[1], o[2], 0);
            if !subnets.contains(&net) {
                subnets.push(net);
            }
        }
    }
    subnets
}

/// 扫描局域网固定端口，聚合所有主机的房间列表。
pub fn scan(port: u16) -> Vec<HostRooms> {
    let hosts: Vec<Ipv4Addr> = local_subnets()
        .into_iter()
        .flat_map(subnet_hosts)
        .collect();
    scan_hosts(&hosts, port)
}

/// 并发扫描指定主机列表，返回所有「可响应」主机的房间列表。
pub fn scan_hosts(hosts: &[Ipv4Addr], port: u16) -> Vec<HostRooms> {
    let mut seen = std::collections::HashSet::new();
    let hosts: Vec<Ipv4Addr> = hosts
        .iter()
        .copied()
        .filter(|h| seen.insert(*h))
        .collect();
    let n = hosts.len();
    if n == 0 {
        return Vec::new();
    }

    let next = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<Vec<HostRooms>>> = Arc::new(Mutex::new(Vec::new()));
    let shared_agent = Arc::new(agent());
    let workers = 32.min(n);
    let mut handles = Vec::new();
    for _ in 0..workers {
        let next = Arc::clone(&next);
        let results = Arc::clone(&results);
        let agent = Arc::clone(&shared_agent);
        let hosts = hosts.clone();
        handles.push(std::thread::spawn(move || loop {
            let i = next.fetch_add(1, Ordering::SeqCst);
            if i >= n {
                break;
            }
            if let Ok(rooms) = list_host_quiet(&agent, &hosts[i], port) {
                results.lock().unwrap().push(HostRooms {
                    host: hosts[i],
                    rooms,
                });
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }

    let mut out = results.lock().unwrap().clone();
    out.sort_by_key(|h| h.host.to_string());
    out
}

/// /24 网段的 1..=254 个主机地址。
fn subnet_hosts(net: Ipv4Addr) -> Vec<Ipv4Addr> {
    let o = net.octets();
    (1..=254)
        .map(|last| Ipv4Addr::new(o[0], o[1], o[2], last))
        .collect()
}

/// 扫描用静默请求：不可达/非服务器一律视为 `None`（正常现象，不打印噪音）。
fn list_host_quiet(
    agent: &ureq::Agent,
    host: &Ipv4Addr,
    port: u16,
) -> Result<Vec<RoomSummary>, String> {
    let url = format!("http://{host}:{port}/api/rooms");
    let body = agent
        .get(&url)
        .timeout(REQUEST_TIMEOUT)
        .call()
        .map_err(|_| "unreachable".to_string())?
        .into_string()
        .map_err(|_| "unreadable".to_string())?;
    serde_json::from_str(&body).map_err(|_| "not a monopoly server".to_string())
}

/// 打印单个主机的房间列表。
pub fn print_host(h: &HostRooms) {
    println!("{}：", h.host);
    if h.rooms.is_empty() {
        println!("  （服务器在线，暂无房间）");
        return;
    }
    print_table(&h.rooms);
}

/// 打印一组房间（`list` 命令）。
pub fn print_list(rooms: &[RoomSummary]) {
    if rooms.is_empty() {
        println!("该主机上暂无房间");
        return;
    }
    print_table(rooms);
}

fn print_table(rooms: &[RoomSummary]) {
    println!("  {:<12}{:<20}{:<10}{:<6}", "房号", "房间名", "状态", "人数");
    for r in rooms {
        let name = if r.name.is_empty() {
            "（未命名）".to_string()
        } else {
            r.name.clone()
        };
        println!(
            "  {:<12}{:<20}{:<10}{}/{}",
            format!("{:08}", r.code),
            name,
            r.status,
            r.players,
            r.player_count_max
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subnet_hosts_covers_254_hosts() {
        let hosts = subnet_hosts(Ipv4Addr::new(192, 168, 1, 0));
        assert_eq!(hosts.len(), 254);
        assert_eq!(hosts[0], Ipv4Addr::new(192, 168, 1, 1));
        assert_eq!(hosts[253], Ipv4Addr::new(192, 168, 1, 254));
    }

    #[test]
    fn scan_hosts_deduplicates() {
        // 重复主机只扫一次；端口 1 在本地几乎必然关闭 → 快速失败。
        let hosts = vec![Ipv4Addr::LOCALHOST, Ipv4Addr::LOCALHOST];
        let out = scan_hosts(&hosts, 1);
        assert!(out.is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_and_scan_find_rooms_on_running_server() {
        use monopoly_server::app::{build_router, AppState};

        // 真实起服（多线程运行时，阻塞式 ureq 调用不卡住服务器任务）。
        let app = build_router(AppState::new());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // 通过 HTTP 建一个带名字的房间。
        let url = format!("http://127.0.0.1:{port}/api/rooms");
        let resp = ureq::post(&url)
            .set("Content-Type", "application/json")
            .send_string(r#"{"name":"周末大富翁"}"#)
            .unwrap();
        assert_eq!(resp.status(), 200);

        // `list <ip>`：指定主机查房。
        let rooms = list_host("127.0.0.1", port).unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].name, "周末大富翁");

        // `scan`：扫描固定端口（此处限定该主机）也能发现。
        let found = scan_hosts(&[Ipv4Addr::new(127, 0, 0, 1)], port);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rooms.len(), 1);
        assert_eq!(found[0].rooms[0].code, rooms[0].code);
    }
}
