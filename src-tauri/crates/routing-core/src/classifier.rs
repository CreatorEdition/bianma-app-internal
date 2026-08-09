//! receiver、Normalizer 与三路逐请求门禁的唯一生产编排入口。

use std::sync::Arc;

use ingress_contract::verified::{
    ReceiverAcceptedIngressRequest, VerifiedAuthorizationKind, VerifiedCapabilityBindingView,
    VerifiedEgressPurpose, VerifiedProofKind,
};
use ingress_contract::{
    LocalOperationAuthScope, RegistryDigest, RequestDispatchDomain, RequestKind,
    VerifiedIngressReceiver, VerifiedIngressRequest,
};

use super::clock::ClassifierClock;
use super::disposition::{
    BoundDeploymentGrant, BoundTarget, ClassifiedRequest, ContextExecutionGrant, GrantBinding,
    VerifiedBoundDeploymentRequest, VerifiedIngressDisposition, VerifiedLocalDispatch,
    VerifiedRouteRequest,
};
use super::error::RouteReject;
use super::normalizer::{
    ClientProtocolNormalizer, NormalizedRequestSemantic, NormalizerInput, ProtocolNormalizedRequest,
};
use super::snapshot::{
    validate_registry_digest, ClassifierBoundTarget, ClassifierCapabilityBound,
    ClassifierGatewayContext, ClassifierManagedContext, ClassifierRequestBinding,
    ClassifierSnapshot, ClassifierSnapshotAuthority, ExactUpstreamBoundSnapshot, LocalSnapshot,
    SnapshotDomainSeal, SnapshotMode,
};

pub(crate) struct DispositionConstructionSeal {
    _private: (),
}

impl DispositionConstructionSeal {
    const fn new() -> Self {
        Self { _private: () }
    }
}

/// 同时创建封闭分类器与其唯一配对快照 authority 的 composition-root 容器。
pub struct ClassifierRuntime {
    classifier: ClosedRequestClassifier,
    snapshot_authority: ClassifierSnapshotAuthority,
}

impl ClassifierRuntime {
    /// 初始化单个分类器运行时。
    ///
    /// `receiver` 被分类器独占；Normalizer 与墙钟必须是无外部副作用、可并发调用的受信
    /// 实现。`registry_digest` 必须与创建 receiver 的同一 RouteSpec 注册表一致，否则首个
    /// 请求会在 Normalizer 之前 fail closed。
    pub fn initialize<N, C>(
        receiver: VerifiedIngressReceiver,
        registry_digest: RegistryDigest,
        normalizer: N,
        clock: C,
    ) -> Result<Self, RouteReject>
    where
        N: ClientProtocolNormalizer + 'static,
        C: ClassifierClock + 'static,
    {
        validate_registry_digest(registry_digest)?;
        let snapshot_seal = Arc::new(SnapshotDomainSeal::new());
        let classifier = ClosedRequestClassifier {
            receiver,
            registry_digest,
            snapshot_seal: Arc::clone(&snapshot_seal),
            normalizer: Box::new(normalizer),
            clock: Box::new(clock),
        };
        let snapshot_authority = ClassifierSnapshotAuthority::new(snapshot_seal, registry_digest);
        Ok(Self {
            classifier,
            snapshot_authority,
        })
    }

    /// 将运行时拆为封闭分类器与配对 authority。
    pub fn into_parts(self) -> (ClosedRequestClassifier, ClassifierSnapshotAuthority) {
        (self.classifier, self.snapshot_authority)
    }
}

/// 独占 ingress receiver 的封闭请求分类器。
///
/// 本类型没有接收 `RawIngressRequest` 或 receiver-accepted 中间态的公开入口；生产调用方
/// 只能提交 [`VerifiedIngressRequest`]，并由此方法首先调用 receiver。
pub struct ClosedRequestClassifier {
    receiver: VerifiedIngressReceiver,
    registry_digest: RegistryDigest,
    snapshot_seal: Arc<SnapshotDomainSeal>,
    normalizer: Box<dyn ClientProtocolNormalizer>,
    clock: Box<dyn ClassifierClock>,
}

