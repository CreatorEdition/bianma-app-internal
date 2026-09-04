//! 旧 Provider 目录的窄只读适配器。
//!
//! 本模块是 Phase 2 迁移的发现边界：只投影可公开的目录字段，绝不读取旧
//! `settings_config`、`meta` 或任何能够承载凭据的旧 DTO。端点 URL 在离开
//! 旧表前必须完成无凭据、无 query/fragment 的本地规范化；不合格记录只保留
//! 稳定来源和封闭失败码，绝不保留原始 URL。
//!
//! 此模块不写入 routing v2 表、不创建 Vault 记录，也不参与 Proxy 或真实请求
//! 执行。迁移 journal 与目标元数据的原子写入仍属于后续独立 Saga 切片。

use crate::app_config::AppType;
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

const LEGACY_PROVIDER_CATALOG_WITH_WEBSITE_QUERY: &str =
    "SELECT id, app_type, name, website_url FROM providers ORDER BY app_type ASC, id ASC";
const LEGACY_PROVIDER_CATALOG_WITHOUT_WEBSITE_QUERY: &str =
    "SELECT id, app_type, name, NULL AS website_url FROM providers ORDER BY app_type ASC, id ASC";
const LEGACY_ENDPOINT_CATALOG_QUERY: &str = "SELECT id, provider_id, app_type, url \
    FROM provider_endpoints ORDER BY app_type ASC, provider_id ASC, id ASC";

const MAX_LEGACY_SOURCE_ID_BYTES: usize = 256;
const MAX_LEGACY_APP_TYPE_BYTES: usize = 32;
const MAX_LEGACY_DISPLAY_NAME_BYTES: usize = 512;
const MAX_LEGACY_URL_BYTES: usize = 4096;

/// 旧目录读取的封闭失败码。
///
/// 该码可在后续 migration journal 中持久化，但类型不携带未规范化 URL、JSON
/// 或 Secret，避免失败处理本身成为信息外流路径。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum LegacyProviderCatalogFailureCode {
    InvalidProviderIdentity,
    UnsupportedAppType,
    InvalidProviderName,
    InvalidEndpointIdentity,
    InvalidEndpointId,
    OrphanEndpoint,
    UnsafeEndpointUrl,
    UnsafeWebsiteUrl,
}

/// 尚未进入 routing v2 的隔离记录。
///
/// 仅保存 migration journal 必需的稳定来源标识，不保存端点原始 URL、配置 JSON
/// 或可执行元数据。
pub(crate) struct LegacyProviderCatalogQuarantine {
    pub(crate) source_provider_id: String,
    pub(crate) source_app_type: String,
    pub(crate) source_endpoint_id: Option<i64>,
    pub(crate) failure_code: LegacyProviderCatalogFailureCode,
}

/// 已通过只读发现边界的旧 Provider 目录记录。
///
/// 一个记录必须保持一个独立的 v2 Site 边界；后续迁移不得在这里按域名或网站
/// 地址自动合并 Provider。
pub(crate) struct LegacyProviderCatalogRecord {
    pub(crate) source_provider_id: String,
    pub(crate) source_app_type: String,
    pub(crate) display_name: String,
    pub(crate) website_url: Option<String>,
    pub(crate) endpoints: Vec<LegacyProviderEndpointRecord>,
}

/// 已规范化且不含凭据材料的旧端点目录记录。
pub(crate) struct LegacyProviderEndpointRecord {
    pub(crate) source_endpoint_id: i64,
    pub(crate) display_base_url: String,
    pub(crate) canonical_origin: String,
    pub(crate) base_path: String,
}

/// 旧 Provider 目录读取结果。
///
/// `providers` 只包含能安全进入后续 Saga 的无 Secret 目录；`quarantined` 供
/// Saga 以固定 failure code 记录，不会触发网络、重试或数据写入。
pub(crate) struct LegacyProviderCatalogSnapshot {
    pub(crate) providers: Vec<LegacyProviderCatalogRecord>,
    pub(crate) quarantined: Vec<LegacyProviderCatalogQuarantine>,
}

/// 旧 Provider 目录读取端口。
///
/// 实现只能使用本模块内固定的窄 SQL 投影，避免未来调用方通过旧
/// `Provider`、`ProviderService` 或 `SELECT *` 重新接触配置 JSON 和密钥。
pub(crate) trait LegacyProviderCatalogReadPort {
    /// 读取确定性排序的无 Secret 目录，不修改任意数据库状态。
    fn read_legacy_provider_catalog(&self) -> Result<LegacyProviderCatalogSnapshot, AppError>;
}

