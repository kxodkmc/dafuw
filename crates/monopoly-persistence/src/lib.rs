//! 对局持久化：SQLite 快照存档，支持服务器重启后恢复进行中的对局。
//!
//! 核心是 [`GameRepository`] 接口；[`SqliteRepository`] 为生产实现
//! （rusqlite bundled，免系统依赖），[`InMemoryRepository`] 供测试/演示。
//! 服务器侧经 `spawn_blocking` 调用本接口，避免阻塞异步运行时。

use std::path::Path;
use std::sync::Mutex;

use monopoly_common::archive::GameArchive;
use monopoly_common::room::RoomCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 一条已持久化的对局记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredGame {
    /// 8 位房号，即对局唯一键。
    pub code: RoomCode,
    /// 房主令牌：重启后原房主凭此令牌继续操作。
    pub owner_token: String,
    /// (玩家令牌, 玩家 id) 列表：重启后原玩家凭原令牌直接重连。
    pub player_tokens: Vec<(String, Uuid)>,
    /// 完整对局存档。
    pub archive: GameArchive,
}

/// 对局持久化仓库接口。
pub trait GameRepository: Send + Sync {
    /// 保存（覆盖）一局对局。
    fn save_game(&self, game: &StoredGame) -> Result<(), String>;
    /// 按房号读取一局对局。
    fn load_game(&self, code: RoomCode) -> Result<Option<StoredGame>, String>;
    /// 删除一局对局（房间解散时调用）。
    fn delete_game(&self, code: RoomCode) -> Result<(), String>;
    /// 列出所有未结束的对局（服务器启动恢复用）。
    fn list_active_games(&self) -> Result<Vec<StoredGame>, String>;
}

/// 内存实现：测试/演示用。
#[derive(Debug, Default)]
pub struct InMemoryRepository {
    games: Mutex<Vec<StoredGame>>,
}

impl GameRepository for InMemoryRepository {
    fn save_game(&self, game: &StoredGame) -> Result<(), String> {
        let mut games = self.games.lock().map_err(|_| "锁已污染".to_string())?;
        if let Some(g) = games.iter_mut().find(|g| g.code == game.code) {
            *g = game.clone();
        } else {
            games.push(game.clone());
        }
        Ok(())
    }

    fn load_game(&self, code: RoomCode) -> Result<Option<StoredGame>, String> {
        let games = self.games.lock().map_err(|_| "锁已污染".to_string())?;
        Ok(games.iter().find(|g| g.code == code).cloned())
    }

    fn delete_game(&self, code: RoomCode) -> Result<(), String> {
        let mut games = self.games.lock().map_err(|_| "锁已污染".to_string())?;
        games.retain(|g| g.code != code);
        Ok(())
    }

    fn list_active_games(&self) -> Result<Vec<StoredGame>, String> {
        let games = self.games.lock().map_err(|_| "锁已污染".to_string())?;
        Ok(games
            .iter()
            .filter(|g| g.archive.state.status != "ended")
            .cloned()
            .collect())
    }
}

