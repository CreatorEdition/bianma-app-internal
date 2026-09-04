//! 模型列表获取服务
//!
//! 通过模型发现端点获取供应商可用模型列表。
//!
//! `fetch_models` 保留供应商表单现有的 OpenAI 兼容 `/v1/models` 调用；
//! `fetch_provider_models` 提供 Provider Workspace 后续可复用的通用发现能力。

use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::Duration;
use url::Url;

/// 获取到的模型信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedModel {
    pub id: String,
    pub owned_by: Option<String>,
}

/// 通用模型发现结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredModel {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owned_by: Option<String>,
}

/// OpenAI 兼容的 /v1/models 响应格式
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Option<Vec<ModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct ModelEntry {
    id: String,
    owned_by: Option<String>,
}

const FETCH_TIMEOUT_SECS: u64 = 15;
const DISCOVERY_TIMEOUT_SECS: u64 = 12;
const ERROR_MISSING_ENDPOINT: &str = "missing_endpoint";
const ERROR_INVALID_URL: &str = "invalid_url";
const ERROR_UNAUTHORIZED: &str = "unauthorized";
const ERROR_ENDPOINT_NOT_FOUND: &str = "endpoint_not_found";
const ERROR_TIMEOUT: &str = "timeout";
const ERROR_EMPTY_MODEL_LIST: &str = "empty_model_list";
const ERROR_INVALID_MODEL_ARRAY: &str = "invalid_model_array";
const ERROR_GENERIC: &str = "generic";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtocolHint {
    OpenAi,
    Anthropic,
}

impl ProtocolHint {
    fn from_str(value: Option<&str>) -> Self {
        match value
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str()
        {
            "anthropic" => Self::Anthropic,
            _ => Self::OpenAi,
        }
    }
}

#[derive(Debug, Clone)]
struct ModelFetchFailure {
    code: &'static str,
    message: &'static str,
    endpoint: Option<String>,
    status: Option<u16>,
    detail: Option<String>,
}

impl ModelFetchFailure {
    fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            endpoint: None,
            status: None,
            detail: None,
        }
    }

    fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self
    }

    fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let value = detail.into();
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            self.detail = Some(trimmed.to_string());
        }
        self
    }

    fn from_status(status: u16) -> Self {
        match status {
            401 | 403 => Self::new(
                ERROR_UNAUTHORIZED,
                "Authentication failed when fetching models",
            )
            .with_status(status),
            404 | 405 => Self::new(
                ERROR_ENDPOINT_NOT_FOUND,
                "Model discovery endpoint is not available",
            )
            .with_status(status),
            _ => Self::new(ERROR_GENERIC, "Model discovery request failed").with_status(status),
        }
    }

    fn into_error(self) -> String {
        serde_json::to_string(&json!({
            "code": self.code,
            "message": self.message,
            "context": {
                "endpoint": self.endpoint,
                "status": self.status,
                "detail": self.detail,
            },
        }))
        .unwrap_or_else(|_| self.message.to_string())
    }
}

