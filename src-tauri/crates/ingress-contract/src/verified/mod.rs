//! 字段私有、不可反序列化且只能由 verifier 构造的 Verified 请求。

use std::sync::Arc;

use crate::{
    operation::MatchedOperation,
    request::SanitizedRequest,
    signed::{
        AllowedEgressTarget, AttestationClaims, AuthorizationBundle, CapabilityClaims,
        ContextActivationKey, ContextCapabilityRequirements, ContextEgressPermit,
        ContinuationConstraint, EgressPurpose, SensitivityClass,
    },
    AccountId, AccountSelectorId, AdapterContractRevision, AdapterVersion, AudienceId,
    AuthorizationBundleDigest, BodyDigest, CanonicalOrigin, CapabilityManagementScopeId,
    ClientFamilyId, ClientVersion, ConsentRevision, ContextPolicyVersion, CredentialId, EndpointId,
    EnvelopeDigest, HandleEpoch, HttpMethod, IngressProtocol, IngressSchemaVersion,
    IngressTokenScopeId, IssuerEpoch, ListenerId, LocalOperationAuthScope, ModelDeploymentId,
    OperationId, ProtocolFrameRevision, RegistryDigest, RequestDigest, RequestDispatchDomain,
    RequestKind, RetrievalSchemaRevision, RoutePolicyRevision, SiteId, ToolSchemaRevision,
    TransformOwnerId, TransformOwnerVersion,
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
    registry_digest: RegistryDigest,
}

impl VerifiedIngressReceiver {
    pub(crate) fn new(
        domain_seal: Arc<VerificationDomainSeal>,
        registry_digest: RegistryDigest,
    ) -> Self {
        Self {
            domain_seal,
            registry_digest,
        }
    }

    /// 消费并确认请求属于当前生产验证域及其固定 RouteSpec 注册表。
    pub(crate) fn accept(
        &self,
        request: VerifiedIngressRequest,
    ) -> Result<ReceiverAcceptedIngressRequest, crate::IngressReject> {
        if !Arc::ptr_eq(&self.domain_seal, &request.domain_seal) {
            return Err(crate::IngressReject::VerificationDomainMismatch);
        }
        if request.registry_digest() != self.registry_digest {
            return Err(crate::IngressReject::RegistryMismatch);
        }
        Ok(ReceiverAcceptedIngressRequest { verified: request })
    }

    #[cfg(test)]
    pub(crate) fn with_registry_digest_for_test(&self, registry_digest: RegistryDigest) -> Self {
        Self {
            domain_seal: Arc::clone(&self.domain_seal),
            registry_digest,
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

/// 已验证 Managed egress 的用途闭集。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedEgressPurpose {
    /// 普通模型推理。
    ModelInference,
    /// 独立策略控制的辅助推理。
    AuxiliaryInference,
    /// 与真实请求绑定的远程精确 token 计数。
    ExactUpstreamTokenCount,
}

/// 已验证 Managed 请求的敏感度闭集。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedSensitivityClass {
    /// 公开内容。
    Public,
    /// 内部内容。
    Internal,
    /// 私有代码或同等敏感内容。
    PrivateCode,
}

/// 已验证上下文延续约束闭集。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerifiedContinuationConstraint {
    /// 不允许延续。
    None,
    /// 完整历史可跨目标移植。
    FullHistoryPortable,
    /// 延续状态绑定特定 Provider。
    ProviderStateful,
}

/// Managed 激活键的只读借用视图。
pub struct VerifiedManagedActivationKeyView<'a> {
    key: &'a ContextActivationKey,
}

