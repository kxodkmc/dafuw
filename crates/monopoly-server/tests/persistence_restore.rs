//! 持久化集成测试：模拟服务器重启，验证进行中的对局可凭原房号/原令牌继续。
//!
//! 流程：第一次"开服"建房、入座、开局并产生存档 → 用共享仓库"重启"
//! （新 RoomManager + `restore_all`）→ 原房号/原令牌仍有效 → 可继续操作且状态一致。

use std::sync::Arc;
use std::time::Duration;

use monopoly_common::command::Command;
use monopoly_common::room::RoomSettings;
use monopoly_persistence::{GameRepository, InMemoryRepository};
use monopoly_server::actor::{PlayerIdentity, Session};
use monopoly_server::room_manager::RoomManager;

/// 测试用全局身份：n 不同即身份不同。
fn ident(n: u128) -> PlayerIdentity {
    PlayerIdentity {
        player_uid: uuid::Uuid::from_u128(n),
        secret: uuid::Uuid::from_u128(n + 10_000),
    }
}

/// 等待指定对局落盘（Actor 在回复后异步写库）。
async fn wait_saved(repo: &Arc<InMemoryRepository>, code: u32, status: &str) {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let code = monopoly_common::room::RoomCode::new(code).unwrap();
            if let Some(g) = repo.load_game(code).unwrap() {
                if g.archive.state.status == status {
                    break;
                }
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("对局应在超时前落盘");
}

#[tokio::test]
async fn restore_room_and_continue_with_same_tokens() {
    let repo = Arc::new(InMemoryRepository::default());
    let settings = RoomSettings {
        player_count_max: 2,
        ..Default::default()
    };

    // —— 第一次"开服" ——
    let manager = RoomManager::with_repo(repo.clone());
    let (code, owner_token) = manager.create(settings).unwrap();
    let handle = manager.get(code).expect("房间应存在");

    let a = handle.request_join(ident(1)).await.unwrap();
    let (a_id, a_token) = (a.player_id, a.token);
    let b = handle.request_join(ident(2)).await.unwrap();
    let b_token = b.token;
    // 双方就绪后才能开局。
    handle
        .command(
            handle.resolve_token(&a_token).expect("令牌应有效"),
            Command::SetReady { ready: true },
        )
        .await
        .expect("设置就绪应成功");
    handle
        .command(
            handle.resolve_token(&b_token).expect("令牌应有效"),
            Command::SetReady { ready: true },
        )
        .await
        .expect("设置就绪应成功");
    handle.start().await.expect("开局应成功");
    wait_saved(&repo, code.as_u32(), "playing").await;

    // —— 模拟重启：新建 RoomManager，共享同一仓库 ——
    let manager2 = RoomManager::with_repo(repo.clone());
    assert_eq!(manager2.restore_all().unwrap(), 1, "应恢复 1 局");
    let handle2 = manager2.get(code).expect("恢复后房间应存在");

    // 原房主令牌、原玩家令牌均继续有效，且玩家令牌仍指向同一玩家。
    assert_eq!(handle2.resolve_token(&owner_token), Some(Session::Owner));
    assert_eq!(handle2.resolve_token(&a_token), Some(Session::Player(a_id)));

    // 快照一致：仍为进行中，且轮到原回合玩家。
    let snap = handle2.query_snapshot().await.unwrap();
    assert_eq!(snap.status, "playing");
    let rolling = snap.current_player.expect("应有人行动");

    // 继续操作：当前回合玩家掷骰 → 位置应发生变化。
    let token = if rolling == a_id {
        a_token.clone()
    } else {
        b_token.clone()
    };
    let before_pos = snap
        .players
        .iter()
        .find(|p| p.id == rolling)
        .expect("玩家应在快照中")
        .position;
    let session = handle2.resolve_token(&token).expect("令牌应有效");
    handle2
        .command(session, Command::RollDice)
        .await
        .expect("恢复后应能继续操作");

    let after = handle2.query_snapshot().await.unwrap();
    let after_pos = after
        .players
        .iter()
        .find(|p| p.id == rolling)
        .expect("玩家应在快照中")
        .position;
    assert_ne!(after_pos, before_pos, "掷骰后应移动");
}