/// 获取供应商的可用模型列表
///
/// 使用 OpenAI 兼容的 GET /v1/models 端点。
pub async fn fetch_models(
    base_url: &str,
    api_key: &str,
    is_full_url: bool,
) -> Result<Vec<FetchedModel>, String> {
    if api_key.is_empty() {
        return Err("API Key is required to fetch models".to_string());
    }

    let models_url = build_models_url(base_url, is_full_url)?;
    let client = crate::proxy::http_client::get_for_provider(None);

    let response = client
        .get(&models_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    let resp: ModelsResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {e}"))?;

    let mut models: Vec<FetchedModel> = resp
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|m| FetchedModel {
            id: m.id,
            owned_by: m.owned_by,
        })
        .collect();

    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

/// 获取供应商的通用模型发现结果
///
/// 支持 OpenAI 与 Anthropic 协议提示，并按候选顺序探测 `/models` 与 `/v1/models`。
/// 错误以 JSON 字符串返回，前端可解析其中的 `code` 与 `context`。
pub async fn fetch_provider_models(
    base_url: &str,
    api_key: Option<&str>,
    protocol_hint: Option<&str>,
) -> Result<Vec<DiscoveredModel>, String> {
    let parsed_base = parse_base_url(base_url)?;
    let hint = ProtocolHint::from_str(protocol_hint);
    let candidate_urls = build_candidate_urls(&parsed_base);
    let headers = build_discovery_headers(api_key, hint)?;
    let client = crate::proxy::http_client::get_for_provider(None);

    let mut last_failure: Option<ModelFetchFailure> = None;
    for url in candidate_urls {
        match fetch_discovered_models_from_url(&client, &url, &headers).await {
            Ok(models) => {
                if models.is_empty() {
                    last_failure = Some(
                        ModelFetchFailure::new(
                            ERROR_EMPTY_MODEL_LIST,
                            "Model discovery returned an empty model list",
                        )
                        .with_endpoint(url),
                    );
                } else {
                    return Ok(models);
                }
            }
            Err(failure) => {
                last_failure = Some(failure);
            }
        }
    }

    Err(last_failure
        .unwrap_or_else(|| {
            ModelFetchFailure::new(ERROR_GENERIC, "Model discovery failed")
                .with_detail("No candidate endpoint is available")
        })
        .into_error())
}

/// 构造 /v1/models 的完整 URL
fn build_models_url(base_url: &str, is_full_url: bool) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');

    if trimmed.is_empty() {
        return Err("Base URL is empty".to_string());
    }

    if is_full_url {
        // 尝试从完整端点 URL 推导 API 根路径
        // 例如: https://proxy.example.com/v1/chat/completions → https://proxy.example.com/v1/models
        if let Some(idx) = trimmed.find("/v1/") {
            return Ok(format!("{}/v1/models", &trimmed[..idx]));
        }
        // 如果没有 /v1/ 路径，直接去掉最后一段路径
        if let Some(idx) = trimmed.rfind('/') {
            let root = &trimmed[..idx];
            if root.contains("://") && root.len() > root.find("://").unwrap() + 3 {
                return Ok(format!("{root}/v1/models"));
            }
        }
        return Err("Cannot derive models endpoint from full URL".to_string());
    }

    // 常规情况: base_url 是 API 根路径
    // 如果已经包含 /v1 路径，直接追加 /models
    if trimmed.ends_with("/v1") {
        return Ok(format!("{trimmed}/models"));
    }

    Ok(format!("{trimmed}/v1/models"))
}

fn parse_base_url(raw: &str) -> Result<Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(
            ModelFetchFailure::new(ERROR_MISSING_ENDPOINT, "baseUrl is required").into_error(),
        );
    }

    let mut parsed = Url::parse(trimmed).map_err(|e| {
        ModelFetchFailure::new(ERROR_INVALID_URL, "baseUrl is invalid")
            .with_detail(e.to_string())
            .into_error()
    })?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(
            ModelFetchFailure::new(ERROR_INVALID_URL, "baseUrl must use http or https")
                .with_detail(format!("unsupported URL scheme: {}", parsed.scheme()))
                .into_error(),
        );
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed)
}

fn build_candidate_urls(base_url: &Url) -> Vec<String> {
    let normalized = base_url.as_str().trim_end_matches('/').to_string();
    if normalized.ends_with("/models") {
        return vec![normalized];
    }

    let mut candidates = Vec::new();
    if normalized.ends_with("/v1") {
        candidates.push(format!("{normalized}/models"));
        if let Some(root) = normalized.strip_suffix("/v1") {
            candidates.push(format!("{root}/models"));
        }
    } else {
        candidates.push(format!("{normalized}/models"));
        candidates.push(format!("{normalized}/v1/models"));
    }

    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.clone()))
        .collect()
}

