//! 与分类器实例绑定、只能由配对 authority 签发的逐请求快照。

use std::sync::Arc;

use ingress_contract::{
    AccountId, AccountSelectorId, AdapterContractRevision, AdapterVersion, AudienceId,
    AuthorizationBundleDigest, CanonicalOrigin, CapabilityManagementScopeId, ClientFamilyId,
    ClientVersion, ConsentRevision, ContextPolicyVersion, CredentialId, EndpointId,
    IngressSchemaVersion, IngressTokenScopeId, IssuerEpoch, ListenerId, LocalOperationAuthScope,
    ModelDeploymentId, OperationId, RegistryDigest, RequestDigest, RoutePolicyRevision, SiteId,
    TransformOwnerId, TransformOwnerVersion,
};

use super::error::RouteReject;

pub(crate) struct SnapshotDomainSeal {
    _private: (),
}

impl SnapshotDomainSeal {
    pub(crate) const fn new() -> Self {
        Self { _private: () }
    }
}

/// Operation 与原始请求摘要的逐请求绑定输入。
///
/// 本类型本身不授予权限；只有配对的 [`ClassifierSnapshotAuthority`] 可以消费它并签发
/// 当前分类器能够接受的快照。
pub struct ClassifierRequestBinding {
    pub(crate) operation: OperationId,
    pub(crate) request_digest: RequestDigest,
}

impl ClassifierRequestBinding {
    /// 构造逐请求绑定输入。
    pub const fn new(operation: OperationId, request_digest: RequestDigest) -> Self {
        Self {
            operation,
            request_digest,
        }
    }
}

/// listener、入站 token scope 与 audience 的当前绑定输入。
pub struct ClassifierIngressBinding {
    pub(crate) listener: ListenerId,
    pub(crate) token_scope: IngressTokenScopeId,
    pub(crate) audience: AudienceId,
}

impl ClassifierIngressBinding {
    /// 构造入站 scope 绑定输入。
    pub const fn new(
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        audience: AudienceId,
    ) -> Self {
        Self {
            listener,
            token_scope,
            audience,
        }
    }
}

/// 当前已激活 Managed Client/Adapter/Policy/owner 的不可变事实输入。
pub struct ClassifierManagedActivation {
    pub(crate) client_family: ClientFamilyId,
    pub(crate) client_version: ClientVersion,
    pub(crate) adapter_version: AdapterVersion,
    pub(crate) ingress_schema_version: IngressSchemaVersion,
    pub(crate) context_policy_version: ContextPolicyVersion,
    pub(crate) transform_owner: TransformOwnerId,
    pub(crate) transform_owner_version: TransformOwnerVersion,
}

impl ClassifierManagedActivation {
    /// 构造 Managed 激活事实输入。
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        client_family: ClientFamilyId,
        client_version: ClientVersion,
        adapter_version: AdapterVersion,
        ingress_schema_version: IngressSchemaVersion,
        context_policy_version: ContextPolicyVersion,
        transform_owner: TransformOwnerId,
        transform_owner_version: TransformOwnerVersion,
    ) -> Self {
        Self {
            client_family,
            client_version,
            adapter_version,
            ingress_schema_version,
            context_policy_version,
            transform_owner,
            transform_owner_version,
        }
    }
}

/// Managed 逐请求 Context 门禁的当前事实输入。
pub struct ClassifierManagedContext {
    pub(crate) request: ClassifierRequestBinding,
    pub(crate) ingress: ClassifierIngressBinding,
    pub(crate) issuer_epoch: IssuerEpoch,
    pub(crate) authorization_bundle_digest: AuthorizationBundleDigest,
    pub(crate) route_policy_revision: RoutePolicyRevision,
    pub(crate) consent_revision: ConsentRevision,
    pub(crate) activation: ClassifierManagedActivation,
}

impl ClassifierManagedContext {
    /// 构造 Managed Context 事实输入。
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        request: ClassifierRequestBinding,
        ingress: ClassifierIngressBinding,
        issuer_epoch: IssuerEpoch,
        authorization_bundle_digest: AuthorizationBundleDigest,
        route_policy_revision: RoutePolicyRevision,
        consent_revision: ConsentRevision,
        activation: ClassifierManagedActivation,
    ) -> Self {
        Self {
            request,
            ingress,
            issuer_epoch,
            authorization_bundle_digest,
            route_policy_revision,
            consent_revision,
            activation,
        }
    }
}