impl LegacyProviderCatalogReadPort for Database {
    fn read_legacy_provider_catalog(&self) -> Result<LegacyProviderCatalogSnapshot, AppError> {
        let conn = lock_conn!(self.conn);
        validate_legacy_catalog_schema(&conn)?;

        let mut quarantined = Vec::new();
        let mut providers = read_legacy_providers(&conn, &mut quarantined)?;
        let mut provider_index = BTreeMap::new();

        for (index, provider) in providers.iter().enumerate() {
            provider_index.insert(
                (
                    provider.source_app_type.clone(),
                    provider.source_provider_id.clone(),
                ),
                index,
            );
        }

        read_legacy_endpoints(&conn, &mut providers, &provider_index, &mut quarantined)?;

        Ok(LegacyProviderCatalogSnapshot {
            providers,
            quarantined,
        })
    }
}

/// 读取 Provider 固定投影，并将不合格来源隔离而不是构造可执行 DTO。
fn read_legacy_providers(
    conn: &Connection,
    quarantined: &mut Vec<LegacyProviderCatalogQuarantine>,
) -> Result<Vec<LegacyProviderCatalogRecord>, AppError> {
    let provider_query = if Database::has_column(conn, "providers", "website_url")? {
        LEGACY_PROVIDER_CATALOG_WITH_WEBSITE_QUERY
    } else {
        LEGACY_PROVIDER_CATALOG_WITHOUT_WEBSITE_QUERY
    };
    let mut stmt = conn
        .prepare(provider_query)
        .map_err(|error| AppError::Database(format!("准备旧 Provider 目录查询失败: {error}")))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| AppError::Database(format!("查询旧 Provider 目录失败: {error}")))?;

    let mut providers = Vec::new();
    let mut seen_sources = BTreeSet::new();
    for row in rows {
        let (source_provider_id, source_app_type, display_name, website_url) =
            row.map_err(|error| AppError::Database(format!("读取旧 Provider 目录失败: {error}")))?;

        if !seen_sources.insert((source_app_type.clone(), source_provider_id.clone())) {
            return Err(AppError::InvalidInput(
                "旧 Provider 目录存在重复复合来源，已拒绝发现".to_string(),
            ));
        }

        let failure_code =
            provider_failure_code(&source_provider_id, &source_app_type, &display_name);
        if let Some(failure_code) = failure_code {
            quarantined.push(LegacyProviderCatalogQuarantine {
                source_provider_id,
                source_app_type,
                source_endpoint_id: None,
                failure_code,
            });
            continue;
        }

        let normalized_website_url = match website_url {
            Some(url) => match normalize_safe_url(&url) {
                Ok(safe_url) => Some(safe_url.display_base_url),
                Err(()) => {
                    quarantined.push(LegacyProviderCatalogQuarantine {
                        source_provider_id: source_provider_id.clone(),
                        source_app_type: source_app_type.clone(),
                        source_endpoint_id: None,
                        failure_code: LegacyProviderCatalogFailureCode::UnsafeWebsiteUrl,
                    });
                    None
                }
            },
            None => None,
        };

        providers.push(LegacyProviderCatalogRecord {
            source_provider_id,
            source_app_type,
            display_name,
            website_url: normalized_website_url,
            endpoints: Vec::new(),
        });
    }
    Ok(providers)
}

