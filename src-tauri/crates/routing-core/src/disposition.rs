//! 三路不可互换的分类结果 typestate。

use ingress_contract::verified::{
    ReceiverAcceptedIngressRequest, VerifiedGatewayOnlyView, VerifiedHeaderRef,
    VerifiedManagedCapabilityRequirementsView, VerifiedManagedEgressPermitView,
    VerifiedManagedRequestView,
};
use ingress_contract::{
    AccountId, AccountSelectorId, AdapterContractRevision, CanonicalOrigin, CredentialId,
    EndpointId, HttpMethod, IngressProtocol, ModelDeploymentId, OperationId, RegistryDigest,
    RequestDigest, RequestDispatchDomain, RequestKind, SiteId,
};

use super::classifier::DispositionConstructionSeal;
use super::error::RouteReject;
use super::normalizer::ProtocolNormalizedRequest;
use super::snapshot::ClassifierBoundTarget;

/// 已通过 Context 逐请求门禁的模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextExecutionMode {
    /// 受 Managed attestation 与整体授权 bundle 约束。
    Managed,
    /// 受用户显式启用的 GatewayOnly listener/consent 约束。
    GatewayOnly,
}

/// BoundDeployment 的固定失败策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundFallbackPolicy {
    /// 固定单目标、单次 Attempt，禁止 retry/fallback。
    Forbidden,
}

/// classifier 内部 grant 对同一已接受请求的不可伪造绑定。
///
/// 本类型不实现 `Clone`、`Copy`、`Debug` 或任何 Serde trait；每个门禁只能从当前
/// [`ClassifiedRequest`] 生成它，后续构造器会再次全等复核。
pub(crate) struct GrantBinding {
    operation: OperationId,
    request_digest: RequestDigest,
    registry_digest: RegistryDigest,
}

impl GrantBinding {
    pub(crate) fn from_request(request: &ClassifiedRequest) -> Self {
        Self {
            operation: request.operation(),
            request_digest: request.request_digest(),
            registry_digest: request.accepted.registry_digest(),
        }
    }

    pub(crate) fn matches(&self, request: &ClassifiedRequest) -> bool {
        self.operation == request.operation()
            && self.request_digest == request.request_digest()
            && self.registry_digest == request.accepted.registry_digest()
    }
}

pub(crate) enum ContextExecutionGrant {
    Managed {
        binding: GrantBinding,
        route_policy_revision: ingress_contract::RoutePolicyRevision,
        consent_revision: ingress_contract::ConsentRevision,
        expires_at_millis: u64,
    },
    GatewayOnly {
        binding: GrantBinding,
        route_policy_revision: ingress_contract::RoutePolicyRevision,
        consent_revision: ingress_contract::ConsentRevision,
        maximum_trust_tier: u8,
        expires_at_millis: u64,
    },
}

impl ContextExecutionGrant {
    pub(crate) const fn binding(&self) -> &GrantBinding {
        match self {
            Self::Managed { binding, .. } | Self::GatewayOnly { binding, .. } => binding,
        }
    }

    pub(crate) const fn expires_at_millis(&self) -> u64 {
        match self {
            Self::Managed {
                expires_at_millis, ..
            }
            | Self::GatewayOnly {
                expires_at_millis, ..
            } => *expires_at_millis,
        }
    }
}

pub(crate) struct BoundDeploymentGrant {
    pub(crate) binding: GrantBinding,
    pub(crate) target: BoundTarget,
    pub(crate) context: Option<ContextExecutionGrant>,
    pub(crate) management_scope: Option<ingress_contract::CapabilityManagementScopeId>,
    pub(crate) deadline_millis: u64,
}

pub(crate) struct BoundTarget {
    pub(crate) site: SiteId,
    pub(crate) deployment: ModelDeploymentId,
    pub(crate) endpoint: EndpointId,
    pub(crate) origin: CanonicalOrigin,
    pub(crate) account_selector: AccountSelectorId,
    pub(crate) account: AccountId,
    pub(crate) credential: CredentialId,
    pub(crate) adapter_contract_revision: AdapterContractRevision,
    pub(crate) trust_tier: u8,
}

impl BoundTarget {
    pub(crate) fn from_snapshot(target: &ClassifierBoundTarget) -> Self {
        Self {
            site: target.site,
            deployment: target.deployment,
            endpoint: target.endpoint,
            origin: target.origin.clone(),
            account_selector: target.account_selector,
            account: target.account,
            credential: target.credential,
            adapter_contract_revision: target.adapter_contract_revision,
            trust_tier: target.trust_tier,
        }
    }
}