/// GatewayOnly 逐请求 Context 门禁的当前本机 consent 事实输入。
pub struct ClassifierGatewayContext {
    pub(crate) request: ClassifierRequestBinding,
    pub(crate) ingress: ClassifierIngressBinding,
    pub(crate) consent_revision: ConsentRevision,
    pub(crate) route_policy_revision: RoutePolicyRevision,
    pub(crate) maximum_trust_tier: u8,
}

impl ClassifierGatewayContext {
    /// 构造 GatewayOnly Context 事实输入。
    pub const fn new(
        request: ClassifierRequestBinding,
        ingress: ClassifierIngressBinding,
        consent_revision: ConsentRevision,
        route_policy_revision: RoutePolicyRevision,
        maximum_trust_tier: u8,
    ) -> Self {
        Self {
            request,
            ingress,
            consent_revision,
            route_policy_revision,
            maximum_trust_tier,
        }
    }
}

/// BoundDeployment 唯一目标的当前不可变事实输入。
///
/// 它同时锁定 Site、Deployment、Endpoint、精确 Origin、AccountSelector、Account、
/// Credential、Adapter 合同与 TrustTier，不包含 Secret。该值只能来自受信 RoutingSnapshot
/// 编译器，不能直接采用客户端字段；快照签发后 classifier 会整体移动这些字段，任何下游都
/// 无法替换账户或凭据。
pub struct ClassifierBoundTarget {
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

impl ClassifierBoundTarget {
    /// 构造唯一 BoundDeployment 目标事实输入。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        site: SiteId,
        deployment: ModelDeploymentId,
        endpoint: EndpointId,
        origin: CanonicalOrigin,
        account_selector: AccountSelectorId,
        account: AccountId,
        credential: CredentialId,
        adapter_contract_revision: AdapterContractRevision,
        trust_tier: u8,
    ) -> Self {
        Self {
            site,
            deployment,
            endpoint,
            origin,
            account_selector,
            account,
            credential,
            adapter_contract_revision,
            trust_tier,
        }
    }
}

/// CapabilityScoped 管理探测的当前事实输入。
pub struct ClassifierCapabilityBound {
    pub(crate) request: ClassifierRequestBinding,
    pub(crate) ingress: ClassifierIngressBinding,
    pub(crate) issuer_epoch: IssuerEpoch,
    pub(crate) management_scope: CapabilityManagementScopeId,
    pub(crate) deadline_millis: u64,
    pub(crate) target: ClassifierBoundTarget,
}

impl ClassifierCapabilityBound {
    /// 构造 CapabilityScoped Bound 事实输入。
    pub const fn new(
        request: ClassifierRequestBinding,
        ingress: ClassifierIngressBinding,
        issuer_epoch: IssuerEpoch,
        management_scope: CapabilityManagementScopeId,
        deadline_millis: u64,
        target: ClassifierBoundTarget,
    ) -> Self {
        Self {
            request,
            ingress,
            issuer_epoch,
            management_scope,
            deadline_millis,
            target,
        }
    }
}

pub(crate) struct LocalSnapshot {
    pub(crate) request: ClassifierRequestBinding,
    pub(crate) listener: ListenerId,
    pub(crate) token_scope: IngressTokenScopeId,
    pub(crate) auth_scope: LocalOperationAuthScope,
}

pub(crate) struct ExactUpstreamBoundSnapshot {
    pub(crate) request: ClassifierRequestBinding,
    pub(crate) target: ClassifierBoundTarget,
}

pub(crate) enum SnapshotMode {
    Local(LocalSnapshot),
    ManagedRouted(ClassifierManagedContext),
    GatewayRouted(ClassifierGatewayContext),
    CapabilityBound(ClassifierCapabilityBound),
    ManagedExact {
        context: ClassifierManagedContext,
        bound: ExactUpstreamBoundSnapshot,
    },
    GatewayExact {
        context: ClassifierGatewayContext,
        bound: ExactUpstreamBoundSnapshot,
    },
}

