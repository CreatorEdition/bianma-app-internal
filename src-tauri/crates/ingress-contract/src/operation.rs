//! RouteSpec 注册表、严格 preflight 与原始请求摘要。

use std::collections::HashSet;

use sha2::{Digest as _, Sha256};

use crate::{
    BodyDigest, HttpMethod, IngressReject, OperationId, RawIngressRequest, RegistryDigest,
    RequestDigest, SemanticHeadersDigest, MAX_HEADER_NAME_BYTES, MAX_RAW_BODY_BYTES,
};

const REGISTRY_DIGEST_DOMAIN: &[u8] = b"bianma.ingress.route-registry.v1\0";
const BODY_DIGEST_DOMAIN: &[u8] = b"bianma.ingress.raw-body.v1\0";
const HEADER_DIGEST_DOMAIN: &[u8] = b"bianma.ingress.semantic-headers.v1\0";
const REQUEST_DIGEST_DOMAIN: &[u8] = b"bianma.ingress.raw-request.v1\0";

/// 请求的唯一分发域。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestDispatchDomain {
    /// 只进入匹配的本地 handler。
    Local,
    /// 只进入固定单部署且禁止 fallback 的能力计划。
    BoundDeployment,
    /// 进入显式 RoutePolicy 的普通或辅助路由。
    RoutedPolicy,
}

impl RequestDispatchDomain {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Local => 1,
            Self::BoundDeployment => 2,
            Self::RoutedPolicy => 3,
        }
    }

    pub(crate) fn from_code(code: u8) -> Result<Self, IngressReject> {
        match code {
            1 => Ok(Self::Local),
            2 => Ok(Self::BoundDeployment),
            3 => Ok(Self::RoutedPolicy),
            _ => Err(IngressReject::ProofMalformed),
        }
    }
}

/// 首个合同切片支持的请求语义闭集。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RequestKind {
    /// 无数据库/Secret 的最小存活探测。
    Liveness,
    /// 已鉴权的本地管理操作。
    LocalAdmin,
    /// 专用 OAuth/Token 认证流程。
    AuthFlow,
    /// 从本地快照合成的统一模型目录。
    UnifiedModelCatalog,
    /// 本地 tokenizer 的精确计数。
    ExactLocalTokenCount,
    /// 本地估算计数。
    EstimatedLocalTokenCount,
    /// 纯本地 Context compact。
    LocalContextCompact,
    /// 固定单部署、无正文的管理模型探测。
    DeploymentModelProbe,
    /// 与真实请求同部署的远程精确计数。
    ExactUpstreamTokenCount,
    /// 普通模型推理。
    ModelInference,
    /// 独立 RoutePolicy 的辅助推理。
    AuxiliaryInference,
}

/// 本地 Operation 必须绑定的最小授权 scope。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LocalOperationAuthScope {
    /// 仅允许最小 `/health` handler。
    PublicLiveness,
    /// 允许模型目录、本地计数和本地 compact。
    LocalData,
    /// 允许本地管理面。
    LocalAdmin,
    /// 允许专用认证流程。
    AuthFlow,
}

impl LocalOperationAuthScope {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::PublicLiveness => 1,
            Self::LocalData => 2,
            Self::LocalAdmin => 3,
            Self::AuthFlow => 4,
        }
    }
}

impl RequestKind {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Liveness => 1,
            Self::LocalAdmin => 2,
            Self::AuthFlow => 3,
            Self::UnifiedModelCatalog => 4,
            Self::ExactLocalTokenCount => 5,
            Self::EstimatedLocalTokenCount => 6,
            Self::LocalContextCompact => 7,
            Self::DeploymentModelProbe => 8,
            Self::ExactUpstreamTokenCount => 9,
            Self::ModelInference => 10,
            Self::AuxiliaryInference => 11,
        }
    }
}

/// Query 是否属于已注册 Operation 的语义。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QueryPolicy {
    /// 拒绝任何 query。
    Forbidden,
    /// 允许 query，并将原始 target 纳入请求摘要。
    Allowed,
}

/// Operation 对正文和 Content-Type 的硬约束。
#[derive(Clone, Eq, PartialEq)]
pub enum BodyPolicy {
    /// Operation 不接受正文。
    Forbidden,
    /// Operation 接受受限正文。
    Bounded {
        /// 正文逐字节最大长度。
        max_bytes: usize,
        /// 可选的必需媒体类型，不含参数。
        required_content_type: Option<Vec<u8>>,
    },
}

impl BodyPolicy {
    /// 构造有界正文策略。Content-Type 使用 ASCII 小写媒体类型，例如 `application/json`。
    pub fn bounded(
        max_bytes: usize,
        required_content_type: Option<&[u8]>,
    ) -> Result<Self, IngressReject> {
        let content_type = required_content_type
            .map(normalize_declared_media_type)
            .transpose()?;
        let policy = Self::Bounded {
            max_bytes,
            required_content_type: content_type,
        };
        validate_body_policy(&policy)?;
        Ok(policy)
    }
}