/// classifier 内部的私有中间状态；不实现 IPC/Serde `Deserialize`。
pub(crate) struct ClassifiedRequest {
    pub(crate) accepted: ReceiverAcceptedIngressRequest,
    pub(crate) _normalized: ProtocolNormalizedRequest,
}

impl ClassifiedRequest {
    pub(crate) fn new(
        accepted: ReceiverAcceptedIngressRequest,
        normalized: ProtocolNormalizedRequest,
    ) -> Self {
        Self {
            accepted,
            _normalized: normalized,
        }
    }

    pub(crate) const fn operation(&self) -> OperationId {
        self.accepted.operation()
    }

    pub(crate) const fn request_kind(&self) -> RequestKind {
        self.accepted.request_kind()
    }

    pub(crate) const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.accepted.dispatch_domain()
    }

    pub(crate) const fn ingress_protocol(&self) -> IngressProtocol {
        self.accepted.ingress_protocol()
    }

    pub(crate) fn request_digest(&self) -> RequestDigest {
        self.accepted.request_digest()
    }
}

/// classifier 唯一产生的三路 Verified disposition。
///
/// 三个 variant 不共享可提升的公共计划类型；调用方只能把每个 variant 交给与其域匹配的
/// 下游 Port。
pub enum VerifiedIngressDisposition {
    /// 仅可进入匹配的本地 handler。
    Local(VerifiedLocalDispatch),
    /// 仅可进入固定单部署 planner。
    BoundDeployment(VerifiedBoundDeploymentRequest),
    /// 仅可进入显式 RoutePolicy planner。
    Routed(VerifiedRouteRequest),
}

/// 通过 LocalOperationScopeGate 的本地请求。
pub struct VerifiedLocalDispatch {
    request: ClassifiedRequest,
    auth_scope: ingress_contract::LocalOperationAuthScope,
}

impl VerifiedLocalDispatch {
    pub(crate) fn construct(
        request: ClassifiedRequest,
        auth_scope: ingress_contract::LocalOperationAuthScope,
        _seal: DispositionConstructionSeal,
    ) -> Self {
        Self {
            request,
            auth_scope,
        }
    }

    /// 返回 Operation ID。
    pub const fn operation(&self) -> OperationId {
        self.request.operation()
    }

    /// 返回请求语义。
    pub const fn request_kind(&self) -> RequestKind {
        self.request.request_kind()
    }

    /// 返回固定入站协议。
    pub const fn ingress_protocol(&self) -> IngressProtocol {
        self.request.ingress_protocol()
    }

    /// 返回唯一分发域（始终为 Local）。
    pub const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.request.dispatch_domain()
    }

    /// 返回原始请求摘要。
    pub fn request_digest(&self) -> RequestDigest {
        self.request.request_digest()
    }

    /// 返回 receiver 已复核的 RouteSpec 注册表摘要。
    pub const fn registry_digest(&self) -> ingress_contract::RegistryDigest {
        self.request.accepted.registry_digest()
    }

    /// 返回本地授权 scope。
    pub const fn auth_scope(&self) -> ingress_contract::LocalOperationAuthScope {
        self.auth_scope
    }

    /// 返回清理后的 HTTP 方法。
    pub const fn method(&self) -> HttpMethod {
        self.request.accepted.method()
    }

    /// 返回已验证 target。
    pub fn target(&self) -> &[u8] {
        self.request.accepted.target()
    }

    /// 返回原始正文借用。
    pub fn body(&self) -> &[u8] {
        self.request.accepted.body()
    }

    /// 遍历已清理 Header。
    pub fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.request.accepted.headers()
    }
}

/// 通过 ContextRequestExecutionGate、且必要时通过 BoundDeploymentRequestGate 的普通路由请求。
pub struct VerifiedRouteRequest {
    request: ClassifiedRequest,
    context: ContextExecutionGrant,
}

impl VerifiedRouteRequest {
    pub(crate) fn construct(
        request: ClassifiedRequest,
        context: ContextExecutionGrant,
        _seal: DispositionConstructionSeal,
    ) -> Result<Self, RouteReject> {
        if !context.binding().matches(&request) {
            return Err(RouteReject::SnapshotBindingMismatch);
        }
        Ok(Self { request, context })
    }