fn build_discovery_headers(
    api_key: Option<&str>,
    protocol_hint: ProtocolHint,
) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

    let key = api_key.map(str::trim).filter(|value| !value.is_empty());
    if let Some(key) = key {
        match protocol_hint {
            ProtocolHint::OpenAi => {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {key}")).map_err(|e| {
                        ModelFetchFailure::new(ERROR_GENERIC, "apiKey is invalid")
                            .with_detail(e.to_string())
                            .into_error()
                    })?,
                );
            }
            ProtocolHint::Anthropic => {
                headers.insert(
                    "x-api-key",
                    HeaderValue::from_str(key).map_err(|e| {
                        ModelFetchFailure::new(ERROR_GENERIC, "apiKey is invalid")
                            .with_detail(e.to_string())
                            .into_error()
                    })?,
                );
                headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
            }
        }
    }

    Ok(headers)
}

async fn fetch_discovered_models_from_url(
    client: &reqwest::Client,
    url: &str,
    headers: &HeaderMap,
) -> Result<Vec<DiscoveredModel>, ModelFetchFailure> {
    let response = client
        .get(url)
        .headers(headers.clone())
        .timeout(Duration::from_secs(DISCOVERY_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ModelFetchFailure::new(ERROR_TIMEOUT, "Model discovery request timed out")
                    .with_endpoint(url)
                    .with_detail(e.to_string())
            } else {
                ModelFetchFailure::new(ERROR_GENERIC, "Model discovery request failed")
                    .with_endpoint(url)
                    .with_detail(e.to_string())
            }
        })?;

    let status = response.status();
    let payload_text = response.text().await.map_err(|e| {
        ModelFetchFailure::new(ERROR_GENERIC, "Model discovery request failed")
            .with_endpoint(url)
            .with_detail(e.to_string())
    })?;
    if !status.is_success() {
        return Err(ModelFetchFailure::from_status(status.as_u16())
            .with_endpoint(url)
            .with_detail(payload_text));
    }

    let payload = serde_json::from_str::<Value>(&payload_text).map_err(|e| {
        ModelFetchFailure::new(ERROR_GENERIC, "Model discovery response is not valid JSON")
            .with_endpoint(url)
            .with_detail(e.to_string())
    })?;

    parse_discovered_models_from_payload(&payload).map_err(|failure| failure.with_endpoint(url))
}