impl VerifiedManagedActivationKeyView<'_> {
    /// 返回客户端家族。
    pub const fn client_family(&self) -> ClientFamilyId {
        self.key.client_family
    }

    /// 返回客户端版本。
    pub const fn client_version(&self) -> ClientVersion {
        self.key.client_version
    }

    /// 返回客户端 Adapter 版本。
    pub const fn adapter_version(&self) -> AdapterVersion {
        self.key.adapter_version
    }

    /// 返回入站 schema 版本。
    pub const fn ingress_schema_version(&self) -> IngressSchemaVersion {
        self.key.ingress_schema_version
    }

    /// 返回 ContextPolicy 版本。
    pub const fn context_policy_version(&self) -> ContextPolicyVersion {
        self.key.context_policy_version
    }

    /// 返回唯一有损变换 owner。
    pub const fn transform_owner(&self) -> TransformOwnerId {
        self.key.transform_owner
    }

    /// 返回有损变换 owner 版本。
    pub const fn transform_owner_version(&self) -> TransformOwnerVersion {
        self.key.transform_owner_version
    }
}

/// Managed EgressPermit 单个允许目标的只读借用视图。
pub struct VerifiedManagedEgressTargetView<'a> {
    target: &'a AllowedEgressTarget,
}

impl VerifiedManagedEgressTargetView<'_> {
    /// 返回站点逻辑标识。
    pub const fn site(&self) -> SiteId {
        self.target.site
    }

    /// 返回模型部署逻辑标识。
    pub const fn deployment(&self) -> ModelDeploymentId {
        self.target.deployment
    }

    /// 返回已规范化且验证过的 Origin。
    pub fn origin(&self) -> &CanonicalOrigin {
        &self.target.origin
    }

    /// 返回目标 TrustTier。
    pub const fn trust_tier(&self) -> u8 {
        self.target.trust_tier
    }
}

/// Managed EgressPermit 的只读借用视图。
///
/// 本视图不会暴露 permit nonce、原始 authorization bundle 或任何 MAC。
pub struct VerifiedManagedEgressPermitView<'a> {
    permit: &'a ContextEgressPermit,
}

impl<'a> VerifiedManagedEgressPermitView<'a> {
    /// 返回绑定的 Operation。
    pub const fn operation(&self) -> OperationId {
        self.permit.operation
    }

    /// 返回绑定的原始请求摘要；不得写入普通日志。
    pub const fn request_digest(&self) -> RequestDigest {
        self.permit.request_digest
    }

    /// 返回绑定的原始正文摘要；不得写入普通日志。
    pub const fn body_digest(&self) -> BodyDigest {
        self.permit.body_digest
    }

    /// 返回绑定的 ContextEnvelope 摘要；不得写入普通日志。
    pub const fn envelope_digest(&self) -> EnvelopeDigest {
        self.permit.envelope_digest
    }

    /// 返回 egress 用途。
    pub const fn purpose(&self) -> VerifiedEgressPurpose {
        match self.permit.purpose {
            EgressPurpose::ModelInference => VerifiedEgressPurpose::ModelInference,
            EgressPurpose::AuxiliaryInference => VerifiedEgressPurpose::AuxiliaryInference,
            EgressPurpose::ExactUpstreamTokenCount => {
                VerifiedEgressPurpose::ExactUpstreamTokenCount
            }
        }
    }

    /// 返回数据敏感度。
    pub const fn sensitivity(&self) -> VerifiedSensitivityClass {
        match self.permit.sensitivity {
            SensitivityClass::Public => VerifiedSensitivityClass::Public,
            SensitivityClass::Internal => VerifiedSensitivityClass::Internal,
            SensitivityClass::PrivateCode => VerifiedSensitivityClass::PrivateCode,
        }
    }

    /// 返回允许的最大出站字节数。
    pub const fn max_outbound_bytes(&self) -> u64 {
        self.permit.max_outbound_bytes
    }

    /// 返回允许目标数量。
    pub fn target_count(&self) -> usize {
        self.permit.allowed_targets.len()
    }

    /// 依照 canonical 顺序遍历精确允许目标。
    pub fn targets(
        &self,
    ) -> impl ExactSizeIterator<Item = VerifiedManagedEgressTargetView<'a>> + 'a {
        self.permit
            .allowed_targets
            .iter()
            .map(|target| VerifiedManagedEgressTargetView { target })
    }