    /// 返回 Operation ID。
    pub const fn operation(&self) -> OperationId {
        self.request.operation()
    }

    /// 返回请求语义。
    pub const fn request_kind(&self) -> RequestKind {
        self.request.request_kind()
    }

    /// 返回固定入站协议。
    pub const fn ingress_protocol(&self) -> IngressProtocol {
        self.request.ingress_protocol()
    }

    /// 返回唯一分发域（始终为 RoutedPolicy）。
    pub const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.request.dispatch_domain()
    }

    /// 返回原始请求摘要。
    pub fn request_digest(&self) -> RequestDigest {
        self.request.request_digest()
    }

    /// 返回 receiver 已复核的 RouteSpec 注册表摘要。
    pub const fn registry_digest(&self) -> ingress_contract::RegistryDigest {
        self.request.accepted.registry_digest()
    }

    /// 返回已通过的 Context 模式。
    pub const fn context_mode(&self) -> ContextExecutionMode {
        match self.context {
            ContextExecutionGrant::Managed { .. } => ContextExecutionMode::Managed,
            ContextExecutionGrant::GatewayOnly { .. } => ContextExecutionMode::GatewayOnly,
        }
    }

    /// 返回当前 RoutePolicy 修订号。
    pub const fn route_policy_revision(&self) -> ingress_contract::RoutePolicyRevision {
        match self.context {
            ContextExecutionGrant::Managed {
                route_policy_revision,
                ..
            }
            | ContextExecutionGrant::GatewayOnly {
                route_policy_revision,
                ..
            } => route_policy_revision,
        }
    }

    /// 返回当前 consent 修订号。
    pub const fn consent_revision(&self) -> ingress_contract::ConsentRevision {
        match self.context {
            ContextExecutionGrant::Managed {
                consent_revision, ..
            }
            | ContextExecutionGrant::GatewayOnly {
                consent_revision, ..
            } => consent_revision,
        }
    }

    /// GatewayOnly 成功时返回当前允许的最高 TrustTier。
    pub const fn maximum_trust_tier(&self) -> Option<u8> {
        match self.context {
            ContextExecutionGrant::Managed { .. } => None,
            ContextExecutionGrant::GatewayOnly {
                maximum_trust_tier, ..
            } => Some(maximum_trust_tier),
        }
    }

    /// 返回 Context grant 的绝对过期时间（Unix 毫秒）。
    pub const fn expires_at_millis(&self) -> u64 {
        self.context.expires_at_millis()
    }

    /// Managed 成功时返回已验证的 egress 只读视图。
    pub fn managed_egress_permit(&self) -> Option<VerifiedManagedEgressPermitView<'_>> {
        self.request
            .accepted
            .managed()
            .map(|view| view.egress_permit())
    }

    /// Managed 成功时返回已验证的能力要求只读视图。
    pub fn managed_capability_requirements(
        &self,
    ) -> Option<VerifiedManagedCapabilityRequirementsView<'_>> {
        self.request
            .accepted
            .managed()
            .map(|view| view.capability_requirements())
    }

    /// Managed 成功时返回激活键只读视图。
    pub fn managed_context(&self) -> Option<VerifiedManagedRequestView<'_>> {
        self.request.accepted.managed()
    }

    /// GatewayOnly 成功时返回本机 consent 只读视图。
    pub fn gateway_context(&self) -> Option<VerifiedGatewayOnlyView<'_>> {
        self.request.accepted.gateway_only()
    }

    /// 返回清理后的 HTTP 方法。
    pub const fn method(&self) -> HttpMethod {
        self.request.accepted.method()
    }

    /// 返回已验证 target。
    pub fn target(&self) -> &[u8] {
        self.request.accepted.target()
    }

    /// 返回原始正文借用。
    pub fn body(&self) -> &[u8] {
        self.request.accepted.body()
    }

    /// 遍历已清理 Header。
    pub fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.request.accepted.headers()
    }
}

/// 通过固定单部署 BoundDeploymentRequestGate 的请求。
pub struct VerifiedBoundDeploymentRequest {
    request: ClassifiedRequest,
    grant: BoundDeploymentGrant,
}

impl VerifiedBoundDeploymentRequest {
    pub(crate) fn construct(
        request: ClassifiedRequest,
        grant: BoundDeploymentGrant,
        _seal: DispositionConstructionSeal,
    ) -> Result<Self, RouteReject> {
        if !grant.binding.matches(&request) {
            return Err(RouteReject::SnapshotBindingMismatch);
        }
        Ok(Self { request, grant })
    }