impl ClosedRequestClassifier {
    /// 接受、规范化并分类单个 Verified 请求。
    ///
    /// 执行顺序固定为 receiver accept → snapshot runtime/registry/request 绑定 → Normalizer
    /// → Local/Context/Bound gate。registry 漂移必定发生在 Normalizer 与任何下游门禁前。
    pub fn classify(
        &self,
        request: VerifiedIngressRequest,
        snapshot: &ClassifierSnapshot,
    ) -> Result<VerifiedIngressDisposition, RouteReject> {
        let accepted = self.receiver.accept(request)?;
        self.validate_snapshot_before_normalizer(&accepted, snapshot)?;

        let normalized = self.normalizer.normalize(NormalizerInput::new(&accepted))?;
        validate_normalized_binding(&accepted, &normalized)?;

        let now_millis = self.clock.now_millis()?;
        let classified = ClassifiedRequest::new(accepted, normalized);
        self.apply_closed_gates(classified, snapshot, now_millis)
    }

    fn validate_snapshot_before_normalizer(
        &self,
        accepted: &ReceiverAcceptedIngressRequest,
        snapshot: &ClassifierSnapshot,
    ) -> Result<(), RouteReject> {
        if !Arc::ptr_eq(&self.snapshot_seal, &snapshot.seal) {
            return Err(RouteReject::SnapshotDomainMismatch);
        }
        if accepted.registry_digest() != self.registry_digest
            || snapshot.registry_digest != self.registry_digest
        {
            return Err(RouteReject::RegistryMismatch);
        }
        let request = snapshot_request(&snapshot.mode);
        if request.operation != accepted.operation()
            || request.request_digest != accepted.request_digest()
        {
            return Err(RouteReject::SnapshotBindingMismatch);
        }
        Ok(())
    }

    fn apply_closed_gates(
        &self,
        request: ClassifiedRequest,
        snapshot: &ClassifierSnapshot,
        now_millis: u64,
    ) -> Result<VerifiedIngressDisposition, RouteReject> {
        match (
            &snapshot.mode,
            request.request_kind(),
            request.dispatch_domain(),
        ) {
            (SnapshotMode::Local(local), kind, RequestDispatchDomain::Local)
                if is_local_kind(kind) =>
            {
                gate_local(&request, local)?;
                Ok(VerifiedIngressDisposition::Local(
                    VerifiedLocalDispatch::construct(
                        request,
                        local.auth_scope,
                        DispositionConstructionSeal::new(),
                    ),
                ))
            }
            (
                SnapshotMode::CapabilityBound(capability),
                RequestKind::DeploymentModelProbe,
                RequestDispatchDomain::BoundDeployment,
            ) => {
                let grant = gate_capability_bound(&request, capability, now_millis)?;
                Ok(VerifiedIngressDisposition::BoundDeployment(
                    VerifiedBoundDeploymentRequest::construct(
                        request,
                        grant,
                        DispositionConstructionSeal::new(),
                    )?,
                ))
            }
            (
                SnapshotMode::ManagedExact { context, bound },
                RequestKind::ExactUpstreamTokenCount,
                RequestDispatchDomain::BoundDeployment,
            ) => {
                require_managed_proof(&request)?;
                let context_grant = gate_managed_context(&request, context, now_millis)?;
                let grant = gate_exact_upstream_bound(&request, bound, context_grant)?;
                Ok(VerifiedIngressDisposition::BoundDeployment(
                    VerifiedBoundDeploymentRequest::construct(
                        request,
                        grant,
                        DispositionConstructionSeal::new(),
                    )?,
                ))
            }
            (
                SnapshotMode::GatewayExact { context, bound },
                RequestKind::ExactUpstreamTokenCount,
                RequestDispatchDomain::BoundDeployment,
            ) => {
                require_gateway_proof(&request)?;
                let context_grant = gate_gateway_context(&request, context, now_millis)?;
                let grant = gate_exact_upstream_bound(&request, bound, context_grant)?;
                Ok(VerifiedIngressDisposition::BoundDeployment(
                    VerifiedBoundDeploymentRequest::construct(
                        request,
                        grant,
                        DispositionConstructionSeal::new(),
                    )?,
                ))
            }
            (
                SnapshotMode::ManagedRouted(context),
                RequestKind::ModelInference | RequestKind::AuxiliaryInference,
                RequestDispatchDomain::RoutedPolicy,
            ) => {
                require_managed_proof(&request)?;
                let context = gate_managed_context(&request, context, now_millis)?;
                Ok(VerifiedIngressDisposition::Routed(
                    VerifiedRouteRequest::construct(
                        request,
                        context,
                        DispositionConstructionSeal::new(),
                    )?,
                ))
            }
            (
                SnapshotMode::GatewayRouted(context),
                RequestKind::ModelInference | RequestKind::AuxiliaryInference,
                RequestDispatchDomain::RoutedPolicy,
            ) => {
                require_gateway_proof(&request)?;
                let context = gate_gateway_context(&request, context, now_millis)?;
                Ok(VerifiedIngressDisposition::Routed(
                    VerifiedRouteRequest::construct(
                        request,
                        context,
                        DispositionConstructionSeal::new(),
                    )?,
                ))
            }
            _ => Err(RouteReject::DispositionNotAllowed),
        }
    }
}

