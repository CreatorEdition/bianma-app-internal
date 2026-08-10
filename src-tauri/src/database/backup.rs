//! 数据库备份和恢复
//!
//! 提供 SQL 导出/导入和二进制快照备份功能。

use super::{
    backup_scope::{local_only_relation, local_only_relations},
    is_routing_v2_table, lock_conn, Database, ROUTING_V2_TABLE_PREFIX,
};
use crate::config::get_app_config_dir;
use crate::error::AppError;
use chrono::{Local, Utc};
use rusqlite::backup::Backup;
use rusqlite::types::ValueRef;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::NamedTempFile;

const LEGACY_SQL_EXPORT_HEADER: &str = "-- CC Switch SQLite 导出";
const BRANDED_SQL_EXPORT_HEADER: &str = "-- bianma.ai SQLite 导出";
const SNAPSHOT_COPY_MAX_ATTEMPTS: usize = 3;
const SNAPSHOT_COPY_RETRY_DELAY: Duration = Duration::from_millis(25);

/// Tables whose data rows are skipped when exporting for WebDAV sync.
const SYNC_SKIP_TABLES: &[&str] = &[
    "proxy_request_logs",
    "stream_check_logs",
    "provider_health",
    "proxy_live_backup",
    "usage_daily_rollups",
];

/// Tables whose local data is preserved (restored from local snapshot) during WebDAV import.
/// Excludes ephemeral tables like provider_health that can safely rebuild at runtime.
const SYNC_PRESERVE_TABLES: &[&str] = &[
    "proxy_request_logs",
    "stream_check_logs",
    "proxy_live_backup",
    "usage_daily_rollups",
];

/// A database backup entry for the UI
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub filename: String,
    pub size_bytes: u64,
    pub created_at: String, // ISO 8601
}

impl Database {
    /// 导出为 SQLite 兼容的便携 SQL 文本。
    ///
    /// `routing_v2_*` 属于设备本地控制面，不会进入任何 SQL 导出。
    pub fn export_sql_string(&self) -> Result<String, AppError> {
        let snapshot = self.snapshot_to_memory()?;
        Self::validate_local_only_registry(&snapshot)?;
        Self::dump_sql(&snapshot, &[])
    }

    /// Export SQL for sync (WebDAV), skipping local-only tables' data
    pub fn export_sql_string_for_sync(&self) -> Result<String, AppError> {
        let snapshot = self.snapshot_to_memory()?;
        Self::validate_local_only_registry(&snapshot)?;
        Self::dump_sql(&snapshot, SYNC_SKIP_TABLES)
    }