/// 只读、精确匹配的 Operation 规格。
#[derive(Clone, Eq, PartialEq)]
pub struct RouteSpec {
    pub(crate) operation: OperationId,
    pub(crate) method: HttpMethod,
    pub(crate) path: Vec<u8>,
    pub(crate) query_policy: QueryPolicy,
    pub(crate) body_policy: BodyPolicy,
    pub(crate) kind: RequestKind,
    pub(crate) dispatch_domain: RequestDispatchDomain,
    pub(crate) semantic_headers: Vec<Vec<u8>>,
    pub(crate) local_scope: Option<LocalOperationAuthScope>,
}

impl RouteSpec {
    /// 构造 RouteSpec；非法 RequestKind×DispatchDomain 组合会被拒绝。
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        operation: OperationId,
        method: HttpMethod,
        path: &[u8],
        query_policy: QueryPolicy,
        body_policy: BodyPolicy,
        kind: RequestKind,
        dispatch_domain: RequestDispatchDomain,
        semantic_headers: Vec<Vec<u8>>,
        local_scope: Option<LocalOperationAuthScope>,
    ) -> Result<Self, IngressReject> {
        validate_registered_path(path)?;
        validate_body_policy(&body_policy)?;
        validate_kind_domain(kind, dispatch_domain, local_scope)?;

        let mut normalized_headers = Vec::with_capacity(semantic_headers.len());
        for header in semantic_headers {
            if header.is_empty()
                || header.len() > MAX_HEADER_NAME_BYTES
                || !header
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            {
                return Err(IngressReject::RouteSpecInvalid);
            }
            let normalized = header
                .into_iter()
                .map(|byte| byte.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if matches!(
                normalized.as_slice(),
                b"authorization"
                    | b"proxy-authorization"
                    | b"x-api-key"
                    | b"x-goog-api-key"
                    | b"api-key"
                    | b"x-auth-token"
                    | b"cookie"
                    | b"set-cookie"
                    | b"connection"
                    | b"transfer-encoding"
                    | b"host"
                    | b"forwarded"
                    | b"x-forwarded-host"
                    | b"x-forwarded-proto"
                    | b"x-upstream"
                    | b"x-base-url"
                    | b"x-target"
                    | b"x-endpoint"
                    | b"content-length"
                    | b"x-bianma-context-attestation"
                    | b"x-bianma-authorization-bundle"
                    | b"x-bianma-capability-authorization"
            ) {
                return Err(IngressReject::RouteSpecInvalid);
            }
            normalized_headers.push(normalized);
        }
        normalized_headers.sort();
        normalized_headers.dedup();

        Ok(Self {
            operation,
            method,
            path: path.to_vec(),
            query_policy,
            body_policy,
            kind,
            dispatch_domain,
            semantic_headers: normalized_headers,
            local_scope,
        })
    }

    /// 返回 Operation ID。
    pub const fn operation(&self) -> OperationId {
        self.operation
    }

    /// 返回请求语义。
    pub const fn kind(&self) -> RequestKind {
        self.kind
    }

    /// 返回唯一分发域。
    pub const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.dispatch_domain
    }
}

/// 同时供受信 matcher 与 Gateway verifier 使用的不可变注册表。
pub struct RouteSpecRegistry {
    specs: Vec<RouteSpec>,
    digest: RegistryDigest,
}

impl RouteSpecRegistry {
    /// 编译并摘要注册表。Operation ID 与 method+path 必须唯一。
    pub fn compile(mut specs: Vec<RouteSpec>) -> Result<Self, IngressReject> {
        if specs.is_empty() {
            return Err(IngressReject::RegistryInvalid);
        }
        specs.sort_by_key(|spec| spec.operation);

        let mut operation_ids = HashSet::with_capacity(specs.len());
        let mut routes = HashSet::with_capacity(specs.len());
        for spec in &specs {
            if !operation_ids.insert(spec.operation)
                || !routes.insert((spec.method, spec.path.clone()))
            {
                return Err(IngressReject::RegistryInvalid);
            }
        }

        let digest = RegistryDigest::from_bytes(hash_registry(&specs));
        Ok(Self { specs, digest })
    }

    /// 返回固定注册表摘要。
    pub const fn digest(&self) -> RegistryDigest {
        self.digest
    }