    /// 返回 Operation ID。
    pub const fn operation(&self) -> OperationId {
        self.request.operation()
    }

    /// 返回请求语义。
    pub const fn request_kind(&self) -> RequestKind {
        self.request.request_kind()
    }

    /// 返回固定入站协议。
    pub const fn ingress_protocol(&self) -> IngressProtocol {
        self.request.ingress_protocol()
    }

    /// 返回唯一分发域（始终为 BoundDeployment）。
    pub const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.request.dispatch_domain()
    }

    /// 返回原始请求摘要。
    pub fn request_digest(&self) -> RequestDigest {
        self.request.request_digest()
    }

    /// 返回 receiver 已复核的 RouteSpec 注册表摘要。
    pub const fn registry_digest(&self) -> ingress_contract::RegistryDigest {
        self.request.accepted.registry_digest()
    }

    /// 返回已锁定的 Site。
    pub const fn site(&self) -> SiteId {
        self.grant.target.site
    }

    /// 返回已锁定的 ModelDeployment。
    pub const fn deployment(&self) -> ModelDeploymentId {
        self.grant.target.deployment
    }

    /// 返回已锁定的 Endpoint。
    pub const fn endpoint(&self) -> EndpointId {
        self.grant.target.endpoint
    }

    /// 返回已锁定的精确 Origin。
    pub fn origin(&self) -> &CanonicalOrigin {
        &self.grant.target.origin
    }

    /// 返回已锁定的 AccountSelector。
    pub const fn account_selector(&self) -> AccountSelectorId {
        self.grant.target.account_selector
    }

    /// 返回已锁定的 Account。
    pub const fn account(&self) -> AccountId {
        self.grant.target.account
    }

    /// 返回已锁定的 Credential 逻辑标识。
    pub const fn credential(&self) -> CredentialId {
        self.grant.target.credential
    }

    /// 返回已锁定的上游 Adapter 合同修订。
    pub const fn adapter_contract_revision(&self) -> AdapterContractRevision {
        self.grant.target.adapter_contract_revision
    }

    /// 返回已锁定的 TrustTier。
    pub const fn trust_tier(&self) -> u8 {
        self.grant.target.trust_tier
    }

    /// BoundDeployment 只能执行一次 Attempt。
    pub const fn max_attempts(&self) -> u8 {
        1
    }

    /// BoundDeployment 永远禁止 fallback。
    pub const fn fallback_policy(&self) -> BoundFallbackPolicy {
        BoundFallbackPolicy::Forbidden
    }

    /// CapabilityScoped 管理探测成功时返回锁定的管理 scope。
    pub const fn management_scope(&self) -> Option<ingress_contract::CapabilityManagementScopeId> {
        self.grant.management_scope
    }

    /// 返回本 Bound grant 的绝对截止时间（Unix 毫秒）。
    pub const fn deadline_millis(&self) -> u64 {
        self.grant.deadline_millis
    }

    /// 返回是否同时消费了 Context grant；ExactUpstream 必须为 `Some`。
    pub const fn context_mode(&self) -> Option<ContextExecutionMode> {
        match &self.grant.context {
            None => None,
            Some(ContextExecutionGrant::Managed { .. }) => Some(ContextExecutionMode::Managed),
            Some(ContextExecutionGrant::GatewayOnly { .. }) => {
                Some(ContextExecutionMode::GatewayOnly)
            }
        }
    }

    /// Managed Bound 成功时返回 egress 只读视图。
    pub fn managed_egress_permit(&self) -> Option<VerifiedManagedEgressPermitView<'_>> {
        self.request
            .accepted
            .managed()
            .map(|view| view.egress_permit())
    }

    /// GatewayOnly Bound 成功时返回 consent 只读视图。
    pub fn gateway_context(&self) -> Option<VerifiedGatewayOnlyView<'_>> {
        self.request.accepted.gateway_only()
    }

    /// 返回清理后的 HTTP 方法。
    pub const fn method(&self) -> HttpMethod {
        self.request.accepted.method()
    }

    /// 返回已验证 target。
    pub fn target(&self) -> &[u8] {
        self.request.accepted.target()
    }

    /// 返回原始正文借用。
    pub fn body(&self) -> &[u8] {
        self.request.accepted.body()
    }

    /// 遍历已清理 Header。
    pub fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.request.accepted.headers()
    }
}