    /// 导出为 SQLite 兼容的 SQL 文本
    pub fn export_sql(&self, target_path: &Path) -> Result<(), AppError> {
        let dump = self.export_sql_string()?;

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(parent, e))?;
        }

        crate::config::atomic_write(target_path, dump.as_bytes())
    }

    /// 从 SQL 文件导入，返回生成的备份 ID（若无备份则为空字符串）
    pub fn import_sql(&self, source_path: &Path) -> Result<String, AppError> {
        if !source_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "SQL 文件不存在: {}",
                source_path.display()
            )));
        }

        let sql_raw = fs::read_to_string(source_path).map_err(|e| AppError::io(source_path, e))?;
        let sql_content = sql_raw.trim_start_matches('\u{feff}');
        self.import_sql_string(sql_content)
    }

    /// 从 SQL 字符串导入，返回生成的备份 ID（若无备份则为空字符串）
    pub fn import_sql_string(&self, sql_raw: &str) -> Result<String, AppError> {
        self.import_sql_string_inner(sql_raw, &[], true)
    }

    /// Import SQL generated for sync, then restore local-only tables from the
    /// current device snapshot before replacing the main database.
    pub(crate) fn import_sql_string_for_sync(&self, sql_raw: &str) -> Result<String, AppError> {
        self.import_sql_string_inner(sql_raw, SYNC_PRESERVE_TABLES, true)
    }

    fn import_sql_string_inner(
        &self,
        sql_raw: &str,
        preserve_tables: &[&str],
        preserve_routing_v2: bool,
    ) -> Result<String, AppError> {
        let sql_content = sql_raw.trim_start_matches('\u{feff}');
        Self::validate_branded_or_legacy_sql_export(sql_content)?;
        Self::reject_routing_v2_namespace(sql_content)?;

        // 导入前备份现有数据库
        let backup_path = self.backup_database_file()?;

        let local_snapshot = if preserve_tables.is_empty() && !preserve_routing_v2 {
            None
        } else {
            Some(self.snapshot_to_memory()?)
        };
        let local_tables = match local_snapshot.as_ref() {
            Some(snapshot) => Self::collect_local_tables_to_preserve(
                snapshot,
                preserve_tables,
                preserve_routing_v2,
            )?,
            None => Vec::new(),
        };

        // 在临时数据库执行导入，确保失败不会污染主库
        let temp_file = NamedTempFile::new().map_err(|e| AppError::IoContext {
            context: "创建临时数据库文件失败".to_string(),
            source: e,
        })?;
        let temp_path = temp_file.path().to_path_buf();
        let temp_conn =
            Connection::open(&temp_path).map_err(|e| AppError::Database(e.to_string()))?;

        temp_conn
            .execute_batch(sql_content)
            .map_err(|e| AppError::Database(format!("执行 SQL 导入失败: {e}")))?;

        // 补齐缺失表/索引并进行基础校验
        Self::create_tables_on_conn(&temp_conn)?;
        Self::apply_schema_migrations_on_conn(&temp_conn)?;
        Self::validate_basic_state(&temp_conn)?;
        if let Some(local_snapshot) = local_snapshot.as_ref() {
            Self::restore_tables(local_snapshot, &temp_conn, &local_tables)?;
        }

        // 使用 Backup 将临时库原子写回主库
        {
            let mut main_conn = lock_conn!(self.conn);
            let backup = Backup::new(&temp_conn, &mut main_conn)
                .map_err(|e| AppError::Database(e.to_string()))?;
            backup
                .step(-1)
                .map_err(|e| AppError::Database(e.to_string()))?;
        }

        let backup_id = backup_path
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_default();

        Ok(backup_id)
    }

    /// 创建内存快照以避免长时间持有数据库锁
    pub(crate) fn snapshot_to_memory(&self) -> Result<Connection, AppError> {
        let conn = lock_conn!(self.conn);
        let mut snapshot =
            Connection::open_in_memory().map_err(|e| AppError::Database(e.to_string()))?;

        {
            let backup =
                Backup::new(&conn, &mut snapshot).map_err(|e| AppError::Database(e.to_string()))?;
            backup
                .step(-1)
                .map_err(|e| AppError::Database(e.to_string()))?;
        }

        Ok(snapshot)
    }

    fn validate_branded_or_legacy_sql_export(sql: &str) -> Result<(), AppError> {
        let trimmed = sql.trim_start();
        if trimmed.starts_with(BRANDED_SQL_EXPORT_HEADER)
            || trimmed.starts_with(LEGACY_SQL_EXPORT_HEADER)
        {
            return Ok(());
        }

        Err(AppError::localized(
            "backup.sql.invalid_format",
            "仅支持导入由 bianma.ai 导出的 SQL 备份文件（兼容历史 CC Switch 导出格式）。",
            "Only SQL backups exported by bianma.ai are supported (legacy CC Switch exports remain compatible).",
        ))
    }

    /// 拒绝任何试图经旧 SQL 通道写入设备本地 routing v2 命名空间的导入。
    ///
    /// 这里故意按文本保守拒绝：SQL 导入不应出现该前缀，宁可拒绝带有该保留词的
    /// 旧备份，也不能让注释、字符串或大小写技巧绕过本机控制面的隔离边界。
    fn reject_routing_v2_namespace(sql: &str) -> Result<(), AppError> {
        if sql
            .as_bytes()
            .windows(ROUTING_V2_TABLE_PREFIX.len())
            .any(|window| window.eq_ignore_ascii_case(ROUTING_V2_TABLE_PREFIX.as_bytes()))
        {
            return Err(AppError::InvalidInput(
                "导入的 SQL 包含设备本地 routing v2 命名空间，已拒绝覆盖。".to_string(),
            ));
        }
        Ok(())
    }

    /// 收集同步或便携导入后应从当前设备恢复的本地表。
    fn collect_local_tables_to_preserve(
        source_conn: &Connection,
        preserve_tables: &[&str],
        preserve_routing_v2: bool,
    ) -> Result<Vec<String>, AppError> {
        Self::validate_local_only_registry(source_conn)?;

        let mut tables = Vec::new();
        for table in preserve_tables {
            if Self::table_exists(source_conn, table)? {
                tables.push((*table).to_string());
            }
        }

        if !preserve_routing_v2 {
            return Ok(tables);
        }

        let mut stmt = source_conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .map_err(|e| AppError::Database(format!("读取本机表名失败: {e}")))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| AppError::Database(format!("读取本机 routing v2 表失败: {e}")))?;

        let mut routing_v2_tables = Vec::new();
        for row in rows {
            let table = row.map_err(|e| AppError::Database(e.to_string()))?;
            if is_routing_v2_table(&table) {
                let relation = local_only_relation(&table)
                    .ok_or_else(|| Self::unregistered_local_only_table_error())?;
                routing_v2_tables.push((relation.restore_rank, table));
            }
        }
        routing_v2_tables.sort_by_key(|(restore_rank, table)| (*restore_rank, table.clone()));
        tables.extend(routing_v2_tables.into_iter().map(|(_, table)| table));
        Ok(tables)
    }

    /// 校验当前数据库没有未登记的 routing v2 表。
    ///
    /// 新增设备本地表时，必须先在静态 registry 中声明其恢复顺序；不能依赖名称
    /// 前缀的静默兜底，否则跨设备恢复的外键与隔离边界无法证明。
    fn validate_local_only_registry(conn: &Connection) -> Result<(), AppError> {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .map_err(|error| AppError::Database(format!("读取本机表名失败: {error}")))?;
        let tables = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| AppError::Database(format!("读取本机表名失败: {error}")))?;

        for table in tables {
            let table = table.map_err(|error| AppError::Database(error.to_string()))?;
            if is_routing_v2_table(&table) && local_only_relation(&table).is_none() {
                return Err(Self::unregistered_local_only_table_error());
            }
        }
        Ok(())
    }

    fn unregistered_local_only_table_error() -> AppError {
        AppError::localized(
            "backup.local_only_table_unregistered",
            "发现未登记的设备本地 routing v2 表，已拒绝备份或恢复。",
            "An unregistered device-local routing v2 table was found; backup or restore was rejected.",
        )
    }

    /// 从临时快照剥离所有已登记的设备本地表及其关联对象。
    ///
    /// 该操作只作用于内存恢复副本或待落盘的备份副本，绝不直接修改当前设备的主库。
    fn strip_local_only_relations(conn: &Connection) -> Result<(), AppError> {
        Self::validate_local_only_registry(conn)?;

        let mut stmt = conn
            .prepare(
                "SELECT type, name, tbl_name, sql FROM sqlite_master
                 WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
                 ORDER BY type, name",
            )
            .map_err(|error| AppError::Database(format!("读取本机对象失败: {error}")))?;
        let objects = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|error| AppError::Database(format!("读取本机对象失败: {error}")))?;

        let mut dependent_objects = Vec::new();
        for object in objects {
            let (object_type, name, table_name, sql) =
                object.map_err(|error| AppError::Database(error.to_string()))?;
            if object_type != "table"
                && Self::object_uses_local_only_namespace(&name, &table_name, &sql)
            {
                let drop_order = match object_type.as_str() {
                    "view" => 0,
                    "trigger" => 1,
                    "index" => 2,
                    _ => continue,
                };
                dependent_objects.push((drop_order, object_type, name));
            }
        }

        dependent_objects.sort_by_key(|(drop_order, _, name)| (*drop_order, name.clone()));
        for (_, object_type, name) in dependent_objects {
            let quoted_name = Self::quote_sqlite_identifier(&name);
            conn.execute_batch(&format!("DROP {object_type} IF EXISTS {quoted_name};"))
                .map_err(|error| AppError::Database(format!("剥离设备本地对象失败: {error}")))?;
        }

        let mut relations = local_only_relations().to_vec();
        relations.sort_by_key(|relation| relation.restore_rank);
        for relation in relations.into_iter().rev() {
            let quoted_table = Self::quote_sqlite_identifier(relation.table);
            conn.execute_batch(&format!("DROP TABLE IF EXISTS {quoted_table};"))
                .map_err(|error| AppError::Database(format!("剥离设备本地表失败: {error}")))?;
        }

        Self::ensure_local_only_namespace_stripped(conn)
    }

    fn ensure_local_only_namespace_stripped(conn: &Connection) -> Result<(), AppError> {
        let mut stmt = conn
            .prepare(
                "SELECT name, tbl_name, sql FROM sqlite_master
                 WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
                 ORDER BY name",
            )
            .map_err(|error| AppError::Database(format!("读取本机对象失败: {error}")))?;
        let objects = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| AppError::Database(format!("读取本机对象失败: {error}")))?;

        for object in objects {
            let (name, table_name, sql) =
                object.map_err(|error| AppError::Database(error.to_string()))?;
            if Self::object_uses_local_only_namespace(&name, &table_name, &sql) {
                return Err(AppError::localized(
                    "backup.local_only_namespace_not_stripped",
                    "设备本地 routing v2 命名空间未能从备份副本完全剥离。",
                    "The device-local routing v2 namespace could not be fully removed from the backup copy.",
                ));
            }
        }
        Ok(())
    }

    fn object_uses_local_only_namespace(name: &str, table_name: &str, sql: &str) -> bool {
        is_routing_v2_table(name)
            || is_routing_v2_table(table_name)
            || sql
                .as_bytes()
                .windows(ROUTING_V2_TABLE_PREFIX.len())
                .any(|window| window.eq_ignore_ascii_case(ROUTING_V2_TABLE_PREFIX.as_bytes()))
    }

    fn quote_sqlite_identifier(identifier: &str) -> String {
        format!("\"{}\"", identifier.replace('"', "\"\""))
    }

    fn restore_tables(
        source_conn: &Connection,
        target_conn: &Connection,
        tables: &[String],
    ) -> Result<(), AppError> {
        let mut existing_tables = Vec::new();
        for table in tables {
            if Self::table_exists(source_conn, table)? && Self::table_exists(target_conn, table)? {
                existing_tables.push(table);
            }
        }

        for table in existing_tables.iter().rev() {
            target_conn
                .execute(&format!("DELETE FROM \"{table}\""), [])
                .map_err(|e| AppError::Database(format!("清空表 {table} 失败: {e}")))?;
        }

        for table in existing_tables {
            let columns = Self::get_table_columns(source_conn, table)?;
            if columns.is_empty() {
                continue;
            }

            let placeholders = (1..=columns.len())
                .map(|idx| format!("?{idx}"))
                .collect::<Vec<_>>()
                .join(", ");
            let cols = columns
                .iter()
                .map(|column| format!("\"{column}\""))
                .collect::<Vec<_>>()
                .join(", ");
            let insert_sql = format!("INSERT INTO \"{table}\" ({cols}) VALUES ({placeholders})");

            let mut stmt = source_conn
                .prepare(&format!("SELECT * FROM \"{table}\""))
                .map_err(|e| AppError::Database(format!("读取表 {table} 失败: {e}")))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| AppError::Database(format!("查询表 {table} 数据失败: {e}")))?;

            while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                let mut values = Vec::with_capacity(columns.len());
                for idx in 0..columns.len() {
                    values.push(
                        row.get::<_, rusqlite::types::Value>(idx)
                            .map_err(|e| AppError::Database(e.to_string()))?,
                    );
                }

                target_conn
                    .execute(&insert_sql, rusqlite::params_from_iter(values.iter()))
                    .map_err(|e| AppError::Database(format!("恢复表 {table} 数据失败: {e}")))?;
            }
        }

        Ok(())
    }

    /// Periodic backup: create a new backup if the latest one is older than the configured interval
    pub(crate) fn periodic_backup_if_needed(&self) -> Result<(), AppError> {
        let interval_hours = crate::settings::effective_backup_interval_hours();
        if interval_hours > 0 {
            let backup_dir = get_app_config_dir().join("backups");
            if !backup_dir.exists() {
                self.backup_database_file()?;
            } else {
                let latest = fs::read_dir(&backup_dir).ok().and_then(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().map(|ext| ext == "db").unwrap_or(false))
                        .filter_map(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
                        .max()
                });

                let interval_secs = u64::from(interval_hours) * 3600;
                let needs_backup = match latest {
                    None => true,
                    Some(last_modified) => {
                        last_modified.elapsed().unwrap_or_default()
                            > std::time::Duration::from_secs(interval_secs)
                    }
                };

                if needs_backup {
                    log::info!(
                        "Periodic backup: latest backup is older than {interval_hours} hours, creating new backup"
                    );
                    self.backup_database_file()?;
                }
            }
        }

        // Periodic maintenance is always enabled, regardless of auto-backup settings.
        let mut reclaimed_rows = 0u64;
        match self.cleanup_old_stream_check_logs(7) {
            Ok(deleted) => {
                reclaimed_rows += deleted;
            }
            Err(e) => {
                log::warn!("Periodic stream_check_logs cleanup failed: {e}");
            }
        }
        match self.rollup_and_prune(30) {
            Ok(deleted) => {
                reclaimed_rows += deleted;
            }
            Err(e) => {
                log::warn!("Periodic rollup_and_prune failed: {e}");
            }
        }
        if reclaimed_rows > 0 {
            let conn = lock_conn!(self.conn);
            if let Err(e) = conn.execute_batch("PRAGMA incremental_vacuum;") {
                log::warn!("Periodic incremental vacuum failed: {e}");
            }
        }

        Ok(())
    }

    /// 生成一致性快照备份，返回备份文件路径（不存在主库时返回 None）
    pub(crate) fn backup_database_file(&self) -> Result<Option<PathBuf>, AppError> {
        let db_path = get_app_config_dir().join("cc-switch.db");
        if !db_path.exists() {
            return Ok(None);
        }

        let snapshot = self.snapshot_for_portable_backup()?;

        let backup_dir = db_path
            .parent()
            .ok_or_else(|| AppError::Config("无效的数据库路径".to_string()))?
            .join("backups");

        fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

        let base_id = format!("db_backup_{}", Local::now().format("%Y%m%d_%H%M%S"));
        let mut backup_id = base_id.clone();
        let mut backup_path = backup_dir.join(format!("{backup_id}.db"));
        let mut counter = 1;
        while backup_path.exists() {
            backup_id = format!("{base_id}_{counter}");
            backup_path = backup_dir.join(format!("{backup_id}.db"));
            counter += 1;
        }

        let mut dest_conn =
            Connection::open(&backup_path).map_err(|e| AppError::Database(e.to_string()))?;
        Self::copy_snapshot(&snapshot, &mut dest_conn)?;

        Self::cleanup_db_backups(&backup_dir)?;
        Ok(Some(backup_path))
    }

    /// 将来源快照完整复制到目标连接；调用方负责先完成本机表的隔离或恢复准备。
    fn copy_snapshot(
        source_conn: &Connection,
        target_conn: &mut Connection,
    ) -> Result<(), AppError> {
        let backup = Backup::new(source_conn, target_conn)
            .map_err(|error| AppError::Database(error.to_string()))?;
        for attempt in 0..SNAPSHOT_COPY_MAX_ATTEMPTS {
            match backup
                .step(-1)
                .map_err(|error| AppError::Database(error.to_string()))?
            {
                rusqlite::backup::StepResult::Done => return Ok(()),
                _ if attempt + 1 < SNAPSHOT_COPY_MAX_ATTEMPTS => {
                    // 只在低频备份/恢复路径做有界等待，避免 Busy/Locked 被误判为成功。
                    std::thread::sleep(SNAPSHOT_COPY_RETRY_DELAY);
                }
                _ => {
                    return Err(AppError::Database(
                        "数据库快照复制未能在限定重试次数内完成".to_string(),
                    ));
                }
            }
        }
        unreachable!("有界快照复制循环必须在成功或失败时返回")
    }

    /// 创建可写入二进制备份的临时快照，并剥离设备本地表。
    fn snapshot_for_portable_backup(&self) -> Result<Connection, AppError> {
        let snapshot = self.snapshot_to_memory()?;
        Self::strip_local_only_relations(&snapshot)?;
        Ok(snapshot)
    }

    /// 清理旧的数据库备份，保留最新的 N 个
    fn cleanup_db_backups(dir: &Path) -> Result<(), AppError> {
        let retain = crate::settings::effective_backup_retain_count();
        let entries = match fs::read_dir(dir) {
            Ok(iter) => iter
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|ext| ext == "db")
                        .unwrap_or(false)
                })
                .collect::<Vec<_>>(),
            Err(_) => return Ok(()),
        };

        if entries.len() <= retain {
            return Ok(());
        }

        let remove_count = entries.len().saturating_sub(retain);
        let mut sorted = entries;
        sorted.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());

        for entry in sorted.into_iter().take(remove_count) {
            if let Err(err) = fs::remove_file(entry.path()) {
                log::warn!("删除旧数据库备份失败 {}: {}", entry.path().display(), err);
            }
        }
        Ok(())
    }

    /// 基础状态校验
    fn validate_basic_state(conn: &Connection) -> Result<(), AppError> {
        let provider_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))
            .map_err(|e| AppError::Database(e.to_string()))?;
        let mcp_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM mcp_servers", [], |row| row.get(0))
            .map_err(|e| AppError::Database(e.to_string()))?;

        if provider_count == 0 && mcp_count == 0 {
            return Err(AppError::Config(
                "导入的 SQL 未包含有效的供应商或 MCP 数据".to_string(),
            ));
        }
        Ok(())
    }

    /// 导出数据库为 SQL 文本
    fn dump_sql(conn: &Connection, skip_tables: &[&str]) -> Result<String, AppError> {
        Self::validate_local_only_registry(conn)?;

        let mut output = String::new();
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let user_version: i64 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .unwrap_or(0);

        output.push_str(&format!(
            "{BRANDED_SQL_EXPORT_HEADER}\n-- 生成时间: {timestamp}\n-- user_version: {user_version}\n"
        ));
        output.push_str("PRAGMA foreign_keys=OFF;\n");
        output.push_str(&format!("PRAGMA user_version={user_version};\n"));
        output.push_str("BEGIN TRANSACTION;\n");

        // 导出 schema
        let mut stmt = conn
            .prepare(
                "SELECT type, name, tbl_name, sql
                 FROM sqlite_master
                 WHERE sql NOT NULL AND type IN ('table','index','trigger','view')
                 ORDER BY type='table' DESC, name",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut tables = Vec::new();
        let mut rows = stmt
            .query([])
            .map_err(|e| AppError::Database(e.to_string()))?;
        while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
            let obj_type: String = row.get(0).map_err(|e| AppError::Database(e.to_string()))?;
            let name: String = row.get(1).map_err(|e| AppError::Database(e.to_string()))?;
            let table_name: String = row.get(2).map_err(|e| AppError::Database(e.to_string()))?;
            let sql: String = row.get(3).map_err(|e| AppError::Database(e.to_string()))?;

            // 跳过 SQLite 内部对象（如 sqlite_sequence）
            if name.starts_with("sqlite_") {
                continue;
            }

            if Self::object_uses_local_only_namespace(&name, &table_name, &sql) {
                continue;
            }

            output.push_str(&sql);
            output.push_str(";\n");

            if obj_type == "table" && !name.starts_with("sqlite_") {
                tables.push(name);
            }
        }

        // 导出数据
        for table in tables {
            if local_only_relation(&table).is_some() || skip_tables.iter().any(|t| *t == table) {
                continue;
            }
            let columns = Self::get_table_columns(conn, &table)?;
            if columns.is_empty() {
                continue;
            }

            let mut stmt = conn
                .prepare(&format!("SELECT * FROM \"{table}\""))
                .map_err(|e| AppError::Database(e.to_string()))?;
            let mut rows = stmt
                .query([])
                .map_err(|e| AppError::Database(e.to_string()))?;

            while let Some(row) = rows.next().map_err(|e| AppError::Database(e.to_string()))? {
                let mut values = Vec::with_capacity(columns.len());
                for idx in 0..columns.len() {
                    let value = row
                        .get_ref(idx)
                        .map_err(|e| AppError::Database(e.to_string()))?;
                    values.push(Self::format_sql_value(value)?);
                }

                let cols = columns
                    .iter()
                    .map(|c| format!("\"{c}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                output.push_str(&format!(
                    "INSERT INTO \"{table}\" ({cols}) VALUES ({});\n",
                    values.join(", ")
                ));
            }
        }

        output.push_str("COMMIT;\nPRAGMA foreign_keys=ON;\n");
        Ok(output)
    }

    /// 获取表的列名列表
    fn get_table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, AppError> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .map_err(|e| AppError::Database(e.to_string()))?;
        let iter = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut columns = Vec::new();
        for col in iter {
            columns.push(col.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(columns)
    }

    /// 格式化 SQL 值
    fn format_sql_value(value: ValueRef<'_>) -> Result<String, AppError> {
        match value {
            ValueRef::Null => Ok("NULL".to_string()),
            ValueRef::Integer(i) => Ok(i.to_string()),
            ValueRef::Real(f) => Ok(f.to_string()),
            ValueRef::Text(t) => {
                let text = std::str::from_utf8(t)
                    .map_err(|e| AppError::Database(format!("文本字段不是有效的 UTF-8: {e}")))?;
                let escaped = text.replace('\'', "''");
                Ok(format!("'{escaped}'"))
            }
            ValueRef::Blob(bytes) => {
                let mut s = String::from("X'");
                for b in bytes {
                    use std::fmt::Write;
                    let _ = write!(&mut s, "{b:02X}");
                }
                s.push('\'');
                Ok(s)
            }
        }
    }

    /// List all database backup files, sorted by creation time (newest first)
    pub fn list_backups() -> Result<Vec<BackupEntry>, AppError> {
        let backup_dir = get_app_config_dir().join("backups");
        if !backup_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries: Vec<BackupEntry> = fs::read_dir(&backup_dir)
            .map_err(|e| AppError::io(&backup_dir, e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|ext| ext == "db").unwrap_or(false))
            .filter_map(|e| {
                let metadata = e.metadata().ok()?;
                let filename = e.file_name().to_string_lossy().to_string();
                let size_bytes = metadata.len();
                let created_at = metadata
                    .modified()
                    .ok()
                    .map(|t| {
                        let dt: chrono::DateTime<Utc> = t.into();
                        dt.to_rfc3339()
                    })
                    .unwrap_or_default();
                Some(BackupEntry {
                    filename,
                    size_bytes,
                    created_at,
                })
            })
            .collect();

        // Sort by created_at descending (newest first)
        entries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(entries)
    }

    /// Restore database from a backup file. Returns the safety backup ID.
    pub fn restore_from_backup(&self, filename: &str) -> Result<String, AppError> {
        // Security: validate filename to prevent path traversal
        if filename.contains("..")
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.ends_with(".db")
        {
            return Err(AppError::InvalidInput(
                "Invalid backup filename".to_string(),
            ));
        }

        let backup_dir = get_app_config_dir().join("backups");
        let backup_path = backup_dir.join(filename);

        if !backup_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Backup file not found: {filename}"
            )));
        }

        // Step 1: Create safety backup of current database
        let safety_backup = self.backup_database_file()?;
        let safety_id = safety_backup
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_default();

        // Step 2: 在临时库中恢复外部快照，并回填当前设备的本机表。
        let source_conn =
            Connection::open(&backup_path).map_err(|e| AppError::Database(e.to_string()))?;
        self.restore_from_connection_preserving_local_only(&source_conn)?;

        log::info!("Database restored from backup: {filename}, safety backup: {safety_id}");
        Ok(safety_id)
    }

    /// 将外部快照替换为当前数据库，同时严格保留当前设备的本机表。
    ///
    /// 外部 `.db` 可能来自另一设备或历史版本，因此它只能贡献可移植数据；任何
    /// 已登记的 `routing_v2_*` 表均会在临时库剥离，并按登记顺序从当前设备快照
    /// 回填。准备失败时主库保持不变。
    fn restore_from_connection_preserving_local_only(
        &self,
        source_conn: &Connection,
    ) -> Result<(), AppError> {
        Self::validate_local_only_registry(source_conn)?;

        let local_snapshot = self.snapshot_to_memory()?;
        let local_tables = Self::collect_local_tables_to_preserve(&local_snapshot, &[], true)?;

        let mut prepared_restore =
            Connection::open_in_memory().map_err(|error| AppError::Database(error.to_string()))?;
        Self::copy_snapshot(source_conn, &mut prepared_restore)?;
        Self::strip_local_only_relations(&prepared_restore)?;
        Self::create_tables_on_conn(&prepared_restore)?;
        Self::apply_schema_migrations_on_conn(&prepared_restore)?;
        Self::restore_tables(&local_snapshot, &prepared_restore, &local_tables)?;
        Self::ensure_model_pricing_seeded_on_conn(&prepared_restore)?;

        let mut main_conn = lock_conn!(self.conn);
        Self::copy_snapshot(&prepared_restore, &mut main_conn)
    }

    /// Rename a backup file. Returns the new filename.
    pub fn rename_backup(old_filename: &str, new_name: &str) -> Result<String, AppError> {
        // Validate old filename (path traversal + .db suffix)
        if old_filename.contains("..")
            || old_filename.contains('/')
            || old_filename.contains('\\')
            || !old_filename.ends_with(".db")
        {
            return Err(AppError::InvalidInput(
                "Invalid backup filename".to_string(),
            ));
        }

        // Clean new name
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(AppError::InvalidInput(
                "New name cannot be empty".to_string(),
            ));
        }

        // Length limit (without .db suffix)
        let name_part = trimmed.strip_suffix(".db").unwrap_or(trimmed);
        if name_part.len() > 100 {
            return Err(AppError::InvalidInput(
                "Name too long (max 100 characters)".to_string(),
            ));
        }

        // Prevent path traversal in new name
        if name_part.contains("..")
            || name_part.contains('/')
            || name_part.contains('\\')
            || name_part.contains('\0')
        {
            return Err(AppError::InvalidInput(
                "Invalid characters in new name".to_string(),
            ));
        }

        let new_filename = format!("{name_part}.db");

        let backup_dir = get_app_config_dir().join("backups");
        let old_path = backup_dir.join(old_filename);
        let new_path = backup_dir.join(&new_filename);

        if !old_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Backup file not found: {old_filename}"
            )));
        }

        if new_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "A backup named '{new_filename}' already exists"
            )));
        }

        fs::rename(&old_path, &new_path).map_err(|e| AppError::io(&old_path, e))?;
        log::info!("Renamed backup: {old_filename} -> {new_filename}");
        Ok(new_filename)
    }

    /// Delete a backup file permanently.
    pub fn delete_backup(filename: &str) -> Result<(), AppError> {
        // Validate filename (path traversal + .db suffix)
        if filename.contains("..")
            || filename.contains('/')
            || filename.contains('\\')
            || !filename.ends_with(".db")
        {
            return Err(AppError::InvalidInput(
                "Invalid backup filename".to_string(),
            ));
        }

        let backup_path = get_app_config_dir().join("backups").join(filename);
        if !backup_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "Backup file not found: {filename}"
            )));
        }

        fs::remove_file(&backup_path).map_err(|e| AppError::io(&backup_path, e))?;
        log::info!("Deleted backup: {filename}");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Database, BRANDED_SQL_EXPORT_HEADER, LEGACY_SQL_EXPORT_HEADER};
    use crate::error::AppError;
    use crate::settings::{update_settings, AppSettings};
    use serial_test::serial;
    use std::ffi::OsString;
    use std::path::PathBuf;

    struct TestHomeGuard {
        previous: Option<OsString>,
        previous_home: Option<OsString>,
        test_home: PathBuf,
    }

    impl Drop for TestHomeGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
            match self.previous_home.take() {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            let _ = std::fs::remove_dir_all(&self.test_home);
        }
    }

    /// 为会触发安全备份的测试隔离应用目录，避免触碰用户真实备份并消除全局环境竞争。
    fn isolated_test_home(name: &str) -> TestHomeGuard {
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        let previous_home = std::env::var_os("HOME");
        let test_home =
            std::env::temp_dir().join(format!("bianma-backup-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&test_home);
        std::fs::create_dir_all(&test_home).expect("创建隔离测试目录");
        std::env::set_var("CC_SWITCH_TEST_HOME", &test_home);
        // Windows 下存在旧 HOME 路径回退；同步覆盖它才能阻止测试触碰真实用户备份。
        std::env::set_var("HOME", &test_home);
        TestHomeGuard {
            previous,
            previous_home,
            test_home,
        }
    }

    fn assert_localized_error_key(error: AppError, expected_key: &str) {
        match error {
            AppError::Localized { key, .. } => assert_eq!(key, expected_key),
            other => panic!("预期本机隔离错误码 {expected_key}，实际为 {other}"),
        }
    }

    #[test]
    fn sql_export_uses_branded_header() -> Result<(), AppError> {
        let db = Database::memory()?;
        let sql = db.export_sql_string()?;

        assert!(
            sql.starts_with(BRANDED_SQL_EXPORT_HEADER),
            "SQL 导出应使用 bianma.ai 品牌头"
        );

        Ok(())
    }

    #[test]
    fn portable_sql_export_excludes_routing_v2_schema_and_rows() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO routing_v2_sites (site_id, display_name) VALUES ('local-site', 'Local Site')",
                [],
            )?;
            conn.execute(
                "CREATE INDEX idx_local_routing_site_name ON routing_v2_sites(display_name)",
                [],
            )?;
        }

        let sql = db.export_sql_string()?;
        assert!(
            !sql.to_ascii_lowercase().contains("routing_v2_"),
            "便携 SQL 导出不得包含 routing v2 的 DDL、索引或行数据"
        );
        assert!(
            !sql.contains("idx_local_routing_site_name"),
            "依附 routing v2 表的索引也不得进入导出"
        );
        Ok(())
    }

    #[test]
    fn backup_registry_rejects_unregistered_routing_v2_table() -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute("CREATE TABLE routing_v2_unregistered (id TEXT)", [])?;
        }

        let error = db
            .export_sql_string()
            .expect_err("未登记的设备本地表不能被静默导出");
        assert_localized_error_key(error, "backup.local_only_table_unregistered");
        Ok(())
    }

    #[test]
    fn binary_backup_snapshot_excludes_device_local_routing_v2_schema_and_rows(
    ) -> Result<(), AppError> {
        let db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('portable-provider', 'claude', 'Portable Provider', '{}', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO routing_v2_sites (site_id, display_name)
                 VALUES ('local-site', 'Local Site')",
                [],
            )?;
            conn.execute(
                "CREATE INDEX idx_local_routing_site_name ON routing_v2_sites(display_name)",
                [],
            )?;
        }

        let snapshot = db.snapshot_for_portable_backup()?;
        let mut binary_backup = rusqlite::Connection::open_in_memory()?;
        Database::copy_snapshot(&snapshot, &mut binary_backup)?;

        assert!(
            !Database::table_exists(&binary_backup, "routing_v2_sites")?,
            "二进制备份不得保留设备本地 routing v2 表"
        );
        let local_index_count: i64 = binary_backup.query_row(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_local_routing_site_name'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            local_index_count, 0,
            "二进制备份不得保留依附设备本地表的索引"
        );
        let provider_count: i64 = binary_backup.query_row(
            "SELECT COUNT(*) FROM providers WHERE id = 'portable-provider'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(provider_count, 1, "可移植数据必须保留在二进制备份中");
        Ok(())
    }

    #[test]
    #[serial]
    fn binary_backup_file_excludes_device_local_routing_v2_schema_and_rows() -> Result<(), AppError>
    {
        let _test_home = isolated_test_home("binary-backup-file");
        let db = Database::init()?;
        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('portable-provider', 'claude', 'Portable Provider', '{}', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO routing_v2_sites (site_id, display_name)
                 VALUES ('local-site', 'Local Site')",
                [],
            )?;
        }

        let backup_path = db
            .backup_database_file()?
            .expect("文件数据库必须生成二进制备份");
        let backup_conn = rusqlite::Connection::open(backup_path)?;
        assert!(
            !Database::table_exists(&backup_conn, "routing_v2_sites")?,
            "二进制备份文件不得保留设备本地 routing v2 表"
        );
        let provider_count: i64 = backup_conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE id = 'portable-provider'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(provider_count, 1, "二进制备份文件必须保留可移植数据");
        Ok(())
    }

    #[test]
    fn binary_restore_preserves_current_device_routing_v2_rows() -> Result<(), AppError> {
        let source_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('remote-provider', 'claude', 'Remote Provider', '{}', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO routing_v2_sites (site_id, display_name)
                 VALUES ('remote-site', 'Remote Site')",
                [],
            )?;
        }

        let local_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(local_db.conn);
            conn.execute_batch(
                "INSERT INTO routing_v2_sites (site_id, display_name)
                 VALUES ('local-site', 'Local Site');
                 INSERT INTO routing_v2_endpoints (
                     endpoint_id, site_id, display_base_url, canonical_origin, base_path, protocol_family
                 ) VALUES (
                     'local-endpoint', 'local-site', 'https://local.example/v1',
                     'https://local.example', '/v1', 'anthropic'
                 );
                 INSERT INTO routing_v2_model_deployments (
                     deployment_id, site_id, endpoint_id, upstream_model_id, adapter_contract_revision
                 ) VALUES (
                     'local-deployment', 'local-site', 'local-endpoint', 'local-model', 1
                 );",
            )?;
        }

        let source_conn = crate::database::lock_conn!(source_db.conn);
        local_db.restore_from_connection_preserving_local_only(&source_conn)?;
        drop(source_conn);

        let conn = crate::database::lock_conn!(local_db.conn);
        let remote_provider_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE id = 'remote-provider'",
            [],
            |row| row.get(0),
        )?;
        let local_site_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM routing_v2_sites WHERE site_id = 'local-site'",
            [],
            |row| row.get(0),
        )?;
        let remote_site_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM routing_v2_sites WHERE site_id = 'remote-site'",
            [],
            |row| row.get(0),
        )?;
        let local_endpoint_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM routing_v2_endpoints WHERE endpoint_id = 'local-endpoint'",
            [],
            |row| row.get(0),
        )?;
        let local_deployment_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM routing_v2_model_deployments
             WHERE deployment_id = 'local-deployment'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(remote_provider_count, 1, "外部快照的可移植数据必须恢复");
        assert_eq!(
            local_site_count, 1,
            "恢复必须保留当前设备的本机 routing v2 行"
        );
        assert_eq!(
            remote_site_count, 0,
            "恢复不得采纳外部快照的本机 routing v2 行"
        );
        assert_eq!(
            local_endpoint_count, 1,
            "恢复必须按外键顺序回填当前设备 Endpoint"
        );
        assert_eq!(
            local_deployment_count, 1,
            "恢复必须按外键顺序回填当前设备 ModelDeployment"
        );
        Ok(())
    }

    #[test]
    fn binary_restore_rejects_unregistered_routing_v2_table_without_main_db_change(
    ) -> Result<(), AppError> {
        let source_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('remote-provider', 'claude', 'Remote Provider', '{}', '{}')",
                [],
            )?;
            conn.execute("CREATE TABLE routing_v2_unregistered (id TEXT)", [])?;
        }

        let local_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(local_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('local-provider', 'claude', 'Local Provider', '{}', '{}')",
                [],
            )?;
        }

        let source_conn = crate::database::lock_conn!(source_db.conn);
        let error = local_db
            .restore_from_connection_preserving_local_only(&source_conn)
            .expect_err("未登记的外部设备本地表必须拒绝恢复");
        drop(source_conn);
        assert_localized_error_key(error, "backup.local_only_table_unregistered");

        let conn = crate::database::lock_conn!(local_db.conn);
        let local_provider_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE id = 'local-provider'",
            [],
            |row| row.get(0),
        )?;
        let remote_provider_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM providers WHERE id = 'remote-provider'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(local_provider_count, 1, "拒绝恢复后主库必须保持原状");
        assert_eq!(remote_provider_count, 0, "拒绝恢复后外部数据不得写入主库");
        Ok(())
    }

    #[test]
    fn sql_import_rejects_routing_v2_namespace() -> Result<(), AppError> {
        let db = Database::memory()?;
        let sql = format!(
            "{BRANDED_SQL_EXPORT_HEADER}\nCREATE TABLE routing_v2_remote_injection (id INTEGER);"
        );

        let error = db
            .import_sql_string(&sql)
            .expect_err("旧 SQL 导入不得写入设备本地 routing v2 命名空间");
        assert!(error.to_string().contains("routing v2"));
        let conn = crate::database::lock_conn!(db.conn);
        assert!(
            !Database::table_exists(&conn, "routing_v2_remote_injection")?,
            "拒绝前必须保持主数据库未被导入污染"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn sql_export_import_accepts_legacy_header() -> Result<(), AppError> {
        let _test_home = isolated_test_home("legacy-sql-import");
        let source_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(source_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('legacy-provider', 'claude', 'Legacy Provider', '{}', '{}')",
                [],
            )?;
        }

        let branded_sql = source_db.export_sql_string()?;
        let legacy_sql =
            branded_sql.replacen(BRANDED_SQL_EXPORT_HEADER, LEGACY_SQL_EXPORT_HEADER, 1);

        let target_db = Database::memory()?;
        target_db.import_sql_string(&legacy_sql)?;

        let provider_count: i64 = {
            let conn = crate::database::lock_conn!(target_db.conn);
            conn.query_row(
                "SELECT COUNT(*) FROM providers WHERE id = 'legacy-provider'",
                [],
                |row| row.get(0),
            )?
        };
        assert_eq!(provider_count, 1, "历史 CC Switch 导出头应继续被导入接受");

        Ok(())
    }

    #[test]
    #[serial]
    fn sync_import_preserves_local_only_tables() -> Result<(), AppError> {
        let _test_home = isolated_test_home("sync-local-tables");
        let remote_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(remote_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('remote-provider', 'claude', 'Remote Provider', '{}', '{}')",
                [],
            )?;
        }
        let remote_sql = remote_db.export_sql_string_for_sync()?;

        let local_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(local_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('local-provider', 'claude', 'Local Provider', '{}', '{}')",
                [],
            )?;
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES ('req-1', 'local-provider', 'claude', 'claude-3', 100, 50, '0.01', 120, 200, 1000)",
                [],
            )?;
            conn.execute(
                "INSERT INTO usage_daily_rollups (
                    date, app_type, provider_id, model, request_count, success_count,
                    input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens,
                    total_cost_usd, avg_latency_ms
                ) VALUES ('2026-03-01', 'claude', 'local-provider', 'claude-3', 7, 7, 700, 350, 0, 0, '0.07', 120)",
                [],
            )?;
            conn.execute(
                "INSERT INTO stream_check_logs (
                    provider_id, provider_name, app_type, status, success, message,
                    response_time_ms, http_status, model_used, retry_count, tested_at
                ) VALUES ('local-provider', 'Local Provider', 'claude', 'operational', 1, 'ok', 42, 200, 'claude-3', 0, 1000)",
                [],
            )?;
        }

        local_db.import_sql_string_for_sync(&remote_sql)?;

        let remote_provider_exists: i64 = {
            let conn = crate::database::lock_conn!(local_db.conn);
            conn.query_row(
                "SELECT COUNT(*) FROM providers WHERE id = 'remote-provider' AND app_type = 'claude'",
                [],
                |row| row.get(0),
            )?
        };
        assert_eq!(
            remote_provider_exists, 1,
            "remote config should be imported"
        );

        let (request_logs, rollups, stream_logs): (i64, i64, i64) = {
            let conn = crate::database::lock_conn!(local_db.conn);
            let request_logs =
                conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
                    row.get(0)
                })?;
            let rollups =
                conn.query_row("SELECT COUNT(*) FROM usage_daily_rollups", [], |row| {
                    row.get(0)
                })?;
            let stream_logs =
                conn.query_row("SELECT COUNT(*) FROM stream_check_logs", [], |row| {
                    row.get(0)
                })?;
            (request_logs, rollups, stream_logs)
        };
        assert_eq!(request_logs, 1, "local request logs should be preserved");
        assert_eq!(rollups, 1, "local rollups should be preserved");
        assert_eq!(
            stream_logs, 1,
            "local stream check logs should be preserved"
        );

        Ok(())
    }

    #[test]
    #[serial]
    fn sync_import_preserves_local_routing_v2_catalog() -> Result<(), AppError> {
        let _test_home = isolated_test_home("sync-routing-v2-catalog");
        let remote_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(remote_db.conn);
            conn.execute(
                "INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('remote-provider', 'claude', 'Remote Provider', '{}', '{}')",
                [],
            )?;
        }
        let remote_sql = remote_db.export_sql_string_for_sync()?;
        assert!(
            !remote_sql.to_ascii_lowercase().contains("routing_v2_"),
            "WebDAV SQL 不得携带 routing v2 命名空间"
        );

        let local_db = Database::memory()?;
        {
            let conn = crate::database::lock_conn!(local_db.conn);
            conn.execute_batch(
                "INSERT INTO routing_v2_sites (site_id, display_name)
                 VALUES ('local-site', 'Local Site');
                 INSERT INTO routing_v2_endpoints (
                     endpoint_id, site_id, display_base_url, canonical_origin, base_path, protocol_family
                 ) VALUES (
                     'local-endpoint', 'local-site', 'https://local.example/v1',
                     'https://local.example', '/v1', 'anthropic'
                 );
                 INSERT INTO routing_v2_model_deployments (
                     deployment_id, site_id, endpoint_id, upstream_model_id, adapter_contract_revision
                 ) VALUES ('local-deployment', 'local-site', 'local-endpoint', 'local-model', 1);",
            )?;
        }

        local_db.import_sql_string_for_sync(&remote_sql)?;

        let local_catalog_counts: (i64, i64, i64) = {
            let conn = crate::database::lock_conn!(local_db.conn);
            let site_count = conn.query_row(
                "SELECT COUNT(*) FROM routing_v2_sites WHERE site_id = 'local-site'",
                [],
                |row| row.get(0),
            )?;
            let endpoint_count = conn.query_row(
                "SELECT COUNT(*) FROM routing_v2_endpoints WHERE endpoint_id = 'local-endpoint'",
                [],
                |row| row.get(0),
            )?;
            let deployment_count = conn.query_row(
                "SELECT COUNT(*) FROM routing_v2_model_deployments
                 WHERE deployment_id = 'local-deployment'",
                [],
                |row| row.get(0),
            )?;
            (site_count, endpoint_count, deployment_count)
        };
        assert_eq!(
            local_catalog_counts,
            (1, 1, 1),
            "同步导入必须按外键顺序恢复完整的本机 routing v2 目录"
        );
        Ok(())
    }

    #[test]
    #[serial]
    fn periodic_maintenance_runs_even_when_auto_backup_disabled() -> Result<(), AppError> {
        let old_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
        let test_home =
            std::env::temp_dir().join("cc-switch-periodic-maintenance-backup-disabled-test");
        let _ = std::fs::remove_dir_all(&test_home);
        std::fs::create_dir_all(&test_home).expect("create test home");
        std::env::set_var("CC_SWITCH_TEST_HOME", &test_home);

        let mut settings = AppSettings::default();
        settings.backup_interval_hours = Some(0);
        update_settings(settings).expect("disable auto backup");

        let db = Database::memory()?;
        let now = chrono::Utc::now().timestamp();
        let old_ts = now - 40 * 86400;
        let old_stream_ts = now - 8 * 86400;

        {
            let conn = crate::database::lock_conn!(db.conn);
            conn.execute(
                "INSERT INTO proxy_request_logs (
                    request_id, provider_id, app_type, model,
                    input_tokens, output_tokens, total_cost_usd,
                    latency_ms, status_code, created_at
                ) VALUES ('old-req', 'p1', 'claude', 'claude-3', 100, 50, '0.01', 100, 200, ?1)",
                [old_ts],
            )?;
            conn.execute(
                "INSERT INTO stream_check_logs (
                    provider_id, provider_name, app_type, status, success, message,
                    response_time_ms, http_status, model_used, retry_count, tested_at
                ) VALUES ('p1', 'Provider 1', 'claude', 'operational', 1, 'ok', 42, 200, 'claude-3', 0, ?1)",
                [old_stream_ts],
            )?;
        }

        db.periodic_backup_if_needed()?;

        let (remaining_request_logs, stream_logs, rollups): (i64, i64, i64) = {
            let conn = crate::database::lock_conn!(db.conn);
            let remaining_request_logs =
                conn.query_row("SELECT COUNT(*) FROM proxy_request_logs", [], |row| {
                    row.get(0)
                })?;
            let stream_logs =
                conn.query_row("SELECT COUNT(*) FROM stream_check_logs", [], |row| {
                    row.get(0)
                })?;
            let rollups =
                conn.query_row("SELECT COUNT(*) FROM usage_daily_rollups", [], |row| {
                    row.get(0)
                })?;
            (remaining_request_logs, stream_logs, rollups)
        };

        assert_eq!(
            remaining_request_logs, 0,
            "old request logs should still be pruned when auto backup is disabled"
        );
        assert_eq!(
            stream_logs, 0,
            "old stream check logs should still be pruned when auto backup is disabled"
        );
        assert_eq!(rollups, 1, "old request logs should be rolled up");

        match old_test_home {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }

        Ok(())
    }
}
