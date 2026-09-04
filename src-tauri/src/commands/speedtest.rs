//! Provider 批量测速相关命令。

use crate::app_config::AppType;
use crate::database::ProviderLatencyResult;
use crate::error::AppError;
use crate::provider::Provider;
use crate::services::speedtest::SpeedtestService;
use crate::store::AppState;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Provider 批量测速请求参数。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProvidersLatencyRequest {
    pub app_type: String,
    /// 为空时测试当前应用下所有可提取 base URL 的 provider。
    pub provider_ids: Option<Vec<String>>,
    pub timeout_secs: Option<u64>,
}

/// 单个 provider 的测速结果。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLatencyResponse {
    pub provider_id: String,
    pub provider_name: String,
    pub base_url: String,
    pub latency_ms: Option<i64>,
    pub status: Option<u16>,
    pub error: Option<String>,
    pub tested_at: i64,
}

/// Provider 批量测速响应。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProvidersLatencyResponse {
    pub results: Vec<ProviderLatencyResponse>,
    pub total: usize,
    pub success: usize,
    pub failed: usize,
}

struct ProviderLatencyTarget {
    provider_id: String,
    provider_name: String,
    base_url: String,
}

/// 测试 provider 延迟，并把最近一次结果写入缓存表。
#[tauri::command]
pub async fn test_providers_latency(
    state: tauri::State<'_, AppState>,
    request: TestProvidersLatencyRequest,
) -> Result<TestProvidersLatencyResponse, String> {
    test_providers_latency_impl(&state, request)
        .await
        .map_err(|e| e.to_string())
}

async fn test_providers_latency_impl(
    state: &AppState,
    request: TestProvidersLatencyRequest,
) -> Result<TestProvidersLatencyResponse, AppError> {
    let app_type = AppType::from_str(&request.app_type)?;
    let app_type_str = app_type.as_str();
    let requested_ids = request.provider_ids.as_ref().map(|ids| {
        ids.iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect::<HashSet<_>>()
    });

    let providers = state.db.get_all_providers(app_type_str)?;
    let targets = providers
        .iter()
        .filter(|(provider_id, _)| {
            requested_ids
                .as_ref()
                .map(|ids| ids.contains(provider_id.as_str()))
                .unwrap_or(true)
        })
        .filter_map(|(provider_id, provider)| {
            extract_base_url(&app_type, provider).map(|base_url| ProviderLatencyTarget {
                provider_id: provider_id.to_string(),
                provider_name: provider.name.clone(),
                base_url,
            })
        })
        .collect::<Vec<_>>();

    if targets.is_empty() {
        return Ok(TestProvidersLatencyResponse {
            results: Vec::new(),
            total: 0,
            success: 0,
            failed: 0,
        });
    }

    let urls = targets
        .iter()
        .map(|target| target.base_url.clone())
        .collect::<Vec<_>>();
    let endpoint_results = SpeedtestService::test_endpoints(urls, request.timeout_secs).await?;
    let tested_at = current_unix_timestamp_secs()?;

    let mut results = Vec::with_capacity(endpoint_results.len());
    let mut success = 0;
    let mut failed = 0;

    for (target, endpoint_result) in targets.into_iter().zip(endpoint_results) {
        let latency_ms = endpoint_result.latency.map(latency_to_i64);
        let status_i64 = endpoint_result.status.map(i64::from);
        let is_success = endpoint_result.error.is_none()
            && endpoint_result
                .status
                .map(|status| status < 400)
                .unwrap_or(false);

        if is_success {
            success += 1;
        } else {
            failed += 1;
        }

        let db_result = ProviderLatencyResult {
            provider_id: target.provider_id.clone(),
            app_type: app_type_str.to_string(),
            base_url: target.base_url.clone(),
            latency_ms,
            status: status_i64,
            error: endpoint_result.error.clone(),
            tested_at,
        };
        state.db.save_provider_latency_result(&db_result)?;

        results.push(ProviderLatencyResponse {
            provider_id: target.provider_id,
            provider_name: target.provider_name,
            base_url: target.base_url,
            latency_ms,
            status: endpoint_result.status,
            error: endpoint_result.error,
            tested_at,
        });
    }

    Ok(TestProvidersLatencyResponse {
        total: results.len(),
        success,
        failed,
        results,
    })
}