/// 读取端点固定投影，按复合来源归属安全附着或隔离。
fn read_legacy_endpoints(
    conn: &Connection,
    providers: &mut [LegacyProviderCatalogRecord],
    provider_index: &BTreeMap<(String, String), usize>,
    quarantined: &mut Vec<LegacyProviderCatalogQuarantine>,
) -> Result<(), AppError> {
    let mut stmt = conn
        .prepare(LEGACY_ENDPOINT_CATALOG_QUERY)
        .map_err(|error| AppError::Database(format!("准备旧端点目录查询失败: {error}")))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| AppError::Database(format!("查询旧端点目录失败: {error}")))?;

    let mut seen_endpoint_sources = BTreeSet::new();
    for row in rows {
        let (source_endpoint_id, source_provider_id, source_app_type, raw_url) =
            row.map_err(|error| AppError::Database(format!("读取旧端点目录失败: {error}")))?;

        if !seen_endpoint_sources.insert(source_endpoint_id) {
            return Err(AppError::InvalidInput(
                "旧端点目录存在重复来源，已拒绝发现".to_string(),
            ));
        }

        let normalized_url = normalize_safe_url(&raw_url);
        let failure_code = endpoint_failure_code(
            source_endpoint_id,
            &source_provider_id,
            &source_app_type,
            provider_index,
            normalized_url.is_err(),
        );
        if let Some(failure_code) = failure_code {
            quarantined.push(LegacyProviderCatalogQuarantine {
                source_provider_id,
                source_app_type,
                source_endpoint_id: Some(source_endpoint_id),
                failure_code,
            });
            continue;
        }

        let safe_url = normalized_url.map_err(|_| {
            AppError::InvalidInput("旧端点目录 URL 规范化不变量被破坏，已拒绝发现".to_string())
        })?;
        let provider_index = provider_index
            .get(&(source_app_type, source_provider_id))
            .ok_or_else(|| {
                AppError::InvalidInput("旧端点目录归属不变量被破坏，已拒绝发现".to_string())
            })?;
        providers[*provider_index]
            .endpoints
            .push(LegacyProviderEndpointRecord {
                source_endpoint_id,
                display_base_url: safe_url.display_base_url,
                canonical_origin: safe_url.canonical_origin,
                base_path: safe_url.base_path,
            });
    }
    Ok(())
}

/// 判断 Provider 是否可作为后续 Saga 的稳定无 Secret 来源。
fn provider_failure_code(
    source_provider_id: &str,
    source_app_type: &str,
    display_name: &str,
) -> Option<LegacyProviderCatalogFailureCode> {
    if !is_safe_legacy_text(source_provider_id, MAX_LEGACY_SOURCE_ID_BYTES) {
        return Some(LegacyProviderCatalogFailureCode::InvalidProviderIdentity);
    }
    if !is_supported_app_type(source_app_type) {
        return Some(LegacyProviderCatalogFailureCode::UnsupportedAppType);
    }
    if !is_safe_legacy_text(display_name, MAX_LEGACY_DISPLAY_NAME_BYTES) {
        return Some(LegacyProviderCatalogFailureCode::InvalidProviderName);
    }
    None
}

/// 判断端点是否可安全投影，失败时只返回封闭原因。
fn endpoint_failure_code(
    source_endpoint_id: i64,
    source_provider_id: &str,
    source_app_type: &str,
    provider_index: &BTreeMap<(String, String), usize>,
    has_unsafe_url: bool,
) -> Option<LegacyProviderCatalogFailureCode> {
    if source_endpoint_id <= 0 {
        return Some(LegacyProviderCatalogFailureCode::InvalidEndpointId);
    }
    if !is_safe_legacy_text(source_provider_id, MAX_LEGACY_SOURCE_ID_BYTES)
        || !is_supported_app_type(source_app_type)
    {
        return Some(LegacyProviderCatalogFailureCode::InvalidEndpointIdentity);
    }
    if !provider_index.contains_key(&(source_app_type.to_string(), source_provider_id.to_string()))
    {
        return Some(LegacyProviderCatalogFailureCode::OrphanEndpoint);
    }
    if has_unsafe_url {
        return Some(LegacyProviderCatalogFailureCode::UnsafeEndpointUrl);
    }
    None
}

/// 只接受现有 AppType 的精确小写标识，禁止宽松 trim/lowercase 归一化。
fn is_supported_app_type(value: &str) -> bool {
    value.len() <= MAX_LEGACY_APP_TYPE_BYTES
        && AppType::all().any(|app_type| app_type.as_str() == value)
}

/// 只允许稳定来源标识和本地显示名携带有限、非控制字符的 UTF-8 文本。
fn is_safe_legacy_text(value: &str, maximum_bytes: usize) -> bool {
    !value.trim().is_empty() && value.len() <= maximum_bytes && !value.chars().any(char::is_control)
}

/// 无 Secret 的 HTTP(S) URL 规范化结果。
struct SafeUrl {
    display_base_url: String,
    canonical_origin: String,
    base_path: String,
}

/// 只允许绝对 HTTP(S) URL，拒绝 userinfo、query、fragment、控制字符和超长输入。
///
/// 错误不携带原始输入，调用方只能映射为封闭 quarantine failure code。
fn normalize_safe_url(raw_url: &str) -> Result<SafeUrl, ()> {
    if !is_safe_legacy_text(raw_url, MAX_LEGACY_URL_BYTES) {
        return Err(());
    }

    let parsed = Url::parse(raw_url).map_err(|_| ())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(());
    }

    let canonical_origin = parsed.origin().ascii_serialization();
    if canonical_origin == "null" {
        return Err(());
    }

    Ok(SafeUrl {
        display_base_url: parsed.as_str().to_string(),
        canonical_origin,
        base_path: parsed.path().to_string(),
    })
}