    /// 返回是否允许按 permit 顺序 fallback。
    pub const fn fallback_allowed(&self) -> bool {
        self.permit.fallback_allowed
    }

    /// 返回 RoutePolicy 修订号。
    pub const fn route_policy_revision(&self) -> RoutePolicyRevision {
        self.permit.policy_revision
    }

    /// 返回用户 consent 修订号。
    pub const fn consent_revision(&self) -> ConsentRevision {
        self.permit.consent_revision
    }

    /// 返回 permit 绝对过期时间（Unix 毫秒）。
    pub const fn expires_at_millis(&self) -> u64 {
        self.permit.expires_at_millis
    }
}

/// Managed CapabilityRequirements 的只读借用视图。
pub struct VerifiedManagedCapabilityRequirementsView<'a> {
    requirements: &'a ContextCapabilityRequirements,
}

impl VerifiedManagedCapabilityRequirementsView<'_> {
    /// 返回工具 schema 修订号。
    pub const fn tool_schema_revision(&self) -> ToolSchemaRevision {
        self.requirements.tool_schema_revision
    }

    /// 返回检索 schema 修订号。
    pub const fn retrieval_schema_revision(&self) -> RetrievalSchemaRevision {
        self.requirements.retrieval_schema_revision
    }

    /// 返回客户端 Adapter 版本。
    pub const fn client_adapter_version(&self) -> AdapterVersion {
        self.requirements.client_adapter_version
    }

    /// 返回上游 Adapter 合同修订号。
    pub const fn upstream_adapter_revision(&self) -> AdapterContractRevision {
        self.requirements.upstream_adapter_revision
    }

    /// 返回本地 handle epoch。
    pub const fn handle_epoch(&self) -> HandleEpoch {
        self.requirements.handle_epoch
    }

    /// 返回所有 handle 中最早的绝对过期时间；零表示不携带 handle。
    pub const fn handle_earliest_expiry_millis(&self) -> u64 {
        self.requirements.handle_earliest_expiry_millis
    }

    /// 返回协议 frame schema 修订号。
    pub const fn protocol_frame_revision(&self) -> ProtocolFrameRevision {
        self.requirements.protocol_frame_revision
    }

    /// 返回上下文延续约束。
    pub const fn continuation(&self) -> VerifiedContinuationConstraint {
        match self.requirements.continuation {
            ContinuationConstraint::None => VerifiedContinuationConstraint::None,
            ContinuationConstraint::FullHistoryPortable => {
                VerifiedContinuationConstraint::FullHistoryPortable
            }
            ContinuationConstraint::ProviderStateful => {
                VerifiedContinuationConstraint::ProviderStateful
            }
        }
    }

    /// 返回后续执行是否必须具备本地检索 handle。
    pub const fn local_handle_required(&self) -> bool {
        self.requirements.local_handle_required
    }
}

struct VerifiedManagedProofBinding {
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
    issuer_epoch: IssuerEpoch,
    issued_at_millis: u64,
    expires_at_millis: u64,
    registry_digest: RegistryDigest,
    authorization_bundle_digest: AuthorizationBundleDigest,
    policy_revision: RoutePolicyRevision,
    adapter_version: AdapterVersion,
    transform_owner_version: TransformOwnerVersion,
}

struct VerifiedGatewayOnlyProofBinding {
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
    issued_at_millis: u64,
    expires_at_millis: u64,
    consent_revision: ConsentRevision,
    route_policy_revision: RoutePolicyRevision,
    registry_digest: RegistryDigest,
    request_digest: RequestDigest,
}

struct VerifiedLocalProofBinding {
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    auth_scope: LocalOperationAuthScope,
    registry_digest: RegistryDigest,
    request_digest: RequestDigest,
}