fn snapshot_request(mode: &SnapshotMode) -> &ClassifierRequestBinding {
    match mode {
        SnapshotMode::Local(snapshot) => &snapshot.request,
        SnapshotMode::ManagedRouted(snapshot) => &snapshot.request,
        SnapshotMode::GatewayRouted(snapshot) => &snapshot.request,
        SnapshotMode::CapabilityBound(snapshot) => &snapshot.request,
        SnapshotMode::ManagedExact { context, .. } => &context.request,
        SnapshotMode::GatewayExact { context, .. } => &context.request,
    }
}

fn validate_normalized_binding(
    accepted: &ReceiverAcceptedIngressRequest,
    normalized: &ProtocolNormalizedRequest,
) -> Result<(), RouteReject> {
    if normalized.operation() != accepted.operation()
        || normalized.request_kind() != accepted.request_kind()
        || normalized.dispatch_domain() != accepted.dispatch_domain()
        || normalized.ingress_protocol() != accepted.ingress_protocol()
        || normalized.request_digest() != accepted.request_digest()
        || normalized.semantic() != expected_semantic(accepted.request_kind())
    {
        return Err(RouteReject::NormalizedBindingMismatch);
    }
    Ok(())
}

const fn expected_semantic(kind: RequestKind) -> NormalizedRequestSemantic {
    match kind {
        RequestKind::Liveness
        | RequestKind::LocalAdmin
        | RequestKind::AuthFlow
        | RequestKind::UnifiedModelCatalog
        | RequestKind::ExactLocalTokenCount
        | RequestKind::EstimatedLocalTokenCount
        | RequestKind::LocalContextCompact => NormalizedRequestSemantic::LocalOperation,
        RequestKind::DeploymentModelProbe => NormalizedRequestSemantic::DeploymentModelProbe,
        RequestKind::ExactUpstreamTokenCount => NormalizedRequestSemantic::ExactUpstreamTokenCount,
        RequestKind::ModelInference => NormalizedRequestSemantic::ModelInference,
        RequestKind::AuxiliaryInference => NormalizedRequestSemantic::AuxiliaryInference,
    }
}

const fn is_local_kind(kind: RequestKind) -> bool {
    matches!(
        kind,
        RequestKind::Liveness
            | RequestKind::LocalAdmin
            | RequestKind::AuthFlow
            | RequestKind::UnifiedModelCatalog
            | RequestKind::ExactLocalTokenCount
            | RequestKind::EstimatedLocalTokenCount
            | RequestKind::LocalContextCompact
    )
}

