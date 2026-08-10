//! routing v2 本机目录的最小访问栅栏。
//!
//! 这里只验证 v8 `routing_v2_store_state` 的非敏感元数据，绝不读取旧 Provider、
//! 配置 JSON、Secret 或任何其他 routing v2 表。它不在请求热路径中运行。

use super::{ROUTING_V2_MINIMUM_READER_VERSION, SCHEMA_VERSION};
use rusqlite::Connection;
use thiserror::Error;

const STORE_STATE_TABLE: &str = "routing_v2_store_state";
const CURRENT_READER_VERSION: i64 = SCHEMA_VERSION as i64;

/// Store Gate 的封闭失败码。
///
/// 不向调用方回显 SQLite 原始错误、表定义或任何数据内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum StoreGateError {
    #[error("routing_v2_store_unavailable")]
    Unavailable,
    #[error("routing_v2_store_state_missing")]
    StateMissing,
    #[error("routing_v2_store_state_invalid")]
    StateInvalid,
    #[error("routing_v2_store_reader_incompatible")]
    ReaderIncompatible,
}

/// 已通过本机 routing v2 状态校验的私有 capability。
///
/// 它没有公开构造器、格式化实现或状态字段，未来 repository 只能从本模块取得。
pub(crate) struct RoutingV2StoreAccess {
    _private: (),
}

/// 验证 routing v2 本机目录可被当前 reader 安全打开。
///
/// 查询范围刻意限制为 `sqlite_master` 与 `routing_v2_store_state`。没有回退到
/// 旧 Provider 或设置表：缺表、无行、异常行和查询失败一律拒绝访问。
pub(crate) fn acquire(conn: &Connection) -> Result<RoutingV2StoreAccess, StoreGateError> {
    if !store_state_table_exists(conn)? {
        return Err(StoreGateError::StateMissing);
    }

    let state = read_single_state(conn)?;
    if state.id != 1
        || state.migration_epoch < 1
        || state.minimum_reader_version < ROUTING_V2_MINIMUM_READER_VERSION
        || state.rollback_generation < 0
    {
        return Err(StoreGateError::StateInvalid);
    }

    if state.minimum_reader_version > CURRENT_READER_VERSION {
        return Err(StoreGateError::ReaderIncompatible);
    }

    Ok(RoutingV2StoreAccess { _private: () })
}

fn store_state_table_exists(conn: &Connection) -> Result<bool, StoreGateError> {
    conn.query_row(
        "SELECT EXISTS(\
            SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1\
        )",
        [STORE_STATE_TABLE],
        |row| row.get(0),
    )
    .map_err(|_| StoreGateError::Unavailable)
}

fn read_single_state(conn: &Connection) -> Result<StoreState, StoreGateError> {
    let mut statement = conn
        .prepare(
            "SELECT id, migration_epoch, minimum_reader_version, rollback_generation \
             FROM routing_v2_store_state ORDER BY id LIMIT 2",
        )
        .map_err(|_| StoreGateError::Unavailable)?;
    let mut rows = statement
        .query([])
        .map_err(|_| StoreGateError::Unavailable)?;

    let first = rows
        .next()
        .map_err(|_| StoreGateError::Unavailable)?
        .ok_or(StoreGateError::StateInvalid)?;
    let state = StoreState {
        id: first.get(0).map_err(|_| StoreGateError::StateInvalid)?,
        migration_epoch: first.get(1).map_err(|_| StoreGateError::StateInvalid)?,
        minimum_reader_version: first.get(2).map_err(|_| StoreGateError::StateInvalid)?,
        rollback_generation: first.get(3).map_err(|_| StoreGateError::StateInvalid)?,
    };

    if rows
        .next()
        .map_err(|_| StoreGateError::Unavailable)?
        .is_some()
    {
        return Err(StoreGateError::StateInvalid);
    }

    Ok(state)
}

struct StoreState {
    id: i64,
    migration_epoch: i64,
    minimum_reader_version: i64,
    rollback_generation: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_only_connection() -> Connection {
        let conn = Connection::open_in_memory().expect("打开内存数据库");
        conn.execute_batch(
            "CREATE TABLE routing_v2_store_state (\
                id INTEGER,\
                migration_epoch INTEGER,\
                minimum_reader_version INTEGER,\
                rollback_generation INTEGER\
            )",
        )
        .expect("创建最小状态表");
        conn
    }

    fn insert_state(
        conn: &Connection,
        id: i64,
        migration_epoch: i64,
        minimum_reader_version: i64,
        rollback_generation: i64,
    ) {
        conn.execute(
            "INSERT INTO routing_v2_store_state (\
                id, migration_epoch, minimum_reader_version, rollback_generation\
            ) VALUES (?1, ?2, ?3, ?4)",
            (
                id,
                migration_epoch,
                minimum_reader_version,
                rollback_generation,
            ),
        )
        .expect("写入状态行");
    }

    #[test]
    fn gate_accepts_valid_state_without_any_legacy_provider_table() {
        let conn = state_only_connection();
        insert_state(&conn, 1, 1, ROUTING_V2_MINIMUM_READER_VERSION, 0);

        assert!(acquire(&conn).is_ok());
        let provider_tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name IN ('providers', 'provider_endpoints')",
                [],
                |row| row.get(0),
            )
            .expect("读取测试表清单");
        assert_eq!(provider_tables, 0, "测试库不应包含 legacy Provider 表");
    }

    #[test]
    fn gate_rejects_missing_state_table() {
        let conn = Connection::open_in_memory().expect("打开内存数据库");

        assert!(matches!(acquire(&conn), Err(StoreGateError::StateMissing)));
    }

    #[test]
    fn gate_rejects_multiple_state_rows() {
        let conn = state_only_connection();
        insert_state(&conn, 1, 1, 1, 0);
        insert_state(&conn, 2, 1, 1, 0);

        assert!(matches!(acquire(&conn), Err(StoreGateError::StateInvalid)));
    }

    #[test]
    fn gate_rejects_non_singleton_id() {
        let conn = state_only_connection();
        insert_state(&conn, 2, 1, 1, 0);

        assert!(matches!(acquire(&conn), Err(StoreGateError::StateInvalid)));
    }

    #[test]
    fn gate_rejects_reader_version_newer_than_current_reader() {
        let conn = state_only_connection();
        insert_state(&conn, 1, 1, CURRENT_READER_VERSION + 1, 0);

        assert!(matches!(
            acquire(&conn),
            Err(StoreGateError::ReaderIncompatible)
        ));
    }

    #[test]
    fn gate_rejects_reader_version_older_than_v8_store_contract() {
        let conn = state_only_connection();
        insert_state(&conn, 1, 1, ROUTING_V2_MINIMUM_READER_VERSION - 1, 0);

        assert!(matches!(acquire(&conn), Err(StoreGateError::StateInvalid)));
    }

    #[test]
    fn gate_rejects_zero_migration_epoch() {
        let conn = state_only_connection();
        insert_state(&conn, 1, 0, 1, 0);

        assert!(matches!(acquire(&conn), Err(StoreGateError::StateInvalid)));
    }

    #[test]
    fn gate_rejects_negative_rollback_generation() {
        let conn = state_only_connection();
        insert_state(&conn, 1, 1, 1, -1);

        assert!(matches!(acquire(&conn), Err(StoreGateError::StateInvalid)));
    }
}