/// SQLite 实现（rusqlite bundled，免系统依赖）。
pub struct SqliteRepository {
    conn: Mutex<rusqlite::Connection>,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS games (
    code          TEXT PRIMARY KEY,
    owner_token   TEXT NOT NULL,
    player_tokens TEXT NOT NULL,
    archive       TEXT NOT NULL,
    ended         INTEGER NOT NULL DEFAULT 0,
    updated_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_games_ended ON games(ended);
";

impl SqliteRepository {
    /// 打开（不存在则创建）数据库；`:memory:` 可用于测试。
    pub fn open(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("创建数据库目录失败: {e}"))?;
            }
        }
        let conn = rusqlite::Connection::open(path)
            .map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| format!("初始化表结构失败: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }
}

impl GameRepository for SqliteRepository {
    fn save_game(&self, game: &StoredGame) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "数据库锁已污染".to_string())?;
        let archive = serde_json::to_string(&game.archive)
            .map_err(|e| format!("序列化对局存档失败: {e}"))?;
        let player_tokens = serde_json::to_string(&game.player_tokens)
            .map_err(|e| format!("序列化玩家令牌失败: {e}"))?;
        let ended = i64::from(game.archive.state.status == "ended");
        conn.execute(
            "INSERT OR REPLACE INTO games
             (code, owner_token, player_tokens, archive, ended, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                game.code.to_string(),
                game.owner_token,
                player_tokens,
                archive,
                ended,
                now_unix()
            ],
        )
        .map_err(|e| format!("写入对局失败: {e}"))?;
        Ok(())
    }

    fn load_game(&self, code: RoomCode) -> Result<Option<StoredGame>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库锁已污染".to_string())?;
        let mut stmt = conn
            .prepare("SELECT owner_token, player_tokens, archive FROM games WHERE code = ?1")
            .map_err(|e| format!("查询对局失败: {e}"))?;
        let mut rows = stmt
            .query_map(rusqlite::params![code.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| format!("查询对局失败: {e}"))?;
        match rows.next() {
            Some(Ok((owner_token, player_tokens, archive))) => {
                let (owner_token, player_tokens, archive) =
                    decode(owner_token, player_tokens, archive)?;
                Ok(Some(StoredGame {
                    code,
                    owner_token,
                    player_tokens,
                    archive,
                }))
            }
            Some(Err(e)) => Err(format!("读取对局失败: {e}")),
            None => Ok(None),
        }
    }

    fn delete_game(&self, code: RoomCode) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "数据库锁已污染".to_string())?;
        conn.execute("DELETE FROM games WHERE code = ?1", rusqlite::params![code.to_string()])
            .map_err(|e| format!("删除对局失败: {e}"))?;
        Ok(())
    }

    fn list_active_games(&self) -> Result<Vec<StoredGame>, String> {
        let conn = self.conn.lock().map_err(|_| "数据库锁已污染".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT code, owner_token, player_tokens, archive
                 FROM games WHERE ended = 0 ORDER BY updated_at",
            )
            .map_err(|e| format!("查询对局失败: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| format!("查询对局失败: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            let (code, owner_token, player_tokens, archive) =
                row.map_err(|e| format!("读取对局失败: {e}"))?;
            let code: RoomCode = code
                .parse::<u32>()
                .map_err(|_| format!("数据库中的非法房号: {code}"))
                .and_then(RoomCode::new)?;
            let (owner_token, player_tokens, archive) =
                decode(owner_token, player_tokens, archive)?;
            out.push(StoredGame {
                code,
                owner_token,
                player_tokens,
                archive,
            });
        }
        Ok(out)
    }
}

/// 单行记录解码后的数据（房号由调用方解析）。
type DecodedRow = (String, Vec<(String, Uuid)>, GameArchive);

/// 反序列化单行记录。
fn decode(
    owner_token: String,
    player_tokens: String,
    archive: String,
) -> Result<DecodedRow, String> {
    let player_tokens = serde_json::from_str(&player_tokens)
        .map_err(|e| format!("解析玩家令牌失败: {e}"))?;
    let archive =
        serde_json::from_str(&archive).map_err(|e| format!("解析对局存档失败: {e}"))?;
    Ok((owner_token, player_tokens, archive))
}

/// 当前 Unix 时间戳（秒）。
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use monopoly_common::room::RoomSettings;
    use monopoly_core::engine::GameEngine;

    fn sample_game() -> StoredGame {
        let engine = GameEngine::new(RoomSettings::default()).unwrap();
        StoredGame {
            code: RoomCode::new(12_345_678).unwrap(),
            owner_token: "owner".to_string(),
            player_tokens: vec![("tk-a".to_string(), Uuid::new_v4())],
            archive: engine.archive(),
        }
    }

    #[test]
    fn in_memory_save_load_delete() {
        let repo = InMemoryRepository::default();
        let game = sample_game();
        repo.save_game(&game).unwrap();
        assert_eq!(repo.load_game(game.code).unwrap().unwrap().code, game.code);
        repo.delete_game(game.code).unwrap();
        assert!(repo.load_game(game.code).unwrap().is_none());
    }

    #[test]
    fn sqlite_save_load_delete_roundtrip() {
        let repo = SqliteRepository::open(":memory:").unwrap();
        let game = sample_game();
        repo.save_game(&game).unwrap();
        let loaded = repo.load_game(game.code).unwrap().expect("应能读回");
        assert_eq!(loaded.code, game.code);
        assert_eq!(loaded.owner_token, game.owner_token);
        assert_eq!(loaded.player_tokens, game.player_tokens);
        repo.delete_game(game.code).unwrap();
        assert!(repo.load_game(game.code).unwrap().is_none());
    }

    #[test]
    fn sqlite_active_games_exclude_ended() {
        let repo = SqliteRepository::open(":memory:").unwrap();
        let active = sample_game();
        let mut ended = sample_game();
        ended.code = RoomCode::new(87_654_321).unwrap();
        repo.save_game(&active).unwrap();
        repo.save_game(&ended).unwrap();
        assert_eq!(repo.list_active_games().unwrap().len(), 2);

        // 结束后存档写回 → 不再被视为进行中。
        ended.archive.state.status = "ended".to_string();
        repo.save_game(&ended).unwrap();
        let active_codes: Vec<RoomCode> = repo
            .list_active_games()
            .unwrap()
            .into_iter()
            .map(|g| g.code)
            .collect();
        assert_eq!(active_codes, vec![active.code]);
    }
}