const fn expected_local_scope(kind: RequestKind) -> Option<LocalOperationAuthScope> {
    match kind {
        RequestKind::Liveness => Some(LocalOperationAuthScope::PublicLiveness),
        RequestKind::LocalAdmin => Some(LocalOperationAuthScope::LocalAdmin),
        RequestKind::AuthFlow => Some(LocalOperationAuthScope::AuthFlow),
        RequestKind::UnifiedModelCatalog
        | RequestKind::ExactLocalTokenCount
        | RequestKind::EstimatedLocalTokenCount
        | RequestKind::LocalContextCompact => Some(LocalOperationAuthScope::LocalData),
        RequestKind::DeploymentModelProbe
        | RequestKind::ExactUpstreamTokenCount
        | RequestKind::ModelInference
        | RequestKind::AuxiliaryInference => None,
    }
}

fn gate_local(request: &ClassifiedRequest, snapshot: &LocalSnapshot) -> Result<(), RouteReject> {
    if request.accepted.proof_kind() != VerifiedProofKind::LocalOperationScoped
        || request.accepted.authorization_kind() != VerifiedAuthorizationKind::None
        || request.accepted.local_handle_allowed()
        || request.accepted.local_auth_scope() != Some(snapshot.auth_scope)
        || expected_local_scope(request.request_kind()) != Some(snapshot.auth_scope)
        || request.accepted.listener_scope() != (snapshot.listener, snapshot.token_scope)
    {
        return Err(RouteReject::LocalScopeMismatch);
    }
    Ok(())
}

fn require_managed_proof(request: &ClassifiedRequest) -> Result<(), RouteReject> {
    if request.accepted.proof_kind() != VerifiedProofKind::Managed
        || request.accepted.authorization_kind() != VerifiedAuthorizationKind::ManagedEgress
        || request.accepted.managed().is_none()
        || request.accepted.gateway_only().is_some()
        || request.accepted.capability_binding().is_some()
    {
        return Err(RouteReject::ContextGateRejected);
    }
    Ok(())
}

fn require_gateway_proof(request: &ClassifiedRequest) -> Result<(), RouteReject> {
    if request.accepted.proof_kind() != VerifiedProofKind::GatewayOnlyScopedConsent
        || request.accepted.authorization_kind() != VerifiedAuthorizationKind::GatewayOnlyExplicit
        || request.accepted.gateway_only().is_none()
        || request.accepted.managed().is_some()
        || request.accepted.capability_binding().is_some()
        || request.accepted.local_handle_allowed()
    {
        return Err(RouteReject::ContextGateRejected);
    }
    Ok(())
}

fn gate_managed_context(
    request: &ClassifiedRequest,
    snapshot: &ClassifierManagedContext,
    now_millis: u64,
) -> Result<ContextExecutionGrant, RouteReject> {
    let view = request
        .accepted
        .managed()
        .ok_or(RouteReject::ContextGateRejected)?;
    let activation = view.activation_key();
    let permit = view.egress_permit();
    let requirements = view.capability_requirements();

    if request.accepted.listener_scope()
        != (snapshot.ingress.listener, snapshot.ingress.token_scope)
        || view.listener() != snapshot.ingress.listener
        || view.token_scope() != snapshot.ingress.token_scope
        || view.audience() != snapshot.ingress.audience
        || view.issuer_epoch() != snapshot.issuer_epoch
        || view.registry_digest() != request.accepted.registry_digest()
        || view.authorization_bundle_digest() != snapshot.authorization_bundle_digest
        || view.route_policy_revision() != snapshot.route_policy_revision
        || view.consent_revision() != snapshot.consent_revision
        || view.adapter_version() != snapshot.activation.adapter_version
        || view.transform_owner() != snapshot.activation.transform_owner
        || view.transform_owner_version() != snapshot.activation.transform_owner_version
        || activation.client_family() != snapshot.activation.client_family
        || activation.client_version() != snapshot.activation.client_version
        || activation.adapter_version() != snapshot.activation.adapter_version
        || activation.ingress_schema_version() != snapshot.activation.ingress_schema_version
        || activation.context_policy_version() != snapshot.activation.context_policy_version
        || activation.transform_owner() != snapshot.activation.transform_owner
        || activation.transform_owner_version() != snapshot.activation.transform_owner_version
        || permit.operation() != request.operation()
        || permit.request_digest() != request.request_digest()
        || permit.route_policy_revision() != snapshot.route_policy_revision
        || permit.consent_revision() != snapshot.consent_revision
        || permit.target_count() == 0
        || requirements.client_adapter_version() != snapshot.activation.adapter_version
        || !managed_purpose_matches(request.request_kind(), permit.purpose())
    {
        return Err(RouteReject::ContextGateRejected);
    }

    validate_time_window(
        view.issued_at_millis(),
        view.expires_at_millis(),
        now_millis,
    )?;
    if permit.expires_at_millis() <= now_millis
        || permit.expires_at_millis() > view.expires_at_millis()
    {
        return Err(RouteReject::AuthorizationExpired);
    }

    Ok(ContextExecutionGrant::Managed {
        binding: GrantBinding::from_request(request),
        route_policy_revision: snapshot.route_policy_revision,
        consent_revision: snapshot.consent_revision,
        expires_at_millis: permit.expires_at_millis(),
    })
}