/// Managed proof 与 authorization bundle 的只读借用视图。
///
/// 本视图只暴露整体 authorization bundle 摘要，不暴露 MAC、nonce 或原始 bundle wire。
pub struct VerifiedManagedRequestView<'a> {
    proof: &'a VerifiedManagedProofBinding,
    bundle: &'a AuthorizationBundle,
}

impl<'a> VerifiedManagedRequestView<'a> {
    /// 返回绑定的 listener。
    pub const fn listener(&self) -> ListenerId {
        self.proof.listener
    }

    /// 返回绑定的入站 token scope。
    pub const fn token_scope(&self) -> IngressTokenScopeId {
        self.proof.token_scope
    }

    /// 返回证明 audience。
    pub const fn audience(&self) -> AudienceId {
        self.proof.audience
    }

    /// 返回签发 key epoch。
    pub const fn issuer_epoch(&self) -> IssuerEpoch {
        self.proof.issuer_epoch
    }

    /// 返回签发时间（Unix 毫秒）。
    pub const fn issued_at_millis(&self) -> u64 {
        self.proof.issued_at_millis
    }

    /// 返回证明绝对过期时间（Unix 毫秒）。
    pub const fn expires_at_millis(&self) -> u64 {
        self.proof.expires_at_millis
    }

    /// 返回证明绑定的 RouteSpec 注册表摘要。
    pub const fn registry_digest(&self) -> RegistryDigest {
        self.proof.registry_digest
    }

    /// 返回已验证的整体 authorization bundle 摘要；不得写入普通日志。
    pub const fn authorization_bundle_digest(&self) -> AuthorizationBundleDigest {
        self.proof.authorization_bundle_digest
    }

    /// 返回 RoutePolicy 修订号。
    pub const fn route_policy_revision(&self) -> RoutePolicyRevision {
        self.proof.policy_revision
    }

    /// 返回用户 consent 修订号。
    pub const fn consent_revision(&self) -> ConsentRevision {
        self.bundle.permit.consent_revision
    }

    /// 返回 attestation 绑定的客户端 Adapter 版本。
    pub const fn adapter_version(&self) -> AdapterVersion {
        self.proof.adapter_version
    }

    /// 返回 attestation 绑定的有损变换 owner 版本。
    pub const fn transform_owner_version(&self) -> TransformOwnerVersion {
        self.proof.transform_owner_version
    }

    /// 返回唯一有损变换 owner。
    pub const fn transform_owner(&self) -> TransformOwnerId {
        self.bundle.activation_key.transform_owner
    }

    /// 返回完整激活键的只读借用视图。
    pub const fn activation_key(&self) -> VerifiedManagedActivationKeyView<'a> {
        VerifiedManagedActivationKeyView {
            key: &self.bundle.activation_key,
        }
    }

    /// 返回 EgressPermit 的只读借用视图。
    pub const fn egress_permit(&self) -> VerifiedManagedEgressPermitView<'a> {
        VerifiedManagedEgressPermitView {
            permit: &self.bundle.permit,
        }
    }

    /// 返回 CapabilityRequirements 的只读借用视图。
    pub const fn capability_requirements(&self) -> VerifiedManagedCapabilityRequirementsView<'a> {
        VerifiedManagedCapabilityRequirementsView {
            requirements: &self.bundle.requirements,
        }
    }
}

/// GatewayOnly 本机 consent 约束的只读借用视图。
pub struct VerifiedGatewayOnlyView<'a> {
    proof: &'a VerifiedGatewayOnlyProofBinding,
    maximum_trust_tier: u8,
    local_handle_allowed: bool,
}