    pub(crate) fn bind_request(
        &self,
        request: &RawIngressRequest,
    ) -> Result<RequestBinding, IngressReject> {
        let path_candidates = self
            .specs
            .iter()
            .filter(|spec| spec.path.as_slice() == request.path())
            .collect::<Vec<_>>();
        if path_candidates.is_empty() {
            return Err(IngressReject::RouteNotFound);
        }
        let spec = path_candidates
            .into_iter()
            .find(|spec| spec.method == request.method())
            .ok_or(IngressReject::MethodNotAllowed)?;

        if request.has_query() && spec.query_policy == QueryPolicy::Forbidden {
            return Err(IngressReject::QueryNotAllowed);
        }
        validate_request_body(spec, request)?;

        let body_digest = BodyDigest::from_bytes(hash_parts(BODY_DIGEST_DOMAIN, &[request.body()]));
        let semantic_headers_digest = SemanticHeadersDigest::from_bytes(hash_semantic_headers(
            request,
            &spec.semantic_headers,
        ));
        let body_length =
            u64::try_from(request.body().len()).map_err(|_| IngressReject::RequestTooLarge)?;
        let request_digest = RequestDigest::from_bytes(hash_request(
            request,
            body_digest,
            semantic_headers_digest,
            body_length,
        ));

        Ok(RequestBinding {
            operation: MatchedOperation { spec: spec.clone() },
            body_digest,
            semantic_headers_digest,
            request_digest,
            body_length,
        })
    }
}

pub(crate) struct MatchedOperation {
    pub(crate) spec: RouteSpec,
}

pub(crate) struct RequestBinding {
    pub(crate) operation: MatchedOperation,
    pub(crate) body_digest: BodyDigest,
    pub(crate) semantic_headers_digest: SemanticHeadersDigest,
    pub(crate) request_digest: RequestDigest,
    pub(crate) body_length: u64,
}

fn validate_registered_path(path: &[u8]) -> Result<(), IngressReject> {
    if path.is_empty()
        || path[0] != b'/'
        || path.contains(&b'?')
        || path.contains(&b'#')
        || path.contains(&b'\\')
        || path
            .iter()
            .any(|byte| !byte.is_ascii() || byte.is_ascii_control() || *byte == b' ')
    {
        return Err(IngressReject::RouteSpecInvalid);
    }
    for segment in path.split(|byte| *byte == b'/') {
        if segment == b"." || segment == b".." {
            return Err(IngressReject::RouteSpecInvalid);
        }
    }
    let lower = path.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    if lower
        .windows(3)
        .any(|window| matches!(window, b"%2f" | b"%5c" | b"%2e" | b"%25" | b"%00"))
    {
        return Err(IngressReject::RouteSpecInvalid);
    }
    Ok(())
}

fn validate_body_policy(policy: &BodyPolicy) -> Result<(), IngressReject> {
    match policy {
        BodyPolicy::Forbidden => Ok(()),
        BodyPolicy::Bounded {
            max_bytes,
            required_content_type,
        } => {
            if *max_bytes == 0 || *max_bytes > MAX_RAW_BODY_BYTES {
                return Err(IngressReject::RouteSpecInvalid);
            }
            if let Some(content_type) = required_content_type {
                if normalize_declared_media_type(content_type)? != *content_type {
                    return Err(IngressReject::RouteSpecInvalid);
                }
            }
            Ok(())
        }
    }
}

fn validate_kind_domain(
    kind: RequestKind,
    domain: RequestDispatchDomain,
    local_scope: Option<LocalOperationAuthScope>,
) -> Result<(), IngressReject> {
    let valid = match kind {
        RequestKind::Liveness
        | RequestKind::LocalAdmin
        | RequestKind::AuthFlow
        | RequestKind::UnifiedModelCatalog
        | RequestKind::ExactLocalTokenCount
        | RequestKind::EstimatedLocalTokenCount
        | RequestKind::LocalContextCompact => domain == RequestDispatchDomain::Local,
        RequestKind::DeploymentModelProbe | RequestKind::ExactUpstreamTokenCount => {
            domain == RequestDispatchDomain::BoundDeployment
        }
        RequestKind::ModelInference | RequestKind::AuxiliaryInference => {
            domain == RequestDispatchDomain::RoutedPolicy
        }
    };
    if !valid || (domain == RequestDispatchDomain::Local) != local_scope.is_some() {
        return Err(IngressReject::RouteSpecInvalid);
    }
    if kind == RequestKind::Liveness && local_scope != Some(LocalOperationAuthScope::PublicLiveness)
    {
        return Err(IngressReject::RouteSpecInvalid);
    }
    if kind == RequestKind::LocalAdmin && local_scope != Some(LocalOperationAuthScope::LocalAdmin) {
        return Err(IngressReject::RouteSpecInvalid);
    }
    if kind == RequestKind::AuthFlow && local_scope != Some(LocalOperationAuthScope::AuthFlow) {
        return Err(IngressReject::RouteSpecInvalid);
    }
    if matches!(
        kind,
        RequestKind::UnifiedModelCatalog
            | RequestKind::ExactLocalTokenCount
            | RequestKind::EstimatedLocalTokenCount
            | RequestKind::LocalContextCompact
    ) && local_scope != Some(LocalOperationAuthScope::LocalData)
    {
        return Err(IngressReject::RouteSpecInvalid);
    }
    Ok(())
}