fn gate_gateway_context(
    request: &ClassifiedRequest,
    snapshot: &ClassifierGatewayContext,
    now_millis: u64,
) -> Result<ContextExecutionGrant, RouteReject> {
    let view = request
        .accepted
        .gateway_only()
        .ok_or(RouteReject::ContextGateRejected)?;
    if request.accepted.listener_scope()
        != (snapshot.ingress.listener, snapshot.ingress.token_scope)
        || view.listener() != snapshot.ingress.listener
        || view.token_scope() != snapshot.ingress.token_scope
        || view.audience() != snapshot.ingress.audience
        || view.registry_digest() != request.accepted.registry_digest()
        || view.request_digest() != request.request_digest()
        || view.consent_revision() != snapshot.consent_revision
        || view.route_policy_revision() != snapshot.route_policy_revision
        || view.maximum_trust_tier() != snapshot.maximum_trust_tier
        || view.local_handle_allowed()
        || request.accepted.local_handle_allowed()
    {
        return Err(RouteReject::ContextGateRejected);
    }
    validate_time_window(
        view.issued_at_millis(),
        view.expires_at_millis(),
        now_millis,
    )?;
    Ok(ContextExecutionGrant::GatewayOnly {
        binding: GrantBinding::from_request(request),
        route_policy_revision: snapshot.route_policy_revision,
        consent_revision: snapshot.consent_revision,
        maximum_trust_tier: snapshot.maximum_trust_tier,
        expires_at_millis: view.expires_at_millis(),
    })
}

fn gate_capability_bound(
    request: &ClassifiedRequest,
    snapshot: &ClassifierCapabilityBound,
    now_millis: u64,
) -> Result<BoundDeploymentGrant, RouteReject> {
    if request.accepted.proof_kind() != VerifiedProofKind::CapabilityScoped
        || request.accepted.authorization_kind() != VerifiedAuthorizationKind::CapabilityBound
        || request.accepted.managed().is_some()
        || request.accepted.gateway_only().is_some()
        || request.accepted.local_auth_scope().is_some()
        || request.accepted.local_handle_allowed()
    {
        return Err(RouteReject::BoundGateRejected);
    }
    let binding = request
        .accepted
        .capability_binding()
        .ok_or(RouteReject::BoundGateRejected)?;
    validate_capability_binding(request, snapshot, &binding, now_millis)?;
    Ok(BoundDeploymentGrant {
        binding: GrantBinding::from_request(request),
        target: BoundTarget::from_snapshot(&snapshot.target),
        context: None,
        management_scope: Some(snapshot.management_scope),
        deadline_millis: snapshot.deadline_millis,
    })
}