fn parse_discovered_models_from_payload(
    payload: &Value,
) -> Result<Vec<DiscoveredModel>, ModelFetchFailure> {
    let items = payload
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| payload.get("models").and_then(Value::as_array))
        .or_else(|| payload.as_array())
        .ok_or_else(|| {
            ModelFetchFailure::new(
                ERROR_INVALID_MODEL_ARRAY,
                "Model response does not contain data/models array",
            )
        })?;

    let mut dedup = HashSet::new();
    let mut models = Vec::new();
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if id.is_empty() || !dedup.insert(id.clone()) {
            continue;
        }

        let name = item
            .get("display_name")
            .and_then(Value::as_str)
            .or_else(|| item.get("name").and_then(Value::as_str))
            .unwrap_or(&id)
            .trim()
            .to_string();
        let provider = item
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_string);
        let context_window = item
            .get("context_window")
            .and_then(Value::as_u64)
            .or_else(|| item.get("contextWindow").and_then(Value::as_u64))
            .or_else(|| item.get("input_token_limit").and_then(Value::as_u64));
        let owned_by = item
            .get("owned_by")
            .and_then(Value::as_str)
            .map(str::to_string);

        models.push(DiscoveredModel {
            id,
            name,
            provider,
            context_window,
            owned_by,
        });
    }

    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_models_url_basic() {
        assert_eq!(
            build_models_url("https://api.siliconflow.cn", false).unwrap(),
            "https://api.siliconflow.cn/v1/models"
        );
    }

    #[test]
    fn test_build_models_url_trailing_slash() {
        assert_eq!(
            build_models_url("https://api.example.com/", false).unwrap(),
            "https://api.example.com/v1/models"
        );
    }

    #[test]
    fn test_build_models_url_with_v1() {
        assert_eq!(
            build_models_url("https://api.example.com/v1", false).unwrap(),
            "https://api.example.com/v1/models"
        );
    }

    #[test]
    fn test_build_models_url_full_url() {
        assert_eq!(
            build_models_url("https://proxy.example.com/v1/chat/completions", true).unwrap(),
            "https://proxy.example.com/v1/models"
        );
    }

    #[test]
    fn test_build_models_url_empty() {
        assert!(build_models_url("", false).is_err());
    }

    #[test]
    fn test_parse_response() {
        let json = r#"{"object":"list","data":[{"id":"gpt-4","object":"model","owned_by":"openai"},{"id":"claude-3-sonnet","object":"model","owned_by":"anthropic"}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        let data = resp.data.unwrap();
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].id, "gpt-4");
        assert_eq!(data[0].owned_by.as_deref(), Some("openai"));
        assert_eq!(data[1].id, "claude-3-sonnet");
    }

    #[test]
    fn test_parse_response_no_owned_by() {
        let json = r#"{"object":"list","data":[{"id":"my-model","object":"model"}]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        let data = resp.data.unwrap();
        assert_eq!(data[0].id, "my-model");
        assert!(data[0].owned_by.is_none());
    }

    #[test]
    fn test_parse_response_empty_data() {
        let json = r#"{"object":"list","data":[]}"#;
        let resp: ModelsResponse = serde_json::from_str(json).unwrap();
        assert!(resp.data.unwrap().is_empty());
    }

    #[test]
    fn test_build_candidate_urls_with_root_endpoint() {
        let base = Url::parse("https://api.example.com").unwrap();
        assert_eq!(
            build_candidate_urls(&base),
            vec![
                "https://api.example.com/models".to_string(),
                "https://api.example.com/v1/models".to_string()
            ]
        );
    }

    #[test]
    fn test_build_candidate_urls_with_v1_endpoint() {
        let base = Url::parse("https://api.example.com/v1").unwrap();
        assert_eq!(
            build_candidate_urls(&base),
            vec![
                "https://api.example.com/v1/models".to_string(),
                "https://api.example.com/models".to_string()
            ]
        );
    }

    #[test]
    fn test_parse_base_url_returns_structured_missing_endpoint_code() {
        let error = parse_base_url("").expect_err("empty base url should fail");
        let payload: Value = serde_json::from_str(&error).unwrap();
        assert_eq!(
            payload.get("code").and_then(Value::as_str),
            Some(ERROR_MISSING_ENDPOINT)
        );
    }

    #[test]
    fn test_parse_base_url_rejects_non_http_scheme() {
        let error = parse_base_url("file:///tmp/models").expect_err("file URL should fail");
        let payload: Value = serde_json::from_str(&error).unwrap();
        assert_eq!(
            payload.get("code").and_then(Value::as_str),
            Some(ERROR_INVALID_URL)
        );
        assert!(payload
            .get("context")
            .and_then(|context| context.get("detail"))
            .and_then(Value::as_str)
            .is_some_and(|detail| detail.contains("unsupported URL scheme: file")));
    }

    #[test]
    fn test_parse_discovered_models_from_data_array() {
        let payload = json!({
            "data": [
                {
                    "id": "gpt-4o",
                    "name": "GPT-4o",
                    "provider": "openai",
                    "context_window": 128000,
                    "owned_by": "openai"
                },
                {
                    "id": "gpt-4o",
                    "name": "duplicate"
                }
            ]
        });

        let models = parse_discovered_models_from_payload(&payload).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "gpt-4o");
        assert_eq!(models[0].name, "GPT-4o");
        assert_eq!(models[0].provider.as_deref(), Some("openai"));
        assert_eq!(models[0].context_window, Some(128000));
        assert_eq!(models[0].owned_by.as_deref(), Some("openai"));
    }

    #[test]
    fn test_parse_discovered_models_from_models_array() {
        let payload = json!({
            "models": [
                {
                    "id": "claude-sonnet-4",
                    "display_name": "Claude Sonnet 4",
                    "input_token_limit": 200000
                }
            ]
        });

        let models = parse_discovered_models_from_payload(&payload).unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "Claude Sonnet 4");
        assert_eq!(models[0].context_window, Some(200000));
    }

    #[test]
    fn test_parse_discovered_models_returns_invalid_model_array() {
        let payload = json!({ "data": { "id": "not-array" } });
        let error = parse_discovered_models_from_payload(&payload).unwrap_err();
        assert_eq!(error.code, ERROR_INVALID_MODEL_ARRAY);
    }
}
