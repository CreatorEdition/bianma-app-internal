//! 字段私有、不可反序列化且只能由 verifier 构造的 Verified 请求。

use std::sync::Arc;

use crate::{
    operation::MatchedOperation,
    request::SanitizedRequest,
    signed::{AuthorizationBundle, CapabilityClaims},
    AccountId, AccountSelectorId, AdapterContractRevision, AuthorizationBundleDigest,
    CanonicalOrigin, CapabilityManagementScopeId, ConsentRevision, CredentialId, EndpointId,
    HttpMethod, IngressTokenScopeId, IssuerEpoch, ListenerId, LocalOperationAuthScope,
    ModelDeploymentId, OneShotNonce, OperationId, RegistryDigest, RequestDigest,
    RequestDispatchDomain, RequestKind, RoutePolicyRevision,
};

pub(crate) struct VerificationDomainSeal {
    _private: (),
}

impl VerificationDomainSeal {
    pub(crate) const fn new() -> Self {
        Self { _private: () }
    }
}

/// 生产 classifier 必须持有的实例绑定接收端。
///
/// 它只接受同一 composition-root runtime 产生的 Verified 请求；由其他 verifier 实例
/// 生成的请求即使密码学上自洽也会被拒绝。
pub struct VerifiedIngressReceiver {
    domain_seal: Arc<VerificationDomainSeal>,
}

impl VerifiedIngressReceiver {
    pub(crate) fn new(domain_seal: Arc<VerificationDomainSeal>) -> Self {
        Self { domain_seal }
    }

    /// 消费并确认请求属于当前生产验证域。
    pub fn accept(
        &self,
        request: VerifiedIngressRequest,
    ) -> Result<VerifiedIngressRequest, crate::IngressReject> {
        if Arc::ptr_eq(&self.domain_seal, &request.domain_seal) {
            Ok(request)
        } else {
            Err(crate::IngressReject::VerificationDomainMismatch)
        }
    }
}

/// 已验证证明的只读判别值；它本身不能构造 Verified 请求。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedProofKind {
    /// 已验证完整 Attestation 与授权 bundle。
    Managed,
    /// 已绑定独立 listener/token/consent 的 GatewayOnly。
    GatewayOnlyScopedConsent,
    /// 已绑定本地 Operation scope。
    LocalOperationScoped,
    /// 已绑定固定部署/账户/凭据的管理能力。
    CapabilityScoped,
}

/// Verified 请求携带的授权约束种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedAuthorizationKind {
    /// 不含任何出站权限。
    None,
    /// 已验证 Managed egress bundle。
    ManagedEgress,
    /// 用户显式启用的受限 GatewayOnly 权限。
    GatewayOnlyExplicit,
    /// 固定单部署、禁止 fallback 的能力权限。
    CapabilityBound,
}

/// 已清理 Header 的只读视图。
///
/// 值可能包含用户输入，因此本类型不实现 `Debug`。
pub struct VerifiedHeaderRef<'a> {
    name: &'a [u8],
    value: &'a [u8],
}

impl<'a> VerifiedHeaderRef<'a> {
    /// 返回规范化名称。
    pub const fn name(&self) -> &'a [u8] {
        self.name
    }

    /// 返回原始值。
    pub const fn value(&self) -> &'a [u8] {
        self.value
    }
}

/// CapabilityScoped 精确绑定的只读视图。
///
/// 该视图只能从 [`VerifiedIngressRequest`] 借用，不能据此构造或提升权限。
pub struct VerifiedCapabilityBindingView<'a> {
    claims: &'a CapabilityClaims,
}

impl<'a> VerifiedCapabilityBindingView<'a> {
    /// 返回唯一模型部署。
    pub const fn deployment(&self) -> ModelDeploymentId {
        self.claims.deployment
    }

    /// 返回唯一 Endpoint。
    pub const fn endpoint(&self) -> EndpointId {
        self.claims.endpoint
    }

    /// 返回精确 canonical Origin。
    pub fn origin(&self) -> &CanonicalOrigin {
        &self.claims.origin
    }

    /// 返回唯一账户选择器。
    pub const fn account_selector(&self) -> AccountSelectorId {
        self.claims.account_selector
    }

    /// 返回唯一账户。
    pub const fn account(&self) -> AccountId {
        self.claims.account
    }

    /// 返回唯一逻辑凭据。
    pub const fn credential(&self) -> CredentialId {
        self.claims.credential
    }