impl VerifiedGatewayOnlyView<'_> {
    /// 返回绑定的 listener。
    pub const fn listener(&self) -> ListenerId {
        self.proof.listener
    }

    /// 返回绑定的入站 token scope。
    pub const fn token_scope(&self) -> IngressTokenScopeId {
        self.proof.token_scope
    }

    /// 返回 consent audience。
    pub const fn audience(&self) -> AudienceId {
        self.proof.audience
    }

    /// 返回 consent 签发时间（Unix 毫秒）。
    pub const fn issued_at_millis(&self) -> u64 {
        self.proof.issued_at_millis
    }

    /// 返回 consent 绝对过期时间（Unix 毫秒）。
    pub const fn expires_at_millis(&self) -> u64 {
        self.proof.expires_at_millis
    }

    /// 返回 consent 修订号。
    pub const fn consent_revision(&self) -> ConsentRevision {
        self.proof.consent_revision
    }

    /// 返回 RoutePolicy 修订号。
    pub const fn route_policy_revision(&self) -> RoutePolicyRevision {
        self.proof.route_policy_revision
    }

    /// 返回绑定的 RouteSpec 注册表摘要。
    pub const fn registry_digest(&self) -> RegistryDigest {
        self.proof.registry_digest
    }

    /// 返回绑定的原始请求摘要；不得写入普通日志。
    pub const fn request_digest(&self) -> RequestDigest {
        self.proof.request_digest
    }

    /// 返回 consent 允许的最高 TrustTier。
    pub const fn maximum_trust_tier(&self) -> u8 {
        self.maximum_trust_tier
    }

    /// GatewayOnly 永远不能授予 LocalHandle/retrieval authority。
    pub const fn local_handle_allowed(&self) -> bool {
        self.local_handle_allowed
    }
}

/// CapabilityScoped 精确绑定的只读视图。
///
/// 该视图只能从 receiver 接受后的 crate-private typestate 借用，不能据此构造或提升权限。
pub struct VerifiedCapabilityBindingView<'a> {
    claims: &'a CapabilityClaims,
}

impl<'a> VerifiedCapabilityBindingView<'a> {
    /// 返回绑定的 listener。
    pub const fn listener(&self) -> ListenerId {
        self.claims.listener
    }

    /// 返回绑定的入站 token scope。
    pub const fn token_scope(&self) -> IngressTokenScopeId {
        self.claims.token_scope
    }

    /// 返回授权 audience。
    pub const fn audience(&self) -> AudienceId {
        self.claims.audience
    }

    /// 返回签发 key epoch。
    pub const fn issuer_epoch(&self) -> IssuerEpoch {
        self.claims.issuer_epoch
    }

    /// 返回签发时间（Unix 毫秒）。
    pub const fn issued_at_millis(&self) -> u64 {
        self.claims.issued_at_millis
    }

    /// 返回绑定的 RouteSpec 注册表摘要。
    pub const fn registry_digest(&self) -> RegistryDigest {
        self.claims.registry_digest
    }

    /// 返回绑定的原始请求摘要；不得写入普通日志。
    pub const fn request_digest(&self) -> RequestDigest {
        self.claims.request_digest
    }

    /// 返回唯一站点。
    pub const fn site(&self) -> SiteId {
        self.claims.site
    }

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

