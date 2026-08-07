//! 稳定、脱敏且 fail-closed 的拒绝码。

use thiserror::Error;

/// 入站验证拒绝。
///
/// 所有 `Display` 文本都是常量，不包含请求正文、路径、Header、证明或密钥。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum IngressReject {
    /// 请求 target、长度或 framing 不合法。
    #[error("request_malformed")]
    RequestMalformed,
    /// 请求或证明超过硬上限。
    #[error("request_too_large")]
    RequestTooLarge,
    /// Header 名称、值或重复关系不合法。
    #[error("header_malformed")]
    HeaderMalformed,
    /// 同一请求携带冲突的入站认证材料。
    #[error("conflicting_inbound_authentication")]
    ConflictingInboundAuthentication,
    /// 原始请求混入只能由 Gateway 单独承载的证明 Header。
    #[error("reserved_proof_header")]
    ReservedProofHeader,
    /// 精确路径未注册。
    #[error("route_not_found")]
    RouteNotFound,
    /// 路径存在但方法未注册。
    #[error("method_not_allowed")]
    MethodNotAllowed,
    /// 当前 Operation 禁止 query。
    #[error("query_not_allowed")]
    QueryNotAllowed,
    /// Content-Type 与 RouteSpec 不一致。
    #[error("content_type_not_allowed")]
    ContentTypeNotAllowed,
    /// 当前 Operation 禁止正文。
    #[error("body_not_allowed")]
    BodyNotAllowed,
    /// 正文超过 Operation 上限。
    #[error("body_limit_exceeded")]
    BodyLimitExceeded,
    /// RouteSpec 自身违反闭集约束。
    #[error("route_spec_invalid")]
    RouteSpecInvalid,
    /// 注册表为空、重复或冲突。
    #[error("registry_invalid")]
    RegistryInvalid,
    /// Attestation canonical wire 不合法。
    #[error("proof_malformed")]
    ProofMalformed,
    /// Authorization bundle canonical wire 不合法。
    #[error("authorization_bundle_malformed")]
    AuthorizationBundleMalformed,
    /// Capability authorization canonical wire 不合法。
    #[error("capability_authorization_malformed")]
    CapabilityAuthorizationMalformed,
    /// HMAC 验证失败。
    #[error("mac_invalid")]
    MacInvalid,
    /// 签发 epoch 不在当前轮换窗口。
    #[error("issuer_epoch_unknown")]
    IssuerEpochUnknown,
    /// listener、Token 或管理 scope 不一致。
    #[error("scope_mismatch")]
    ScopeMismatch,
    /// 证明 audience 不一致。
    #[error("audience_mismatch")]
    AudienceMismatch,
    /// 注册表摘要不一致。
    #[error("registry_mismatch")]
    RegistryMismatch,
    /// Operation ID 不一致。
    #[error("operation_mismatch")]
    OperationMismatch,
    /// RequestDispatchDomain 不一致。
    #[error("dispatch_domain_mismatch")]
    DispatchDomainMismatch,
    /// 原始 method/target/Header/body 绑定不一致。
    #[error("request_binding_mismatch")]
    RequestBindingMismatch,
    /// Permit、requirements 或 bundle 摘要绑定不一致。
    #[error("authorization_binding_mismatch")]
    AuthorizationBindingMismatch,
    /// 当前激活快照与证明不一致。
    #[error("activation_binding_mismatch")]
    ActivationBindingMismatch,
    /// 签发、过期或 TTL 窗口非法。
    #[error("time_window_invalid")]
    TimeWindowInvalid,
    /// nonce 是全零或格式非法。
    #[error("invalid_nonce")]
    InvalidNonce,
    /// nonce 重放、容量耗尽或存储故障。
    #[error("nonce_rejected")]
    NonceRejected,
    /// GatewayOnly consent 缺失、过期或非法。
    #[error("gateway_consent_invalid")]
    GatewayConsentInvalid,
    /// 本地 Operation auth scope 不一致。
    #[error("local_scope_mismatch")]
    LocalScopeMismatch,
    /// Capability 精确绑定或 fallback 约束不一致。
    #[error("capability_constraint_mismatch")]
    CapabilityConstraintMismatch,
    /// 证明入口与请求语义发生模式冲突。
    #[error("proof_mode_conflict")]
    ProofModeConflict,
    /// context、activation、consent 或请求来自其他 verifier runtime。
    #[error("verification_domain_mismatch")]
    VerificationDomainMismatch,
    /// 时钟不可用。
    #[error("clock_unavailable")]
    ClockUnavailable,
    /// 内部安全前置条件失败并已关闭执行。
    #[error("internal_fail_closed")]
    InternalFailClosed,
}

impl IngressReject {
    /// 返回适合协议层映射的稳定错误码。
    pub const fn code(self) -> &'static str {
        match self {
            Self::RequestMalformed => "request_malformed",
            Self::RequestTooLarge => "request_too_large",
            Self::HeaderMalformed => "header_malformed",
            Self::ConflictingInboundAuthentication => "conflicting_inbound_authentication",
            Self::ReservedProofHeader => "reserved_proof_header",
            Self::RouteNotFound => "route_not_found",
            Self::MethodNotAllowed => "method_not_allowed",
            Self::QueryNotAllowed => "query_not_allowed",
            Self::ContentTypeNotAllowed => "content_type_not_allowed",
            Self::BodyNotAllowed => "body_not_allowed",
            Self::BodyLimitExceeded => "body_limit_exceeded",
            Self::RouteSpecInvalid => "route_spec_invalid",
            Self::RegistryInvalid => "registry_invalid",
            Self::ProofMalformed => "proof_malformed",
            Self::AuthorizationBundleMalformed => "authorization_bundle_malformed",
            Self::CapabilityAuthorizationMalformed => "capability_authorization_malformed",
            Self::MacInvalid => "mac_invalid",
            Self::IssuerEpochUnknown => "issuer_epoch_unknown",
            Self::ScopeMismatch => "scope_mismatch",
            Self::AudienceMismatch => "audience_mismatch",
            Self::RegistryMismatch => "registry_mismatch",
            Self::OperationMismatch => "operation_mismatch",
            Self::DispatchDomainMismatch => "dispatch_domain_mismatch",
            Self::RequestBindingMismatch => "request_binding_mismatch",
            Self::AuthorizationBindingMismatch => "authorization_binding_mismatch",
            Self::ActivationBindingMismatch => "activation_binding_mismatch",
            Self::TimeWindowInvalid => "time_window_invalid",
            Self::InvalidNonce => "invalid_nonce",
            Self::NonceRejected => "nonce_rejected",
            Self::GatewayConsentInvalid => "gateway_consent_invalid",
            Self::LocalScopeMismatch => "local_scope_mismatch",
            Self::CapabilityConstraintMismatch => "capability_constraint_mismatch",
            Self::ProofModeConflict => "proof_mode_conflict",
            Self::VerificationDomainMismatch => "verification_domain_mismatch",
            Self::ClockUnavailable => "clock_unavailable",
            Self::InternalFailClosed => "internal_fail_closed",
        }
    }
}

/// 原子 nonce store 的脱敏失败类型。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum NonceReject {
    /// nonce 已被同一 namespace/epoch 消费。
    #[error("nonce_replayed")]
    Replayed,
    /// nonce 的期限不晚于不可回拨 high-water。
    #[error("nonce_expired")]
    Expired,
    /// 有界 store 已满且没有可清理的过期项。
    #[error("nonce_capacity_exhausted")]
    CapacityExhausted,
    /// nonce store 锁或后端不可用。
    #[error("nonce_store_unavailable")]
    StoreUnavailable,
}