/// authority 签发、与单个分类器运行时和注册表绑定的逐请求快照。
///
/// 字段私有；本类型不实现 `Clone`、`Debug`、`Default` 或任何 Serde trait。
pub struct ClassifierSnapshot {
    pub(crate) seal: Arc<SnapshotDomainSeal>,
    pub(crate) registry_digest: RegistryDigest,
    pub(crate) mode: SnapshotMode,
}

/// 与单个 [`crate::ClosedRequestClassifier`] 配对的快照签发 authority。
///
/// 本类型只能由 [`crate::ClassifierRuntime::initialize`] 产生，且不实现 `Clone`。普通调用方
/// 即使能够构造事实输入，也不能伪造另一个分类器实例可接受的 [`ClassifierSnapshot`]。
pub struct ClassifierSnapshotAuthority {
    seal: Arc<SnapshotDomainSeal>,
    registry_digest: RegistryDigest,
}

impl ClassifierSnapshotAuthority {
    pub(crate) const fn new(
        seal: Arc<SnapshotDomainSeal>,
        registry_digest: RegistryDigest,
    ) -> Self {
        Self {
            seal,
            registry_digest,
        }
    }

    /// 签发仅允许匹配 Local Operation/scope 的逐请求快照。
    pub fn issue_local(
        &self,
        request: ClassifierRequestBinding,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        auth_scope: LocalOperationAuthScope,
    ) -> Result<ClassifierSnapshot, RouteReject> {
        validate_request(&request)?;
        validate_nonzero(listener.get())?;
        validate_nonzero(token_scope.get())?;
        Ok(self.snapshot(SnapshotMode::Local(LocalSnapshot {
            request,
            listener,
            token_scope,
            auth_scope,
        })))
    }

    /// 签发 Managed RoutedPolicy 的逐请求快照。
    pub fn issue_managed_routed(
        &self,
        context: ClassifierManagedContext,
    ) -> Result<ClassifierSnapshot, RouteReject> {
        validate_managed_context(&context)?;
        Ok(self.snapshot(SnapshotMode::ManagedRouted(context)))
    }

    /// 签发 GatewayOnly RoutedPolicy 的逐请求快照。
    pub fn issue_gateway_routed(
        &self,
        context: ClassifierGatewayContext,
    ) -> Result<ClassifierSnapshot, RouteReject> {
        validate_gateway_context(&context)?;
        Ok(self.snapshot(SnapshotMode::GatewayRouted(context)))
    }

    /// 签发 CapabilityScoped DeploymentModelProbe 的逐请求快照。
    pub fn issue_capability_bound(
        &self,
        capability: ClassifierCapabilityBound,
    ) -> Result<ClassifierSnapshot, RouteReject> {
        validate_capability(&capability)?;
        Ok(self.snapshot(SnapshotMode::CapabilityBound(capability)))
    }

    /// 签发必须同时消费 Managed Context 与 Bound grant 的 ExactUpstream 快照。
    ///
    /// `target` 是受信 RoutingSnapshot 已完成账户选择后的唯一事实；Context permit 再独立
    /// 限制 Site、Deployment、Origin、TrustTier 与 Adapter 合同。两者必须同时通过，输出
    /// 才会固化 Endpoint、AccountSelector、Account 与 Credential。
    pub fn issue_managed_exact_upstream(
        &self,
        context: ClassifierManagedContext,
        bound_request: ClassifierRequestBinding,
        target: ClassifierBoundTarget,
    ) -> Result<ClassifierSnapshot, RouteReject> {
        validate_managed_context(&context)?;
        validate_request(&bound_request)?;
        validate_target(&target)?;
        if !same_request(&context.request, &bound_request) {
            return Err(RouteReject::InvalidSnapshot);
        }
        Ok(self.snapshot(SnapshotMode::ManagedExact {
            context,
            bound: ExactUpstreamBoundSnapshot {
                request: bound_request,
                target,
            },
        }))
    }