    /// 返回锁定的 Adapter 合同修订。
    pub const fn adapter_contract_revision(&self) -> AdapterContractRevision {
        self.claims.adapter_contract_revision
    }

    /// 返回管理鉴权 scope。
    pub const fn management_scope(&self) -> CapabilityManagementScopeId {
        self.claims.management_scope
    }

    /// 返回绝对截止时间（Unix 毫秒）。
    pub const fn deadline_millis(&self) -> u64 {
        self.claims.deadline_millis
    }

    /// CapabilityScoped 永远禁止 fallback。
    pub const fn fallback_forbidden(&self) -> bool {
        self.claims.fallback_forbidden
    }
}

pub(crate) enum VerifiedProof {
    Managed {
        issuer_epoch: IssuerEpoch,
        nonce: OneShotNonce,
        authorization_bundle_digest: AuthorizationBundleDigest,
    },
    GatewayOnlyScopedConsent {
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        consent_revision: ConsentRevision,
        route_policy_revision: RoutePolicyRevision,
        registry_digest: RegistryDigest,
        request_digest: RequestDigest,
    },
    LocalOperationScoped {
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        auth_scope: LocalOperationAuthScope,
        request_digest: RequestDigest,
    },
    CapabilityScoped(CapabilityClaims),
}

pub(crate) enum VerifiedAuthorization {
    None,
    Managed(Box<AuthorizationBundle>),
    GatewayOnlyExplicit {
        consent_revision: ConsentRevision,
        route_policy_revision: RoutePolicyRevision,
        maximum_trust_tier: u8,
        local_handle_allowed: bool,
    },
    CapabilityBound,
}

/// 唯一能进入后续 classifier 的入站请求。
///
/// 字段私有；本类型不实现 `Clone`、`Copy`、`Debug`、`Default` 或任何 Serde trait。
pub struct VerifiedIngressRequest {
    domain_seal: Arc<VerificationDomainSeal>,
    operation: MatchedOperation,
    request: SanitizedRequest,
    proof: VerifiedProof,
    authorization: VerifiedAuthorization,
}

impl VerifiedIngressRequest {
    pub(crate) fn from_managed(
        domain_seal: Arc<VerificationDomainSeal>,
        operation: MatchedOperation,
        request: SanitizedRequest,
        issuer_epoch: IssuerEpoch,
        nonce: OneShotNonce,
        authorization_bundle_digest: AuthorizationBundleDigest,
        authorization_bundle: AuthorizationBundle,
    ) -> Self {
        Self {
            domain_seal,
            operation,
            request,
            proof: VerifiedProof::Managed {
                issuer_epoch,
                nonce,
                authorization_bundle_digest,
            },
            authorization: VerifiedAuthorization::Managed(Box::new(authorization_bundle)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_gateway_only(
        domain_seal: Arc<VerificationDomainSeal>,
        operation: MatchedOperation,
        request: SanitizedRequest,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        consent_revision: ConsentRevision,
        route_policy_revision: RoutePolicyRevision,
        registry_digest: RegistryDigest,
        request_digest: RequestDigest,
        maximum_trust_tier: u8,
    ) -> Self {
        Self {
            domain_seal,
            operation,
            request,
            proof: VerifiedProof::GatewayOnlyScopedConsent {
                listener,
                token_scope,
                consent_revision,
                route_policy_revision,
                registry_digest,
                request_digest,
            },
            authorization: VerifiedAuthorization::GatewayOnlyExplicit {
                consent_revision,
                route_policy_revision,
                maximum_trust_tier,
                local_handle_allowed: false,
            },
        }
    }

    pub(crate) fn from_local(
        domain_seal: Arc<VerificationDomainSeal>,
        operation: MatchedOperation,
        request: SanitizedRequest,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        auth_scope: LocalOperationAuthScope,
        request_digest: RequestDigest,
    ) -> Self {
        Self {
            domain_seal,
            operation,
            request,
            proof: VerifiedProof::LocalOperationScoped {
                listener,
                token_scope,
                auth_scope,
                request_digest,
            },
            authorization: VerifiedAuthorization::None,
        }
    }

    pub(crate) fn from_capability(
        domain_seal: Arc<VerificationDomainSeal>,
        operation: MatchedOperation,
        request: SanitizedRequest,
        claims: CapabilityClaims,
    ) -> Self {
        Self {
            domain_seal,
            operation,
            request,
            proof: VerifiedProof::CapabilityScoped(claims),
            authorization: VerifiedAuthorization::CapabilityBound,
        }
    }

    /// 返回已由 Gateway 重新匹配的 Operation ID。
    pub const fn operation(&self) -> OperationId {
        self.operation.spec.operation
    }

    /// 返回已注册请求语义。
    pub const fn request_kind(&self) -> RequestKind {
        self.operation.spec.kind
    }

    /// 返回唯一分发域。
    pub const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.operation.spec.dispatch_domain
    }

    /// 返回证明种类，不暴露可伪造 proof variant。
    pub const fn proof_kind(&self) -> VerifiedProofKind {
        match &self.proof {
            VerifiedProof::Managed { .. } => VerifiedProofKind::Managed,
            VerifiedProof::GatewayOnlyScopedConsent { .. } => {
                VerifiedProofKind::GatewayOnlyScopedConsent
            }
            VerifiedProof::LocalOperationScoped { .. } => VerifiedProofKind::LocalOperationScoped,
            VerifiedProof::CapabilityScoped(_) => VerifiedProofKind::CapabilityScoped,
        }
    }

    /// 返回授权约束种类。
    pub const fn authorization_kind(&self) -> VerifiedAuthorizationKind {
        match &self.authorization {
            VerifiedAuthorization::None => VerifiedAuthorizationKind::None,
            VerifiedAuthorization::Managed(_) => VerifiedAuthorizationKind::ManagedEgress,
            VerifiedAuthorization::GatewayOnlyExplicit { .. } => {
                VerifiedAuthorizationKind::GatewayOnlyExplicit
            }
            VerifiedAuthorization::CapabilityBound => VerifiedAuthorizationKind::CapabilityBound,
        }
    }

    /// 返回清理后的方法。
    pub const fn method(&self) -> HttpMethod {
        self.request.method
    }

    /// 返回已验证 target。
    pub fn target(&self) -> &[u8] {
        &self.request.target
    }

    /// 返回原始正文的只读借用。
    pub fn body(&self) -> &[u8] {
        &self.request.body
    }

    /// 遍历已移除入站认证、Content-Length 和证明材料的 Header。
    pub fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.request.headers.iter().map(|header| VerifiedHeaderRef {
            name: &header.name,
            value: &header.value,
        })
    }

    /// CapabilityScoped 成功时返回精确绑定视图。
    pub fn capability_binding(&self) -> Option<VerifiedCapabilityBindingView<'_>> {
        match &self.proof {
            VerifiedProof::CapabilityScoped(claims) => {
                Some(VerifiedCapabilityBindingView { claims })
            }
            _ => None,
        }
    }