    /// 返回授权签名绑定的 TrustTier。
    pub const fn trust_tier(&self) -> u8 {
        self.claims.trust_tier
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

enum VerifiedProof {
    Managed(VerifiedManagedProofBinding),
    GatewayOnlyScopedConsent(VerifiedGatewayOnlyProofBinding),
    LocalOperationScoped(VerifiedLocalProofBinding),
    CapabilityScoped(CapabilityClaims),
}

enum VerifiedAuthorization {
    None,
    Managed(Box<AuthorizationBundle>),
    GatewayOnlyExplicit {
        maximum_trust_tier: u8,
        local_handle_allowed: bool,
    },
    CapabilityBound,
}

/// verifier 产生、但尚未由生产 receiver 接受的 opaque 入站请求。
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
        claims: AttestationClaims,
        authorization_bundle: AuthorizationBundle,
    ) -> Self {
        Self {
            domain_seal,
            operation,
            request,
            proof: VerifiedProof::Managed(VerifiedManagedProofBinding {
                listener: claims.listener,
                token_scope: claims.token_scope,
                audience: claims.audience,
                issuer_epoch: claims.issuer_epoch,
                issued_at_millis: claims.issued_at_millis,
                expires_at_millis: claims.expires_at_millis,
                registry_digest: claims.registry_digest,
                authorization_bundle_digest: claims.authorization_bundle_digest,
                policy_revision: claims.policy_revision,
                adapter_version: claims.adapter_version,
                transform_owner_version: claims.transform_owner_version,
            }),
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
        audience: AudienceId,
        issued_at_millis: u64,
        expires_at_millis: u64,
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
            proof: VerifiedProof::GatewayOnlyScopedConsent(VerifiedGatewayOnlyProofBinding {
                listener,
                token_scope,
                audience,
                issued_at_millis,
                expires_at_millis,
                consent_revision,
                route_policy_revision,
                registry_digest,
                request_digest,
            }),
            authorization: VerifiedAuthorization::GatewayOnlyExplicit {
                maximum_trust_tier,
                local_handle_allowed: false,
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_local(
        domain_seal: Arc<VerificationDomainSeal>,
        operation: MatchedOperation,
        request: SanitizedRequest,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        auth_scope: LocalOperationAuthScope,
        registry_digest: RegistryDigest,
        request_digest: RequestDigest,
    ) -> Self {
        Self {
            domain_seal,
            operation,
            request,
            proof: VerifiedProof::LocalOperationScoped(VerifiedLocalProofBinding {
                listener,
                token_scope,
                auth_scope,
                registry_digest,
                request_digest,
            }),
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

    pub(crate) const fn operation(&self) -> OperationId {
        self.operation.spec.operation
    }

    pub(crate) const fn request_kind(&self) -> RequestKind {
        self.operation.spec.kind
    }

    pub(crate) const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.operation.spec.dispatch_domain
    }

    pub(crate) const fn ingress_protocol(&self) -> IngressProtocol {
        self.operation.spec.ingress_protocol
    }

    pub(crate) const fn registry_digest(&self) -> RegistryDigest {
        match &self.proof {
            VerifiedProof::Managed(proof) => proof.registry_digest,
            VerifiedProof::GatewayOnlyScopedConsent(proof) => proof.registry_digest,
            VerifiedProof::LocalOperationScoped(proof) => proof.registry_digest,
            VerifiedProof::CapabilityScoped(claims) => claims.registry_digest,
        }
    }

    pub(crate) const fn proof_kind(&self) -> VerifiedProofKind {
        match &self.proof {
            VerifiedProof::Managed(_) => VerifiedProofKind::Managed,
            VerifiedProof::GatewayOnlyScopedConsent(_) => {
                VerifiedProofKind::GatewayOnlyScopedConsent
            }
            VerifiedProof::LocalOperationScoped(_) => VerifiedProofKind::LocalOperationScoped,
            VerifiedProof::CapabilityScoped(_) => VerifiedProofKind::CapabilityScoped,
        }
    }

    pub(crate) const fn authorization_kind(&self) -> VerifiedAuthorizationKind {
        match &self.authorization {
            VerifiedAuthorization::None => VerifiedAuthorizationKind::None,
            VerifiedAuthorization::Managed(_) => VerifiedAuthorizationKind::ManagedEgress,
            VerifiedAuthorization::GatewayOnlyExplicit { .. } => {
                VerifiedAuthorizationKind::GatewayOnlyExplicit
            }
            VerifiedAuthorization::CapabilityBound => VerifiedAuthorizationKind::CapabilityBound,
        }
    }

    pub(crate) const fn method(&self) -> HttpMethod {
        self.request.method
    }

    pub(crate) fn target(&self) -> &[u8] {
        &self.request.target
    }

    pub(crate) fn body(&self) -> &[u8] {
        &self.request.body
    }

    pub(crate) fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.request.headers.iter().map(|header| VerifiedHeaderRef {
            name: &header.name,
            value: &header.value,
        })
    }

    pub(crate) fn capability_binding(&self) -> Option<VerifiedCapabilityBindingView<'_>> {
        match &self.proof {
            VerifiedProof::CapabilityScoped(claims) => {
                Some(VerifiedCapabilityBindingView { claims })
            }
            _ => None,
        }
    }

    pub(crate) fn request_digest(&self) -> RequestDigest {
        match &self.proof {
            VerifiedProof::Managed(_) => match &self.authorization {
                VerifiedAuthorization::Managed(bundle) => bundle.permit.request_digest,
                _ => unreachable!(),
            },
            VerifiedProof::GatewayOnlyScopedConsent(proof) => proof.request_digest,
            VerifiedProof::LocalOperationScoped(proof) => proof.request_digest,
            VerifiedProof::CapabilityScoped(claims) => claims.request_digest,
        }
    }

    pub(crate) const fn listener_scope(&self) -> (ListenerId, IngressTokenScopeId) {
        match &self.proof {
            VerifiedProof::Managed(proof) => (proof.listener, proof.token_scope),
            VerifiedProof::GatewayOnlyScopedConsent(proof) => (proof.listener, proof.token_scope),
            VerifiedProof::LocalOperationScoped(proof) => (proof.listener, proof.token_scope),
            VerifiedProof::CapabilityScoped(claims) => (claims.listener, claims.token_scope),
        }
    }

    pub(crate) const fn local_auth_scope(&self) -> Option<LocalOperationAuthScope> {
        match &self.proof {
            VerifiedProof::LocalOperationScoped(proof) => Some(proof.auth_scope),
            _ => None,
        }
    }

    pub(crate) fn local_handle_allowed(&self) -> bool {
        match &self.authorization {
            VerifiedAuthorization::Managed(bundle) => bundle.requirements.local_handle_required,
            VerifiedAuthorization::GatewayOnlyExplicit {
                local_handle_allowed,
                ..
            } => *local_handle_allowed,
            VerifiedAuthorization::None | VerifiedAuthorization::CapabilityBound => false,
        }
    }

    pub(crate) fn managed(&self) -> Option<VerifiedManagedRequestView<'_>> {
        match (&self.proof, &self.authorization) {
            (VerifiedProof::Managed(proof), VerifiedAuthorization::Managed(bundle)) => {
                Some(VerifiedManagedRequestView { proof, bundle })
            }
            _ => None,
        }
    }

    pub(crate) fn gateway_only(&self) -> Option<VerifiedGatewayOnlyView<'_>> {
        match (&self.proof, &self.authorization) {
            (
                VerifiedProof::GatewayOnlyScopedConsent(proof),
                VerifiedAuthorization::GatewayOnlyExplicit {
                    maximum_trust_tier,
                    local_handle_allowed,
                    ..
                },
            ) => Some(VerifiedGatewayOnlyView {
                proof,
                maximum_trust_tier: *maximum_trust_tier,
                local_handle_allowed: *local_handle_allowed,
            }),
            _ => None,
        }
    }
}