/// 获取缓存的 provider 延迟测试结果。
#[tauri::command]
pub async fn get_cached_latency_results(
    state: tauri::State<'_, AppState>,
    app_type: String,
) -> Result<Vec<ProviderLatencyResponse>, String> {
    get_cached_latency_results_impl(&state, app_type)
        .await
        .map_err(|e| e.to_string())
}

async fn get_cached_latency_results_impl(
    state: &AppState,
    app_type: String,
) -> Result<Vec<ProviderLatencyResponse>, AppError> {
    let app_type_enum = AppType::from_str(&app_type)?;
    let app_type_str = app_type_enum.as_str();
    let providers = state.db.get_all_providers(app_type_str)?;
    let db_results = state.db.get_all_provider_latency_results(app_type_str)?;

    Ok(db_results
        .into_iter()
        .filter_map(|db_result| {
            providers
                .get(&db_result.provider_id)
                .map(|provider| ProviderLatencyResponse {
                    provider_id: db_result.provider_id,
                    provider_name: provider.name.clone(),
                    base_url: db_result.base_url,
                    latency_ms: db_result.latency_ms,
                    status: db_result
                        .status
                        .and_then(|status| u16::try_from(status).ok()),
                    error: db_result.error,
                    tested_at: db_result.tested_at,
                })
        })
        .collect())
}

fn current_unix_timestamp_secs() -> Result<i64, AppError> {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::Message(format!("系统时间早于 Unix epoch: {e}")))?
        .as_secs();

    i64::try_from(secs).map_err(|_| AppError::Message("系统时间戳超出 i64 范围".to_string()))
}

fn latency_to_i64(latency: u128) -> i64 {
    i64::try_from(latency).unwrap_or(i64::MAX)
}

/// 从 provider 配置中提取可测速的 base URL。
fn extract_base_url(app_type: &AppType, provider: &Provider) -> Option<String> {
    let settings = &provider.settings_config;
    match app_type {
        AppType::Claude => nested_string(settings, "env", "ANTHROPIC_BASE_URL")
            .or_else(|| generic_base_url(settings)),
        AppType::Codex => extract_codex_base_url(settings),
        AppType::Gemini => nested_string(settings, "env", "GOOGLE_GEMINI_BASE_URL")
            .or_else(|| generic_base_url(settings)),
        AppType::OpenCode => nested_string(settings, "options", "baseURL")
            .or_else(|| nested_string(settings, "options", "base_url"))
            .or_else(|| generic_base_url(settings)),
        AppType::OpenClaw => value_string(settings, "baseUrl")
            .or_else(|| value_string(settings, "base_url"))
            .or_else(|| nested_string(settings, "options", "baseURL")),
    }
}

fn extract_codex_base_url(settings: &Value) -> Option<String> {
    generic_base_url(settings).or_else(|| {
        settings.get("config").and_then(|config| match config {
            Value::String(toml_str) => extract_codex_toml_base_url(toml_str),
            Value::Object(_) => {
                value_string(config, "base_url").or_else(|| value_string(config, "baseURL"))
            }
            _ => None,
        })
    })
}

fn extract_codex_toml_base_url(toml_str: &str) -> Option<String> {
    let parsed = toml::from_str::<toml::Value>(toml_str).ok();
    parsed
        .as_ref()
        .and_then(codex_toml_value_base_url)
        .or_else(|| extract_base_url_line(toml_str))
}