/// 确认旧表仍具备固定投影所需的列。
///
/// `website_url` 是可选展示字段：早期真实 schema 缺失时使用另一条固定 SQL 以
/// `NULL` 投影，不会格式化动态列名或阻断可安全发现的 Provider。
fn validate_legacy_catalog_schema(conn: &Connection) -> Result<(), AppError> {
    let required_columns: &[(&str, &[&str])] = &[
        ("providers", &["id", "app_type", "name"]),
        (
            "provider_endpoints",
            &["id", "provider_id", "app_type", "url"],
        ),
    ];
    for (table, columns) in required_columns {
        if !Database::table_exists(conn, table)? {
            return Err(AppError::InvalidInput(format!(
                "旧 Provider 目录缺少表 {table}，已拒绝发现"
            )));
        }
        for column in columns.iter().copied() {
            if !Database::has_column(conn, table, column)? {
                return Err(AppError::InvalidInput(format!(
                    "旧 Provider 目录缺少必要列 {table}.{column}，已拒绝发现"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_safe_url, LegacyProviderCatalogFailureCode, LegacyProviderCatalogReadPort,
        LEGACY_ENDPOINT_CATALOG_QUERY, LEGACY_PROVIDER_CATALOG_WITHOUT_WEBSITE_QUERY,
        LEGACY_PROVIDER_CATALOG_WITH_WEBSITE_QUERY,
    };
    use crate::database::{lock_conn, Database};
    use crate::error::AppError;
    use rusqlite::Connection;
    use std::sync::Mutex;

    #[test]
    fn catalog_queries_only_project_allowlisted_columns() {
        for query in [
            LEGACY_PROVIDER_CATALOG_WITH_WEBSITE_QUERY,
            LEGACY_PROVIDER_CATALOG_WITHOUT_WEBSITE_QUERY,
            LEGACY_ENDPOINT_CATALOG_QUERY,
        ] {
            let normalized = query.to_ascii_lowercase();
            assert!(!normalized.contains("select *"));
            assert!(!normalized.contains("settings_config"));
            assert!(!normalized.contains("meta"));
            assert!(!normalized.contains("api_key"));
        }
    }

    #[test]
    fn catalog_read_is_deterministic_read_only_and_excludes_legacy_config() -> Result<(), AppError>
    {
        let database = Database::memory()?;
        {
            let conn = lock_conn!(database.conn);
            conn.execute_batch(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url, meta)
                 VALUES
                    ('zeta', 'codex', 'Zeta', '{\"legacy\":\"not-projected\"}', NULL, '{\"internal\":\"not-projected\"}'),
                    ('alpha', 'claude', 'Alpha', '{\"legacy\":\"not-projected\"}', 'https://alpha.example', '{\"internal\":\"not-projected\"}');
                 INSERT INTO provider_endpoints (provider_id, app_type, url)
                 VALUES
                    ('zeta', 'codex', 'https://zeta.example/v1'),
                    ('alpha', 'claude', 'https://alpha.example/v2'),
                    ('alpha', 'claude', 'https://alpha.example/v1');",
            )?;
        }

        let changes_before = total_changes(&database)?;
        let catalog = database.read_legacy_provider_catalog()?;
        let changes_after = total_changes(&database)?;

        assert_eq!(changes_after, changes_before, "目录读取不得写入数据库");
        assert!(catalog.quarantined.is_empty());
        assert_eq!(catalog.providers.len(), 2);
        assert_eq!(catalog.providers[0].source_app_type, "claude");
        assert_eq!(catalog.providers[0].source_provider_id, "alpha");
        assert_eq!(catalog.providers[0].display_name, "Alpha");
        assert_eq!(
            catalog.providers[0].website_url.as_deref(),
            Some("https://alpha.example")
        );
        assert_eq!(
            catalog.providers[0]
                .endpoints
                .iter()
                .map(|endpoint| endpoint.display_base_url.as_str())
                .collect::<Vec<_>>(),
            vec!["https://alpha.example/v2", "https://alpha.example/v1"]
        );
        assert_eq!(catalog.providers[1].source_app_type, "codex");
        assert_eq!(catalog.providers[1].source_provider_id, "zeta");
        assert_eq!(catalog.providers[1].endpoints[0].source_endpoint_id, 1);
        assert_eq!(
            catalog.providers[1].endpoints[0].canonical_origin,
            "https://zeta.example"
        );
        assert_eq!(catalog.providers[1].endpoints[0].base_path, "/v1");
        Ok(())
    }

    #[test]
    fn catalog_read_handles_legacy_schema_without_website_url() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE providers (
                 id TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 name TEXT NOT NULL,
                 settings_config TEXT NOT NULL,
                 PRIMARY KEY (id, app_type)
             );
             CREATE TABLE provider_endpoints (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 provider_id TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 url TEXT NOT NULL,
                 FOREIGN KEY (provider_id, app_type) REFERENCES providers(id, app_type)
             );
             INSERT INTO providers (id, app_type, name, settings_config)
             VALUES ('legacy', 'claude', 'Legacy', '{\"ignored\":true}');
             INSERT INTO provider_endpoints (provider_id, app_type, url)
             VALUES ('legacy', 'claude', 'https://legacy.example/v1');",
        )?;
        let database = Database {
            conn: Mutex::new(conn),
        };

        let catalog = database.read_legacy_provider_catalog()?;
        assert_eq!(catalog.providers.len(), 1);
        assert_eq!(catalog.providers[0].website_url, None);
        assert_eq!(
            catalog.providers[0].endpoints[0].canonical_origin,
            "https://legacy.example"
        );
        Ok(())
    }

    #[test]
    fn catalog_read_quarantines_unsafe_urls_without_retaining_them() -> Result<(), AppError> {
        let database = Database::memory()?;
        {
            let conn = lock_conn!(database.conn);
            conn.execute_batch(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url, meta)
                 VALUES ('alpha', 'claude', 'Alpha', '{}', 'https://website.example/?session=not-projected', '{}');
                 INSERT INTO provider_endpoints (provider_id, app_type, url)
                 VALUES
                    ('alpha', 'claude', 'https://user@host.example/v1'),
                    ('alpha', 'claude', 'https://host.example/v1?session=not-projected'),
                    ('alpha', 'claude', 'https://host.example/v1#not-projected'),
                    ('alpha', 'claude', 'file:///tmp/not-projected'),
                    ('alpha', 'claude', '/relative/not-projected');",
            )?;
        }

        let catalog = database.read_legacy_provider_catalog()?;
        assert_eq!(catalog.providers.len(), 1);
        assert_eq!(catalog.providers[0].website_url, None);
        assert!(catalog.providers[0].endpoints.is_empty());
        assert_eq!(catalog.quarantined.len(), 6);
        assert_eq!(
            catalog
                .quarantined
                .iter()
                .filter(|record| {
                    record.failure_code == LegacyProviderCatalogFailureCode::UnsafeWebsiteUrl
                })
                .count(),
            1
        );
        assert_eq!(
            catalog
                .quarantined
                .iter()
                .filter(|record| {
                    record.failure_code == LegacyProviderCatalogFailureCode::UnsafeEndpointUrl
                })
                .count(),
            5
        );
        let website_quarantine = catalog
            .quarantined
            .iter()
            .find(|record| {
                record.failure_code == LegacyProviderCatalogFailureCode::UnsafeWebsiteUrl
            })
            .expect("非法 website_url 必须进入隔离记录");
        assert_eq!(website_quarantine.source_endpoint_id, None);
        assert!(catalog.quarantined.iter().all(
            |record| record.source_provider_id == "alpha" && record.source_app_type == "claude"
        ));
        assert!(catalog
            .quarantined
            .iter()
            .filter(|record| {
                record.failure_code == LegacyProviderCatalogFailureCode::UnsafeEndpointUrl
            })
            .all(|record| record.source_endpoint_id.is_some()));
        Ok(())
    }

    #[test]
    fn normalize_safe_url_uses_url_parser_canonical_display() {
        let normalized = normalize_safe_url("https://EXAMPLE.com:443/v1/").expect("URL 应可规范化");

        assert_eq!(normalized.display_base_url, "https://example.com/v1/");
        assert_eq!(normalized.canonical_origin, "https://example.com");
        assert_eq!(normalized.base_path, "/v1/");
    }

    #[test]
    fn catalog_read_keeps_same_website_and_cross_app_sources_separate() -> Result<(), AppError> {
        let database = Database::memory()?;
        {
            let conn = lock_conn!(database.conn);
            conn.execute_batch(
                "INSERT INTO providers (id, app_type, name, settings_config, website_url, meta)
                 VALUES
                    ('shared', 'claude', 'Claude source', '{}', 'https://same.example', '{}'),
                    ('shared', 'codex', 'Codex source', '{}', 'https://same.example', '{}');
                 INSERT INTO provider_endpoints (provider_id, app_type, url)
                 VALUES
                    ('shared', 'claude', 'https://claude.example/v1'),
                    ('shared', 'codex', 'https://codex.example/v1');",
            )?;
        }

        let catalog = database.read_legacy_provider_catalog()?;
        assert_eq!(catalog.providers.len(), 2);
        assert_eq!(catalog.providers[0].source_app_type, "claude");
        assert_eq!(
            catalog.providers[0].endpoints[0].canonical_origin,
            "https://claude.example"
        );
        assert_eq!(catalog.providers[1].source_app_type, "codex");
        assert_eq!(
            catalog.providers[1].endpoints[0].canonical_origin,
            "https://codex.example"
        );
        Ok(())
    }

    #[test]
    fn catalog_read_quarantines_orphan_and_unsupported_sources() -> Result<(), AppError> {
        let database = Database::memory()?;
        {
            let conn = lock_conn!(database.conn);
            conn.execute_batch(
                "PRAGMA foreign_keys = OFF;
                 INSERT INTO providers (id, app_type, name, settings_config, meta)
                 VALUES ('unsupported', 'Claude', 'Unsupported', '{}', '{}');
                 INSERT INTO provider_endpoints (provider_id, app_type, url)
                 VALUES ('missing', 'claude', 'https://orphan.example/v1');
                 PRAGMA foreign_keys = ON;",
            )?;
        }

        let catalog = database.read_legacy_provider_catalog()?;
        assert!(catalog.providers.is_empty());
        assert_eq!(catalog.quarantined.len(), 2);
        assert!(matches!(
            catalog.quarantined[0].failure_code,
            LegacyProviderCatalogFailureCode::UnsupportedAppType
        ));
        assert!(matches!(
            catalog.quarantined[1].failure_code,
            LegacyProviderCatalogFailureCode::OrphanEndpoint
        ));
        Ok(())
    }

    #[test]
    fn catalog_read_fails_closed_for_duplicate_provider_source() -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE providers (
                 id TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 name TEXT NOT NULL,
                 settings_config TEXT NOT NULL
             );
             CREATE TABLE provider_endpoints (
                 id INTEGER NOT NULL,
                 provider_id TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 url TEXT NOT NULL
             );
             INSERT INTO providers (id, app_type, name, settings_config)
             VALUES
                ('duplicate', 'claude', 'First', '{}'),
                ('duplicate', 'claude', 'Second', '{}');",
        )?;
        let database = Database {
            conn: Mutex::new(conn),
        };

        let error = match database.read_legacy_provider_catalog() {
            Ok(_) => panic!("重复复合来源不能被静默绑定到任意 Provider"),
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidInput(_)));
        assert!(error.to_string().contains("重复复合来源"));
        Ok(())
    }

    #[test]
    fn catalog_read_fails_closed_for_duplicate_endpoint_source_across_providers(
    ) -> Result<(), AppError> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE providers (
                 id TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 name TEXT NOT NULL,
                 settings_config TEXT NOT NULL,
                 PRIMARY KEY (id, app_type)
             );
             CREATE TABLE provider_endpoints (
                 id INTEGER NOT NULL,
                 provider_id TEXT NOT NULL,
                 app_type TEXT NOT NULL,
                 url TEXT NOT NULL
             );
             INSERT INTO providers (id, app_type, name, settings_config)
             VALUES
                ('duplicate', 'claude', 'Provider', '{}'),
                ('other', 'codex', 'Other', '{}');
             INSERT INTO provider_endpoints (id, provider_id, app_type, url)
             VALUES
                (7, 'duplicate', 'claude', 'https://first.example/v1'),
                (7, 'other', 'codex', 'https://second.example/v1');",
        )?;
        let database = Database {
            conn: Mutex::new(conn),
        };

        let error = match database.read_legacy_provider_catalog() {
            Ok(_) => panic!("重复端点来源不能被静默附着到 Provider"),
            Err(error) => error,
        };
        assert!(matches!(error, AppError::InvalidInput(_)));
        assert!(error.to_string().contains("端点目录存在重复来源"));
        Ok(())
    }

    fn total_changes(database: &Database) -> Result<i64, AppError> {
        let conn = lock_conn!(database.conn);
        conn.query_row("SELECT total_changes()", [], |row| row.get(0))
            .map_err(AppError::from)
    }
}