/// 已由生产 receiver 接受、唯一允许 classifier 读取的入站请求 typestate。
///
/// 字段私有；本类型不实现 `Clone`、`Copy`、`Debug`、`Default` 或任何 Serde trait。
pub(crate) struct ReceiverAcceptedIngressRequest {
    verified: VerifiedIngressRequest,
}

impl ReceiverAcceptedIngressRequest {
    /// 返回已由 Gateway 重新匹配的 Operation ID。
    pub(crate) const fn operation(&self) -> OperationId {
        self.verified.operation()
    }

    /// 返回已注册请求语义。
    pub(crate) const fn request_kind(&self) -> RequestKind {
        self.verified.request_kind()
    }

    /// 返回唯一分发域。
    pub(crate) const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.verified.dispatch_domain()
    }

    /// 返回由 RouteSpec 固定的入站协议。
    pub(crate) const fn ingress_protocol(&self) -> IngressProtocol {
        self.verified.ingress_protocol()
    }

    /// 返回 receiver 已复核的 RouteSpec 注册表摘要。
    pub(crate) const fn registry_digest(&self) -> RegistryDigest {
        self.verified.registry_digest()
    }

    /// 返回证明种类，不暴露可伪造 proof variant。
    pub(crate) const fn proof_kind(&self) -> VerifiedProofKind {
        self.verified.proof_kind()
    }

    /// 返回授权约束种类。
    pub(crate) const fn authorization_kind(&self) -> VerifiedAuthorizationKind {
        self.verified.authorization_kind()
    }

    /// 返回清理后的 HTTP 方法。
    pub(crate) const fn method(&self) -> HttpMethod {
        self.verified.method()
    }

    /// 返回已验证的原始 request target。
    pub(crate) fn target(&self) -> &[u8] {
        self.verified.target()
    }

    /// 返回原始正文的只读借用。
    pub(crate) fn body(&self) -> &[u8] {
        self.verified.body()
    }

    /// 遍历已移除入站认证、Content-Length 和证明材料的 Header。
    pub(crate) fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.verified.headers()
    }

    /// 返回证明绑定的原始请求摘要；不得写入普通日志。
    pub(crate) fn request_digest(&self) -> RequestDigest {
        self.verified.request_digest()
    }

    /// 返回 proof 绑定的 listener 与入站 token scope。
    pub(crate) const fn listener_scope(&self) -> (ListenerId, IngressTokenScopeId) {
        self.verified.listener_scope()
    }

    /// Managed 成功时返回不含 MAC、nonce 与原始 bundle 的只读视图。
    pub(crate) fn managed(&self) -> Option<VerifiedManagedRequestView<'_>> {
        self.verified.managed()
    }

    /// GatewayOnly 成功时返回本机 consent 约束只读视图。
    pub(crate) fn gateway_only(&self) -> Option<VerifiedGatewayOnlyView<'_>> {
        self.verified.gateway_only()
    }

    /// Local 成功时返回精确 auth scope。
    pub(crate) const fn local_auth_scope(&self) -> Option<LocalOperationAuthScope> {
        self.verified.local_auth_scope()
    }

    /// CapabilityScoped 成功时返回精确部署/账户/凭据绑定视图。
    pub(crate) fn capability_binding(&self) -> Option<VerifiedCapabilityBindingView<'_>> {
        self.verified.capability_binding()
    }

    /// 返回当前授权是否要求或允许 LocalHandle。
    pub(crate) fn local_handle_allowed(&self) -> bool {
        self.verified.local_handle_allowed()
    }
}