fn codex_toml_value_base_url(value: &toml::Value) -> Option<String> {
    string_from_toml(value.get("base_url"))
        .or_else(|| {
            let provider_name = value.get("model_provider").and_then(|v| v.as_str())?;
            let provider = value
                .get("model_providers")
                .and_then(|v| v.as_table())?
                .get(provider_name)?;
            string_from_toml(provider.get("base_url"))
        })
        .or_else(|| {
            value
                .get("model_providers")
                .and_then(|v| v.as_table())?
                .values()
                .find_map(|provider| string_from_toml(provider.get("base_url")))
        })
}

fn extract_base_url_line(toml_str: &str) -> Option<String> {
    toml_str.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if key.trim() != "base_url" {
            return None;
        }
        trim_to_string(value.trim().trim_matches(['"', '\'']))
    })
}

fn generic_base_url(settings: &Value) -> Option<String> {
    value_string(settings, "base_url")
        .or_else(|| value_string(settings, "baseURL"))
        .or_else(|| value_string(settings, "baseUrl"))
}

fn nested_string(settings: &Value, parent: &str, key: &str) -> Option<String> {
    value_string(settings.get(parent)?, key)
}

fn value_string(settings: &Value, key: &str) -> Option<String> {
    trim_to_string(settings.get(key)?.as_str()?)
}

fn string_from_toml(value: Option<&toml::Value>) -> Option<String> {
    trim_to_string(value?.as_str()?)
}

fn trim_to_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn provider(settings_config: Value) -> Provider {
        Provider::with_id(
            "test".to_string(),
            "Test Provider".to_string(),
            settings_config,
            None,
        )
    }

    #[test]
    fn speedtest_extract_base_url_supports_claude() {
        let provider = provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://claude.example.com"
            }
        }));

        assert_eq!(
            extract_base_url(&AppType::Claude, &provider).as_deref(),
            Some("https://claude.example.com")
        );
    }

    #[test]
    fn speedtest_extract_base_url_supports_codex_top_level() {
        let provider = provider(json!({
            "base_url": "https://codex.example.com/v1"
        }));

        assert_eq!(
            extract_base_url(&AppType::Codex, &provider).as_deref(),
            Some("https://codex.example.com/v1")
        );
    }

    #[test]
    fn speedtest_extract_base_url_supports_codex_model_provider_toml() {
        let provider = provider(json!({
            "config": r#"
model_provider = "custom"

[model_providers.custom]
base_url = "https://codex-provider.example.com/v1"
"#
        }));

        assert_eq!(
            extract_base_url(&AppType::Codex, &provider).as_deref(),
            Some("https://codex-provider.example.com/v1")
        );
    }

    #[test]
    fn speedtest_extract_base_url_supports_gemini() {
        let provider = provider(json!({
            "env": {
                "GOOGLE_GEMINI_BASE_URL": "https://gemini.example.com"
            }
        }));

        assert_eq!(
            extract_base_url(&AppType::Gemini, &provider).as_deref(),
            Some("https://gemini.example.com")
        );
    }

    #[test]
    fn speedtest_extract_base_url_supports_opencode() {
        let provider = provider(json!({
            "options": {
                "baseURL": "https://opencode.example.com/v1"
            }
        }));

        assert_eq!(
            extract_base_url(&AppType::OpenCode, &provider).as_deref(),
            Some("https://opencode.example.com/v1")
        );
    }

    #[test]
    fn speedtest_extract_base_url_supports_openclaw() {
        let provider = provider(json!({
            "baseUrl": "https://openclaw.example.com/v1"
        }));

        assert_eq!(
            extract_base_url(&AppType::OpenClaw, &provider).as_deref(),
            Some("https://openclaw.example.com/v1")
        );
    }

    #[test]
    fn speedtest_extract_base_url_ignores_blank_values() {
        let provider = provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "   "
            }
        }));

        assert!(extract_base_url(&AppType::Claude, &provider).is_none());
    }
}
