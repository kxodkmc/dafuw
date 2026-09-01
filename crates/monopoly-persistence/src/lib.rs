//! 对局持久化接口（预留）。
//!
//! 当前为接口定义与内存示例实现，不阻塞主流程；
//! 后续用 SQLite + SQLx 实现 [`GameRepository`]，即可接入战绩/回放/断线重连。

use monopoly_common::event::{Event, PlayerRank};
use uuid::Uuid;

/// 对局持久化仓库接口。
pub trait GameRepository {
    /// 保存对局事件流（供回放）。
    fn save_events(&self, game_id: Uuid, events: &[Event]) -> Result<(), String>;

    /// 读取对局事件流。
    fn load_events(&self, game_id: Uuid) -> Result<Vec<Event>, String>;

    /// 保存对局结果（战绩）。
    fn save_result(&self, game_id: Uuid, winner: Uuid, rank: &[PlayerRank]) -> Result<(), String>;
}

/// 内存实现：用于测试与演示，后续替换为 SQLite 实现。
#[derive(Debug, Default)]
pub struct InMemoryRepository {
    games: std::sync::Mutex<Vec<(Uuid, Vec<Event>)>>,
}

impl GameRepository for InMemoryRepository {
    fn save_events(&self, game_id: Uuid, events: &[Event]) -> Result<(), String> {
        let mut games = self.games.lock().map_err(|_| "锁已污染".to_string())?;
        if let Some((_, v)) = games.iter_mut().find(|(id, _)| *id == game_id) {
            v.extend_from_slice(events);
        } else {
            games.push((game_id, events.to_vec()));
        }
        Ok(())
    }

    fn load_events(&self, game_id: Uuid) -> Result<Vec<Event>, String> {
        let games = self.games.lock().map_err(|_| "锁已污染".to_string())?;
        Ok(games
            .iter()
            .find(|(id, _)| *id == game_id)
            .map(|(_, v)| v.clone())
            .unwrap_or_default())
    }

    fn save_result(
        &self,
        _game_id: Uuid,
        _winner: Uuid,
        _rank: &[PlayerRank],
    ) -> Result<(), String> {
        Ok(())
    }
}