#[cfg(test)]
impl VerifiedIngressRequest {
    pub(crate) fn proof_binding_is_nonzero(&self) -> bool {
        match &self.proof {
            VerifiedProof::Managed(proof) => {
                proof.issuer_epoch.get() != 0
                    && proof.authorization_bundle_digest.as_bytes() != &[0; 32]
                    && proof.registry_digest.as_bytes() != &[0; 32]
            }
            VerifiedProof::GatewayOnlyScopedConsent(proof) => {
                proof.listener.get() != 0
                    && proof.token_scope.get() != 0
                    && proof.consent_revision.get() != 0
                    && proof.route_policy_revision.get() != 0
                    && proof.registry_digest.as_bytes() != &[0; 32]
                    && proof.request_digest.as_bytes() != &[0; 32]
            }
            VerifiedProof::LocalOperationScoped(proof) => {
                let _ = proof.auth_scope;
                proof.listener.get() != 0
                    && proof.token_scope.get() != 0
                    && proof.registry_digest.as_bytes() != &[0; 32]
                    && proof.request_digest.as_bytes() != &[0; 32]
            }
            VerifiedProof::CapabilityScoped(claims) => {
                claims.nonce.as_bytes() != &[0; 16]
                    && claims.registry_digest.as_bytes() != &[0; 32]
                    && claims.request_digest.as_bytes() != &[0; 32]
            }
        }
    }
}
