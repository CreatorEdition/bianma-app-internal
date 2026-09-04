//! Provider 测速结果数据访问层
//!
//! 仅保存最近一次批量延迟测速结果，供前端读取缓存状态。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// Provider 延迟测试结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderLatencyResult {
    pub provider_id: String,
    pub app_type: String,
    pub base_url: String,
    pub latency_ms: Option<i64>,
    pub status: Option<i64>,
    pub error: Option<String>,
    pub tested_at: i64,
}

impl Database {
    /// 保存单个 provider 的最近一次测速结果。
    pub fn save_provider_latency_result(
        &self,
        result: &ProviderLatencyResult,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO provider_latency_results
             (provider_id, app_type, base_url, latency_ms, status, error, tested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                result.provider_id,
                result.app_type,
                result.base_url,
                result.latency_ms,
                result.status,
                result.error,
                result.tested_at,
            ],
        )
        .map_err(|e| AppError::Database(format!("保存 provider 测速结果失败: {e}")))?;
        Ok(())
    }

    /// 获取指定 provider 的最近一次测速结果。
    pub fn get_provider_latency_result(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Result<Option<ProviderLatencyResult>, AppError> {
        let conn = lock_conn!(self.conn);
        conn.query_row(
            "SELECT provider_id, app_type, base_url, latency_ms, status, error, tested_at
             FROM provider_latency_results
             WHERE provider_id = ?1 AND app_type = ?2",
            params![provider_id, app_type],
            provider_latency_result_from_row,
        )
        .optional()
        .map_err(|e| AppError::Database(format!("查询 provider 测速结果失败: {e}")))
    }

    /// 获取指定应用下所有 provider 的最近一次测速结果。
    pub fn get_all_provider_latency_results(
        &self,
        app_type: &str,
    ) -> Result<Vec<ProviderLatencyResult>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT provider_id, app_type, base_url, latency_ms, status, error, tested_at
                 FROM provider_latency_results
                 WHERE app_type = ?1
                 ORDER BY tested_at DESC, provider_id ASC",
            )
            .map_err(|e| AppError::Database(format!("准备查询 provider 测速结果失败: {e}")))?;

        let results = stmt
            .query_map(params![app_type], provider_latency_result_from_row)
            .map_err(|e| AppError::Database(format!("查询 provider 测速结果失败: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Database(format!("读取 provider 测速结果失败: {e}")))?;

        Ok(results)
    }

    /// 删除指定 provider 的测速缓存。
    pub fn delete_provider_latency_result(
        &self,
        provider_id: &str,
        app_type: &str,
    ) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM provider_latency_results WHERE provider_id = ?1 AND app_type = ?2",
            params![provider_id, app_type],
        )
        .map_err(|e| AppError::Database(format!("删除 provider 测速结果失败: {e}")))?;
        Ok(())
    }

    /// 清理指定时间之前的测速缓存。
    pub fn cleanup_old_latency_results(&self, before_timestamp: i64) -> Result<usize, AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM provider_latency_results WHERE tested_at < ?1",
            params![before_timestamp],
        )
        .map_err(|e| AppError::Database(format!("清理 provider 测速结果失败: {e}")))
    }
}

fn provider_latency_result_from_row(
    row: &rusqlite::Row<'_>,
) -> Result<ProviderLatencyResult, rusqlite::Error> {
    Ok(ProviderLatencyResult {
        provider_id: row.get(0)?,
        app_type: row.get(1)?,
        base_url: row.get(2)?,
        latency_ms: row.get(3)?,
        status: row.get(4)?,
        error: row.get(5)?,
        tested_at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use serde_json::json;

    fn seed_provider(db: &Database, provider_id: &str, app_type: &str) {
        let provider = Provider::with_id(
            provider_id.to_string(),
            format!("Provider {provider_id}"),
            json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.example.com"
                }
            }),
            None,
        );
        db.save_provider(app_type, &provider)
            .expect("写入测试 provider");
    }

    fn latency_result(provider_id: &str, latency_ms: i64, tested_at: i64) -> ProviderLatencyResult {
        ProviderLatencyResult {
            provider_id: provider_id.to_string(),
            app_type: "claude".to_string(),
            base_url: format!("https://{provider_id}.example.com"),
            latency_ms: Some(latency_ms),
            status: Some(200),
            error: None,
            tested_at,
        }
    }

    #[test]
    fn latency_save_and_get_round_trip() {
        let db = Database::memory().expect("创建内存数据库");
        seed_provider(&db, "test-provider", "claude");

        let result = latency_result("test-provider", 123, 1_234_567_890);
        db.save_provider_latency_result(&result)
            .expect("保存测速结果");

        let retrieved = db
            .get_provider_latency_result("test-provider", "claude")
            .expect("查询测速结果")
            .expect("测速结果应存在");

        assert_eq!(retrieved, result);
    }

    #[test]
    fn latency_save_replaces_existing_result() {
        let db = Database::memory().expect("创建内存数据库");
        seed_provider(&db, "test-provider", "claude");

        db.save_provider_latency_result(&latency_result("test-provider", 100, 1_000))
            .expect("保存旧测速结果");
        db.save_provider_latency_result(&latency_result("test-provider", 150, 2_000))
            .expect("保存新测速结果");

        let retrieved = db
            .get_provider_latency_result("test-provider", "claude")
            .expect("查询测速结果")
            .expect("测速结果应存在");

        assert_eq!(retrieved.latency_ms, Some(150));
        assert_eq!(retrieved.tested_at, 2_000);
    }

    #[test]
    fn latency_list_orders_newest_first() {
        let db = Database::memory().expect("创建内存数据库");
        seed_provider(&db, "provider1", "claude");
        seed_provider(&db, "provider2", "claude");

        db.save_provider_latency_result(&latency_result("provider1", 100, 1_000))
            .expect("保存 provider1 测速结果");
        db.save_provider_latency_result(&latency_result("provider2", 200, 2_000))
            .expect("保存 provider2 测速结果");

        let results = db
            .get_all_provider_latency_results("claude")
            .expect("查询所有测速结果");

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].provider_id, "provider2");
        assert_eq!(results[1].provider_id, "provider1");
    }

    #[test]
    fn latency_delete_removes_single_provider_result() {
        let db = Database::memory().expect("创建内存数据库");
        seed_provider(&db, "test-provider", "claude");

        db.save_provider_latency_result(&latency_result("test-provider", 123, 1_000))
            .expect("保存测速结果");
        db.delete_provider_latency_result("test-provider", "claude")
            .expect("删除测速结果");

        let retrieved = db
            .get_provider_latency_result("test-provider", "claude")
            .expect("查询测速结果");
        assert!(retrieved.is_none());
    }

    #[test]
    fn latency_cleanup_removes_old_results() {
        let db = Database::memory().expect("创建内存数据库");
        seed_provider(&db, "old-provider", "claude");
        seed_provider(&db, "new-provider", "claude");

        db.save_provider_latency_result(&latency_result("old-provider", 100, 1_000))
            .expect("保存旧测速结果");
        db.save_provider_latency_result(&latency_result("new-provider", 200, 3_000))
            .expect("保存新测速结果");

        let deleted = db
            .cleanup_old_latency_results(2_000)
            .expect("清理旧测速结果");
        assert_eq!(deleted, 1);

        let results = db
            .get_all_provider_latency_results("claude")
            .expect("查询所有测速结果");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].provider_id, "new-provider");
    }
}