    /// 返回证明绑定的 request digest；Managed 结果从已验 MAC 的 bundle 读取。
    pub fn request_digest(&self) -> RequestDigest {
        match &self.proof {
            VerifiedProof::Managed { .. } => match &self.authorization {
                VerifiedAuthorization::Managed(bundle) => bundle.permit.request_digest,
                _ => unreachable!(),
            },
            VerifiedProof::GatewayOnlyScopedConsent { request_digest, .. }
            | VerifiedProof::LocalOperationScoped { request_digest, .. } => *request_digest,
            VerifiedProof::CapabilityScoped(claims) => claims.request_digest,
        }
    }

    /// 返回 listener/token scope 绑定；Managed 与 Capability 由各自签名 claims 持有。
    pub const fn listener_scope(&self) -> Option<(ListenerId, IngressTokenScopeId)> {
        match &self.proof {
            VerifiedProof::GatewayOnlyScopedConsent {
                listener,
                token_scope,
                ..
            }
            | VerifiedProof::LocalOperationScoped {
                listener,
                token_scope,
                ..
            } => Some((*listener, *token_scope)),
            VerifiedProof::CapabilityScoped(claims) => Some((claims.listener, claims.token_scope)),
            VerifiedProof::Managed { .. } => None,
        }
    }

    /// 返回 Managed proof 的 key epoch。
    pub const fn managed_issuer_epoch(&self) -> Option<IssuerEpoch> {
        match &self.proof {
            VerifiedProof::Managed { issuer_epoch, .. } => Some(*issuer_epoch),
            _ => None,
        }
    }

    /// 返回 Managed proof 的一次性 nonce；调用方不得写入普通日志。
    pub const fn managed_nonce(&self) -> Option<&OneShotNonce> {
        match &self.proof {
            VerifiedProof::Managed { nonce, .. } => Some(nonce),
            _ => None,
        }
    }

    /// 返回整体授权 bundle 的已验证摘要；调用方不得写入普通日志。
    pub const fn managed_authorization_bundle_digest(&self) -> Option<&AuthorizationBundleDigest> {
        match &self.proof {
            VerifiedProof::Managed {
                authorization_bundle_digest,
                ..
            } => Some(authorization_bundle_digest),
            _ => None,
        }
    }