fn validate_capability_binding(
    request: &ClassifiedRequest,
    snapshot: &ClassifierCapabilityBound,
    binding: &VerifiedCapabilityBindingView<'_>,
    now_millis: u64,
) -> Result<(), RouteReject> {
    if request.accepted.listener_scope()
        != (snapshot.ingress.listener, snapshot.ingress.token_scope)
        || binding.listener() != snapshot.ingress.listener
        || binding.token_scope() != snapshot.ingress.token_scope
        || binding.audience() != snapshot.ingress.audience
        || binding.issuer_epoch() != snapshot.issuer_epoch
        || binding.registry_digest() != request.accepted.registry_digest()
        || binding.request_digest() != request.request_digest()
        || binding.site() != snapshot.target.site
        || binding.deployment() != snapshot.target.deployment
        || binding.endpoint() != snapshot.target.endpoint
        || binding.origin() != &snapshot.target.origin
        || binding.account_selector() != snapshot.target.account_selector
        || binding.account() != snapshot.target.account
        || binding.credential() != snapshot.target.credential
        || binding.adapter_contract_revision() != snapshot.target.adapter_contract_revision
        || binding.trust_tier() != snapshot.target.trust_tier
        || binding.management_scope() != snapshot.management_scope
        || binding.deadline_millis() != snapshot.deadline_millis
        || !binding.fallback_forbidden()
    {
        return Err(RouteReject::BoundGateRejected);
    }
    validate_time_window(
        binding.issued_at_millis(),
        binding.deadline_millis(),
        now_millis,
    )
}

fn gate_exact_upstream_bound(
    request: &ClassifiedRequest,
    snapshot: &ExactUpstreamBoundSnapshot,
    context: ContextExecutionGrant,
) -> Result<BoundDeploymentGrant, RouteReject> {
    if snapshot.request.operation != request.operation()
        || snapshot.request.request_digest != request.request_digest()
        || !context.binding().matches(request)
    {
        return Err(RouteReject::BoundGateRejected);
    }
    match &context {
        ContextExecutionGrant::Managed { .. } => {
            validate_managed_exact_target(request, &snapshot.target)?;
        }
        ContextExecutionGrant::GatewayOnly {
            maximum_trust_tier, ..
        } => {
            let gateway = request
                .accepted
                .gateway_only()
                .ok_or(RouteReject::BoundGateRejected)?;
            if gateway.local_handle_allowed()
                || request.accepted.local_handle_allowed()
                || snapshot.target.trust_tier > *maximum_trust_tier
            {
                return Err(RouteReject::BoundGateRejected);
            }
        }
    }
    let deadline_millis = context.expires_at_millis();
    Ok(BoundDeploymentGrant {
        binding: GrantBinding::from_request(request),
        target: BoundTarget::from_snapshot(&snapshot.target),
        context: Some(context),
        management_scope: None,
        deadline_millis,
    })
}

fn validate_managed_exact_target(
    request: &ClassifiedRequest,
    target: &ClassifierBoundTarget,
) -> Result<(), RouteReject> {
    let managed = request
        .accepted
        .managed()
        .ok_or(RouteReject::BoundGateRejected)?;
    let permit = managed.egress_permit();
    let requirements = managed.capability_requirements();
    if permit.purpose() != VerifiedEgressPurpose::ExactUpstreamTokenCount
        || permit.target_count() != 1
        || permit.fallback_allowed()
        || requirements.upstream_adapter_revision() != target.adapter_contract_revision
    {
        return Err(RouteReject::BoundGateRejected);
    }
    let mut targets = permit.targets();
    let permitted = targets.next().ok_or(RouteReject::BoundGateRejected)?;
    if targets.next().is_some()
        || permitted.site() != target.site
        || permitted.deployment() != target.deployment
        || permitted.origin() != &target.origin
        || permitted.trust_tier() != target.trust_tier
    {
        return Err(RouteReject::BoundGateRejected);
    }
    Ok(())
}

const fn managed_purpose_matches(kind: RequestKind, purpose: VerifiedEgressPurpose) -> bool {
    matches!(
        (kind, purpose),
        (
            RequestKind::ModelInference,
            VerifiedEgressPurpose::ModelInference
        ) | (
            RequestKind::AuxiliaryInference,
            VerifiedEgressPurpose::AuxiliaryInference
        ) | (
            RequestKind::ExactUpstreamTokenCount,
            VerifiedEgressPurpose::ExactUpstreamTokenCount
        )
    )
}

fn validate_time_window(
    issued_at_millis: u64,
    expires_at_millis: u64,
    now_millis: u64,
) -> Result<(), RouteReject> {
    if issued_at_millis > now_millis || expires_at_millis <= now_millis {
        return Err(RouteReject::AuthorizationExpired);
    }
    Ok(())
}