    /// 签发必须同时消费 GatewayOnly Context 与 Bound grant 的 ExactUpstream 快照。
    ///
    /// `target` 是受信 RoutingSnapshot 已完成账户选择后的唯一事实，客户端输入无权构造；
    /// classifier 还会以 consent 的最高 TrustTier 复核它。
    pub fn issue_gateway_exact_upstream(
        &self,
        context: ClassifierGatewayContext,
        bound_request: ClassifierRequestBinding,
        target: ClassifierBoundTarget,
    ) -> Result<ClassifierSnapshot, RouteReject> {
        validate_gateway_context(&context)?;
        validate_request(&bound_request)?;
        validate_target(&target)?;
        if !same_request(&context.request, &bound_request) {
            return Err(RouteReject::InvalidSnapshot);
        }
        Ok(self.snapshot(SnapshotMode::GatewayExact {
            context,
            bound: ExactUpstreamBoundSnapshot {
                request: bound_request,
                target,
            },
        }))
    }

    fn snapshot(&self, mode: SnapshotMode) -> ClassifierSnapshot {
        ClassifierSnapshot {
            seal: Arc::clone(&self.seal),
            registry_digest: self.registry_digest,
            mode,
        }
    }
}

pub(crate) fn validate_registry_digest(digest: RegistryDigest) -> Result<(), RouteReject> {
    if digest.as_bytes() == &[0; 32] {
        return Err(RouteReject::InvalidSnapshot);
    }
    Ok(())
}

fn validate_request(request: &ClassifierRequestBinding) -> Result<(), RouteReject> {
    validate_nonzero(request.operation.get())?;
    if request.request_digest.as_bytes() == &[0; 32] {
        return Err(RouteReject::InvalidSnapshot);
    }
    Ok(())
}

fn validate_ingress(ingress: &ClassifierIngressBinding) -> Result<(), RouteReject> {
    validate_nonzero(ingress.listener.get())?;
    validate_nonzero(ingress.token_scope.get())?;
    validate_nonzero(ingress.audience.get())
}

fn validate_activation(activation: &ClassifierManagedActivation) -> Result<(), RouteReject> {
    for value in [
        activation.client_family.get(),
        activation.client_version.get(),
        activation.adapter_version.get(),
        activation.ingress_schema_version.get(),
        activation.context_policy_version.get(),
        activation.transform_owner.get(),
        activation.transform_owner_version.get(),
    ] {
        validate_nonzero(value)?;
    }
    Ok(())
}

fn validate_managed_context(context: &ClassifierManagedContext) -> Result<(), RouteReject> {
    validate_request(&context.request)?;
    validate_ingress(&context.ingress)?;
    validate_nonzero(context.issuer_epoch.get())?;
    validate_nonzero(context.route_policy_revision.get())?;
    validate_nonzero(context.consent_revision.get())?;
    if context.authorization_bundle_digest.as_bytes() == &[0; 32] {
        return Err(RouteReject::InvalidSnapshot);
    }
    validate_activation(&context.activation)
}

fn validate_gateway_context(context: &ClassifierGatewayContext) -> Result<(), RouteReject> {
    validate_request(&context.request)?;
    validate_ingress(&context.ingress)?;
    validate_nonzero(context.route_policy_revision.get())?;
    validate_nonzero(context.consent_revision.get())
}

fn validate_target(target: &ClassifierBoundTarget) -> Result<(), RouteReject> {
    for value in [
        target.site.get(),
        target.deployment.get(),
        target.endpoint.get(),
        target.account_selector.get(),
        target.account.get(),
        target.credential.get(),
        target.adapter_contract_revision.get(),
    ] {
        validate_nonzero(value)?;
    }
    if target.origin.as_bytes().is_empty() {
        return Err(RouteReject::InvalidSnapshot);
    }
    Ok(())
}

fn validate_capability(capability: &ClassifierCapabilityBound) -> Result<(), RouteReject> {
    validate_request(&capability.request)?;
    validate_ingress(&capability.ingress)?;
    validate_nonzero(capability.issuer_epoch.get())?;
    validate_nonzero(capability.management_scope.get())?;
    validate_nonzero(capability.deadline_millis)?;
    validate_target(&capability.target)
}

fn same_request(left: &ClassifierRequestBinding, right: &ClassifierRequestBinding) -> bool {
    left.operation == right.operation && left.request_digest == right.request_digest
}

const fn validate_nonzero(value: u64) -> Result<(), RouteReject> {
    if value == 0 {
        Err(RouteReject::InvalidSnapshot)
    } else {
        Ok(())
    }
}