    /// 返回 GatewayOnly proof 的 consent/RoutePolicy/registry 绑定。
    pub const fn gateway_only_revisions(
        &self,
    ) -> Option<(ConsentRevision, RoutePolicyRevision, RegistryDigest)> {
        match &self.proof {
            VerifiedProof::GatewayOnlyScopedConsent {
                consent_revision,
                route_policy_revision,
                registry_digest,
                ..
            } => Some((*consent_revision, *route_policy_revision, *registry_digest)),
            _ => None,
        }
    }

    /// 返回 Local proof 的精确 auth scope。
    pub const fn local_auth_scope(&self) -> Option<LocalOperationAuthScope> {
        match &self.proof {
            VerifiedProof::LocalOperationScoped { auth_scope, .. } => Some(*auth_scope),
            _ => None,
        }
    }

    /// 返回 Managed Permit 的目标数量；目标内容只通过后续约束视图使用。
    pub fn managed_target_count(&self) -> Option<usize> {
        match &self.authorization {
            VerifiedAuthorization::Managed(bundle) => Some(bundle.permit.allowed_targets.len()),
            _ => None,
        }
    }

    /// 返回 GatewayOnly 本地 consent 允许的最高 TrustTier。
    pub const fn gateway_only_maximum_trust_tier(&self) -> Option<u8> {
        match &self.authorization {
            VerifiedAuthorization::GatewayOnlyExplicit {
                maximum_trust_tier, ..
            } => Some(*maximum_trust_tier),
            _ => None,
        }
    }

    /// 返回 GatewayOnly 授权约束自身的 revision 绑定。
    pub const fn gateway_only_constraint_revisions(
        &self,
    ) -> Option<(ConsentRevision, RoutePolicyRevision)> {
        match &self.authorization {
            VerifiedAuthorization::GatewayOnlyExplicit {
                consent_revision,
                route_policy_revision,
                ..
            } => Some((*consent_revision, *route_policy_revision)),
            _ => None,
        }
    }

    /// GatewayOnly 永远不能授予 LocalHandle/retrieval authority。
    pub fn local_handle_allowed(&self) -> bool {
        match &self.authorization {
            VerifiedAuthorization::Managed(bundle) => bundle.requirements.local_handle_required,
            VerifiedAuthorization::GatewayOnlyExplicit {
                local_handle_allowed,
                ..
            } => *local_handle_allowed,
            VerifiedAuthorization::None | VerifiedAuthorization::CapabilityBound => false,
        }
    }
}

#[cfg(test)]
impl VerifiedIngressRequest {
    pub(crate) fn proof_binding_is_nonzero(&self) -> bool {
        match &self.proof {
            VerifiedProof::Managed {
                issuer_epoch,
                nonce,
                authorization_bundle_digest,
            } => {
                issuer_epoch.get() != 0
                    && nonce.as_bytes() != &[0; 16]
                    && authorization_bundle_digest.as_bytes() != &[0; 32]
            }
            VerifiedProof::GatewayOnlyScopedConsent {
                listener,
                token_scope,
                consent_revision,
                route_policy_revision,
                registry_digest,
                request_digest,
            } => {
                listener.get() != 0
                    && token_scope.get() != 0
                    && consent_revision.get() != 0
                    && route_policy_revision.get() != 0
                    && registry_digest.as_bytes() != &[0; 32]
                    && request_digest.as_bytes() != &[0; 32]
            }
            VerifiedProof::LocalOperationScoped {
                listener,
                token_scope,
                auth_scope,
                request_digest,
            } => {
                let _ = auth_scope;
                listener.get() != 0
                    && token_scope.get() != 0
                    && request_digest.as_bytes() != &[0; 32]
            }
            VerifiedProof::CapabilityScoped(claims) => {
                claims.nonce.as_bytes() != &[0; 16] && claims.request_digest.as_bytes() != &[0; 32]
            }
        }
    }

    pub(crate) fn gateway_constraint_snapshot(&self) -> Option<(u64, u64, u8)> {
        match &self.authorization {
            VerifiedAuthorization::GatewayOnlyExplicit {
                consent_revision,
                route_policy_revision,
                maximum_trust_tier,
                ..
            } => Some((
                consent_revision.get(),
                route_policy_revision.get(),
                *maximum_trust_tier,
            )),
            _ => None,
        }
    }
}