fn validate_request_body(
    spec: &RouteSpec,
    request: &RawIngressRequest,
) -> Result<(), IngressReject> {
    match &spec.body_policy {
        BodyPolicy::Forbidden if !request.body().is_empty() => Err(IngressReject::BodyNotAllowed),
        BodyPolicy::Forbidden => Ok(()),
        BodyPolicy::Bounded {
            max_bytes,
            required_content_type,
        } => {
            if request.body().len() > *max_bytes {
                return Err(IngressReject::BodyLimitExceeded);
            }
            if let Some(expected) = required_content_type {
                let actual = request
                    .content_type()
                    .ok_or(IngressReject::ContentTypeNotAllowed)?;
                if normalize_actual_media_type(actual)? != *expected {
                    return Err(IngressReject::ContentTypeNotAllowed);
                }
            }
            Ok(())
        }
    }
}

fn normalize_declared_media_type(value: &[u8]) -> Result<Vec<u8>, IngressReject> {
    if value.is_empty()
        || !value
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'+' | b'.'))
        || !value.contains(&b'/')
    {
        return Err(IngressReject::RouteSpecInvalid);
    }
    Ok(value.iter().map(u8::to_ascii_lowercase).collect())
}

fn normalize_actual_media_type(value: &[u8]) -> Result<Vec<u8>, IngressReject> {
    let media_type = value
        .splitn(2, |byte| *byte == b';')
        .next()
        .unwrap_or_default();
    let trimmed = trim_ascii_ows(media_type);
    if trimmed.is_empty()
        || !trimmed
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'+' | b'.'))
    {
        return Err(IngressReject::ContentTypeNotAllowed);
    }
    Ok(trimmed.iter().map(u8::to_ascii_lowercase).collect())
}

fn trim_ascii_ows(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|byte| !matches!(byte, b' ' | b'\t'))
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|byte| !matches!(byte, b' ' | b'\t'))
        .map_or(start, |index| index + 1);
    &value[start..end]
}

fn hash_registry(specs: &[RouteSpec]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(REGISTRY_DIGEST_DOMAIN);
    update_len(&mut hasher, specs.len());
    for spec in specs {
        hasher.update(spec.operation.get().to_be_bytes());
        hasher.update([spec.method.code()]);
        update_bytes(&mut hasher, &spec.path);
        hasher.update([match spec.query_policy {
            QueryPolicy::Forbidden => 0,
            QueryPolicy::Allowed => 1,
        }]);
        match &spec.body_policy {
            BodyPolicy::Forbidden => hasher.update([0]),
            BodyPolicy::Bounded {
                max_bytes,
                required_content_type,
            } => {
                hasher.update([1]);
                hasher.update((*max_bytes as u64).to_be_bytes());
                if let Some(content_type) = required_content_type {
                    hasher.update([1]);
                    update_bytes(&mut hasher, content_type);
                } else {
                    hasher.update([0]);
                }
            }
        }
        hasher.update([spec.kind.code(), spec.dispatch_domain.code()]);
        hasher.update([spec.local_scope.map_or(0, LocalOperationAuthScope::code)]);
        update_len(&mut hasher, spec.semantic_headers.len());
        for header in &spec.semantic_headers {
            update_bytes(&mut hasher, header);
        }
    }
    hasher.finalize().into()
}

fn hash_semantic_headers(request: &RawIngressRequest, allowlist: &[Vec<u8>]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(HEADER_DIGEST_DOMAIN);
    for header in &request.headers {
        let selected = header.name == b"content-type"
            || allowlist
                .iter()
                .any(|allowed| allowed.as_slice() == header.name);
        if selected {
            update_bytes(&mut hasher, &header.name);
            update_bytes(&mut hasher, &header.value);
        }
    }
    hasher.finalize().into()
}

fn hash_request(
    request: &RawIngressRequest,
    body_digest: BodyDigest,
    semantic_headers_digest: SemanticHeadersDigest,
    body_length: u64,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(REQUEST_DIGEST_DOMAIN);
    hasher.update([request.method.code()]);
    update_bytes(&mut hasher, request.target());
    hasher.update(semantic_headers_digest.as_bytes());
    hasher.update(body_digest.as_bytes());
    hasher.update(body_length.to_be_bytes());
    hasher.finalize().into()
}

fn hash_parts(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_bytes(&mut hasher, part);
    }
    hasher.finalize().into()
}

fn update_len(hasher: &mut Sha256, len: usize) {
    hasher.update((len as u64).to_be_bytes());
}

fn update_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    update_len(hasher, bytes.len());
    hasher.update(bytes);
}
