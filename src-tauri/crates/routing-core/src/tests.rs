//! 封闭分类器合同测试；测试签发器仅存在于本模块。

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use ingress_contract::{
    AccountId, AccountSelectorId, AdapterContractRevision, AdapterVersion, AudienceId, BodyPolicy,
    CanonicalOrigin, CapabilityManagementScopeId, CapabilityVerificationKey,
    CapabilityVerificationKeyRing, ClientFamilyId, ClientVersion, ConsentRevision,
    ContextActivationBinding, ContextPolicyVersion, CredentialId, EncodedCapabilityAuthorization,
    EndpointId, FixedClock, HttpMethod, IngressProtocol, IngressSchemaVersion, IngressTokenScopeId,
    IngressVerifier, IssuerEpoch, ListenerId, LocalOperationAuthScope, ManagedVerificationKey,
    ManagedVerificationKeyRing, MemoryNonceStore, ModelDeploymentId, OneShotNonceStore,
    OperationId, QueryPolicy, RawHeader, RawIngressRequest, RegistryDigest, RequestDigest,
    RequestDispatchDomain, RequestKind, RoutePolicyRevision, RouteSpec, RouteSpecRegistry,
    SignedIngressRequest, SiteId, TransformOwnerId, TransformOwnerVersion,
};

use crate::{
    BoundFallbackPolicy, ClassifierBoundTarget, ClassifierCapabilityBound,
    ClassifierGatewayContext, ClassifierIngressBinding, ClassifierManagedActivation,
    ClassifierManagedContext, ClassifierRequestBinding, ClassifierRuntime,
    ClassifierSnapshotAuthority, ClientProtocolNormalizer, ClosedRequestClassifier,
    ContextExecutionMode, NormalizedRequestSemantic, NormalizerInput, ProtocolNormalizeError,
    ProtocolNormalizedRequest, RouteReject, VerifiedIngressDisposition,
};

const NOW: u64 = 1_800_000_000_000;
const EXPIRES: u64 = NOW + 60_000;
const MANAGED_KEY: [u8; 32] = [0x11; 32];
const CAPABILITY_KEY: [u8; 32] = [0x21; 32];

const LISTENER: ListenerId = ListenerId::new(91);
const TOKEN_SCOPE: IngressTokenScopeId = IngressTokenScopeId::new(92);
const AUDIENCE: AudienceId = AudienceId::new(90);
const ISSUER: IssuerEpoch = IssuerEpoch::new(1);
const POLICY: RoutePolicyRevision = RoutePolicyRevision::new(70);
const CONSENT: ConsentRevision = ConsentRevision::new(80);

const CAP_LISTENER: ListenerId = ListenerId::new(400);
const CAP_TOKEN: IngressTokenScopeId = IngressTokenScopeId::new(401);
const CAP_AUDIENCE: AudienceId = AudienceId::new(402);
const CAP_SCOPE: CapabilityManagementScopeId = CapabilityManagementScopeId::new(403);

type LocalCase = (
    u64,
    HttpMethod,
    &'static [u8],
    &'static [u8],
    LocalOperationAuthScope,
);

struct FixedClassifierClock;

impl crate::ClassifierClock for FixedClassifierClock {
    fn now_millis(&self) -> Result<u64, RouteReject> {
        Ok(NOW)
    }
}

struct CountingNormalizer {
    calls: Arc<AtomicUsize>,
    behavior: NormalizerBehavior,
}

#[derive(Clone, Copy)]
enum NormalizerBehavior {
    Expected,
    Reject(ProtocolNormalizeError),
    Force(NormalizedRequestSemantic),
}

impl ClientProtocolNormalizer for CountingNormalizer {
    fn normalize(
        &self,
        input: NormalizerInput<'_>,
    ) -> Result<ProtocolNormalizedRequest, ProtocolNormalizeError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let NormalizerBehavior::Reject(error) = self.behavior {
            return Err(error);
        }
        let semantic = match input.request_kind() {
            RequestKind::Liveness
            | RequestKind::LocalAdmin
            | RequestKind::AuthFlow
            | RequestKind::UnifiedModelCatalog
            | RequestKind::ExactLocalTokenCount
            | RequestKind::EstimatedLocalTokenCount
            | RequestKind::LocalContextCompact => NormalizedRequestSemantic::LocalOperation,
            RequestKind::DeploymentModelProbe => NormalizedRequestSemantic::DeploymentModelProbe,
            RequestKind::ExactUpstreamTokenCount => {
                NormalizedRequestSemantic::ExactUpstreamTokenCount
            }
            RequestKind::ModelInference => NormalizedRequestSemantic::ModelInference,
            RequestKind::AuxiliaryInference => NormalizedRequestSemantic::AuxiliaryInference,
        };
        let semantic = match self.behavior {
            NormalizerBehavior::Expected | NormalizerBehavior::Reject(_) => semantic,
            NormalizerBehavior::Force(forced) => forced,
        };
        Ok(ProtocolNormalizedRequest::accept(&input, semantic))
    }
}

struct RuntimeHarness {
    verifier: IngressVerifier,
    listener_authority: ingress_contract::ListenerBindingAuthority,
    consent_authority: ingress_contract::GatewayConsentAuthority,
    activation_authority: ingress_contract::ManagedActivationAuthority,
    classifier: ClosedRequestClassifier,
    snapshots: ClassifierSnapshotAuthority,
    registry_digest: RegistryDigest,
    calls: Arc<AtomicUsize>,
}

fn build_harness() -> RuntimeHarness {
    build_harness_with_behavior(NormalizerBehavior::Expected)
}

fn build_harness_with_behavior(behavior: NormalizerBehavior) -> RuntimeHarness {
    let registry = build_registry();
    let registry_digest = registry.digest();
    let runtime = IngressVerifier::initialize(
        registry,
        ManagedVerificationKeyRing::try_new(vec![ManagedVerificationKey::try_new(
            ISSUER,
            MANAGED_KEY,
        )
        .expect("Managed key 合法")])
        .expect("Managed key ring 合法"),
        CapabilityVerificationKeyRing::try_new(vec![CapabilityVerificationKey::try_new(
            ISSUER,
            CAPABILITY_KEY,
        )
        .expect("Capability key 合法")])
        .expect("Capability key ring 合法"),
        Arc::new(FixedClock::new(NOW)),
        Arc::new(MemoryNonceStore::new(4_096).expect("nonce store 合法"))
            as Arc<dyn OneShotNonceStore>,
    );
    let (verifier, listener_authority, consent_authority, activation_authority, receiver) =
        runtime.into_parts();
    let calls = Arc::new(AtomicUsize::new(0));
    let classifier_runtime = ClassifierRuntime::initialize(
        receiver,
        registry_digest,
        CountingNormalizer {
            calls: Arc::clone(&calls),
            behavior,
        },
        FixedClassifierClock,
    )
    .expect("分类器 runtime 合法");
    let (classifier, snapshots) = classifier_runtime.into_parts();
    RuntimeHarness {
        verifier,
        listener_authority,
        consent_authority,
        activation_authority,
        classifier,
        snapshots,
        registry_digest,
        calls,
    }
}

fn build_registry() -> RouteSpecRegistry {
    let json = || BodyPolicy::bounded(32 * 1024, Some(b"application/json")).expect("JSON 策略合法");
    RouteSpecRegistry::compile(vec![
        route(
            1,
            HttpMethod::Post,
            b"/v1/messages",
            json(),
            RequestKind::ModelInference,
            RequestDispatchDomain::RoutedPolicy,
            IngressProtocol::AnthropicMessages,
            None,
        ),
        route(
            2,
            HttpMethod::Get,
            b"/health",
            BodyPolicy::Forbidden,
            RequestKind::Liveness,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaLocal,
            Some(LocalOperationAuthScope::PublicLiveness),
        ),
        route(
            3,
            HttpMethod::Get,
            b"/status",
            BodyPolicy::Forbidden,
            RequestKind::LocalAdmin,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaManagement,
            Some(LocalOperationAuthScope::LocalAdmin),
        ),
        route(
            4,
            HttpMethod::Get,
            b"/oauth/callback",
            BodyPolicy::Forbidden,
            RequestKind::AuthFlow,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaLocal,
            Some(LocalOperationAuthScope::AuthFlow),
        ),
        route(
            5,
            HttpMethod::Get,
            b"/v1/models",
            BodyPolicy::Forbidden,
            RequestKind::UnifiedModelCatalog,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaLocal,
            Some(LocalOperationAuthScope::LocalData),
        ),
        route(
            6,
            HttpMethod::Post,
            b"/management/models/probe",
            BodyPolicy::Forbidden,
            RequestKind::DeploymentModelProbe,
            RequestDispatchDomain::BoundDeployment,
            IngressProtocol::BianmaManagement,
            None,
        ),
        route(
            7,
            HttpMethod::Post,
            b"/v1/messages/count_tokens",
            json(),
            RequestKind::ExactUpstreamTokenCount,
            RequestDispatchDomain::BoundDeployment,
            IngressProtocol::AnthropicMessages,
            None,
        ),
        route(
            8,
            HttpMethod::Post,
            b"/v1/context/compact",
            json(),
            RequestKind::AuxiliaryInference,
            RequestDispatchDomain::RoutedPolicy,
            IngressProtocol::BianmaLocal,
            None,
        ),
        route(
            9,
            HttpMethod::Post,
            b"/local/tokens/exact",
            json(),
            RequestKind::ExactLocalTokenCount,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaLocal,
            Some(LocalOperationAuthScope::LocalData),
        ),
        route(
            10,
            HttpMethod::Post,
            b"/local/tokens/estimate",
            json(),
            RequestKind::EstimatedLocalTokenCount,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaLocal,
            Some(LocalOperationAuthScope::LocalData),
        ),
        route(
            11,
            HttpMethod::Post,
            b"/local/context/compact",
            json(),
            RequestKind::LocalContextCompact,
            RequestDispatchDomain::Local,
            IngressProtocol::BianmaLocal,
            Some(LocalOperationAuthScope::LocalData),
        ),
    ])
    .expect("测试注册表合法")
}

#[allow(clippy::too_many_arguments)]
fn route(
    id: u64,
    method: HttpMethod,
    path: &[u8],
    body: BodyPolicy,
    kind: RequestKind,
    domain: RequestDispatchDomain,
    protocol: IngressProtocol,
    local_scope: Option<LocalOperationAuthScope>,
) -> RouteSpec {
    RouteSpec::try_new(
        OperationId::new(id),
        method,
        path,
        QueryPolicy::Forbidden,
        body,
        kind,
        domain,
        protocol,
        if method == HttpMethod::Post && path != b"/management/models/probe" {
            vec![b"x-client-version".to_vec()]
        } else {
            vec![]
        },
        local_scope,
    )
    .expect("测试 RouteSpec 合法")
}

fn header(name: &[u8], value: &[u8]) -> RawHeader {
    RawHeader::try_new(name, value).expect("测试 Header 合法")
}

fn data_request(path: &[u8], body: &[u8]) -> RawIngressRequest {
    RawIngressRequest::try_new(
        HttpMethod::Post,
        path,
        vec![
            header(b"content-type", b"application/json; charset=utf-8"),
            header(b"x-client-version", b"1"),
            header(b"authorization", b"Bearer TEST_ONLY"),
            header(b"content-length", body.len().to_string().as_bytes()),
        ],
        body.to_vec(),
    )
    .expect("测试数据请求合法")
}

fn model_request() -> RawIngressRequest {
    data_request(b"/v1/messages", br#"{"messages":["hello"]}"#)
}

fn token_request() -> RawIngressRequest {
    data_request(b"/v1/messages/count_tokens", br#"{"messages":["count"]}"#)
}

fn probe_request() -> RawIngressRequest {
    RawIngressRequest::try_new(
        HttpMethod::Post,
        b"/management/models/probe",
        vec![header(b"authorization", b"Bearer TEST_ONLY")],
        vec![],
    )
    .expect("测试探测请求合法")
}

fn request_material(path: &[u8], body: &[u8], semantic_headers: bool) -> RequestMaterial {
    let body_digest = hash_framed(b"bianma.ingress.raw-body.v1\0", &[body]);
    let mut semantic = Vec::from(b"bianma.ingress.semantic-headers.v1\0".as_slice());
    if !body.is_empty() {
        append_framed(&mut semantic, b"content-type");
        append_framed(&mut semantic, b"application/json; charset=utf-8");
    }
    if semantic_headers {
        append_framed(&mut semantic, b"x-client-version");
        append_framed(&mut semantic, b"1");
    }
    let semantic_digest = sha256(&semantic);
    let mut request = Vec::from(b"bianma.ingress.raw-request.v1\0".as_slice());
    request.push(2);
    append_framed(&mut request, path);
    request.extend_from_slice(&semantic_digest);
    request.extend_from_slice(&body_digest);
    request.extend_from_slice(&(body.len() as u64).to_be_bytes());
    RequestMaterial {
        body_digest,
        semantic_digest,
        request_digest: sha256(&request),
    }
}

struct RequestMaterial {
    body_digest: [u8; 32],
    semantic_digest: [u8; 32],
    request_digest: [u8; 32],
}

fn managed_verified(
    harness: &RuntimeHarness,
    path: &[u8],
    body: &[u8],
    operation: u64,
    kind: RequestKind,
    nonce: u8,
) -> (ingress_contract::VerifiedIngressRequest, ManagedFacts) {
    let material = request_material(path, body, true);
    let bound = kind == RequestKind::ExactUpstreamTokenCount;
    let bundle = managed_bundle(operation, kind, &material, nonce, bound);
    let bundle_digest =
        hash_length_bound(b"bianma.ingress.authorization-bundle-digest.v1\0", &bundle);
    let claims = managed_claims(
        harness.registry_digest,
        path,
        body,
        operation,
        kind,
        &material,
        bundle_digest,
        nonce,
    );
    let tag = hmac_sha256(
        &MANAGED_KEY,
        &mac_input(
            b"bianma.ingress.context-attestation-mac.v1\0",
            &claims,
            Some(&bundle_digest),
        ),
    );
    let mut outer = Encoder::new(b"BIANMA/ATTESTATION/1\0");
    outer.field(1, &claims);
    outer.field(2, &tag);
    let raw = match operation {
        1 => model_request(),
        7 => token_request(),
        8 => data_request(path, body),
        _ => panic!("测试 Managed Operation 未注册"),
    };
    let signed =
        SignedIngressRequest::try_new(raw, outer.finish(), bundle).expect("Managed wire 有界");
    let connection = harness
        .listener_authority
        .managed(LISTENER, TOKEN_SCOPE, AUDIENCE, ISSUER);
    let active = harness.activation_authority.issue_binding(
        LISTENER,
        TOKEN_SCOPE,
        AUDIENCE,
        ISSUER,
        POLICY,
        CONSENT,
        activation_binding(),
        EXPIRES + 60_000,
    );
    let verified = harness
        .verifier
        .verify_managed(signed, &connection, &active)
        .expect("Managed fixture 应通过 ingress verifier");
    (
        verified,
        ManagedFacts {
            request_digest: RequestDigest::from_bytes(material.request_digest),
            bundle_digest: ingress_contract::AuthorizationBundleDigest::from_bytes(bundle_digest),
        },
    )
}

struct ManagedFacts {
    request_digest: RequestDigest,
    bundle_digest: ingress_contract::AuthorizationBundleDigest,
}

fn managed_snapshot(operation: u64, facts: &ManagedFacts) -> ClassifierManagedContext {
    ClassifierManagedContext::new(
        ClassifierRequestBinding::new(OperationId::new(operation), facts.request_digest),
        ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AUDIENCE),
        ISSUER,
        facts.bundle_digest,
        POLICY,
        CONSENT,
        ClassifierManagedActivation::new(
            ClientFamilyId::new(10),
            ClientVersion::new(20),
            AdapterVersion::new(30),
            IngressSchemaVersion::new(1),
            ContextPolicyVersion::new(40),
            TransformOwnerId::new(50),
            TransformOwnerVersion::new(60),
        ),
    )
}

fn activation_binding() -> ContextActivationBinding {
    ContextActivationBinding::new(
        ClientFamilyId::new(10),
        ClientVersion::new(20),
        AdapterVersion::new(30),
        IngressSchemaVersion::new(1),
        ContextPolicyVersion::new(40),
        TransformOwnerId::new(50),
        TransformOwnerVersion::new(60),
    )
}

fn bound_target() -> ClassifierBoundTarget {
    ClassifierBoundTarget::new(
        SiteId::new(100),
        ModelDeploymentId::new(200),
        EndpointId::new(411),
        CanonicalOrigin::try_new(b"https://api.example.test").expect("Origin 合法"),
        AccountSelectorId::new(412),
        AccountId::new(413),
        CredentialId::new(414),
        AdapterContractRevision::new(3),
        1,
    )
}

#[test]
fn local_positive_and_scope_cross_binding_are_closed() {
    let listener = ListenerId::new(10);
    let token = IngressTokenScopeId::new(11);
    let cases: &[LocalCase] = &[
        (
            2,
            HttpMethod::Get,
            b"/health",
            b"",
            LocalOperationAuthScope::PublicLiveness,
        ),
        (
            3,
            HttpMethod::Get,
            b"/status",
            b"",
            LocalOperationAuthScope::LocalAdmin,
        ),
        (
            4,
            HttpMethod::Get,
            b"/oauth/callback",
            b"",
            LocalOperationAuthScope::AuthFlow,
        ),
        (
            5,
            HttpMethod::Get,
            b"/v1/models",
            b"",
            LocalOperationAuthScope::LocalData,
        ),
        (
            9,
            HttpMethod::Post,
            b"/local/tokens/exact",
            br#"{"text":"hello"}"#,
            LocalOperationAuthScope::LocalData,
        ),
        (
            10,
            HttpMethod::Post,
            b"/local/tokens/estimate",
            br#"{"text":"hello"}"#,
            LocalOperationAuthScope::LocalData,
        ),
        (
            11,
            HttpMethod::Post,
            b"/local/context/compact",
            br#"{"messages":["hello"]}"#,
            LocalOperationAuthScope::LocalData,
        ),
    ];

    for &(operation, method, path, body, scope) in cases {
        let harness = build_harness();
        let connection = harness.listener_authority.local(listener, token, scope);
        let request = if method == HttpMethod::Get {
            RawIngressRequest::try_new(method, path, vec![], vec![]).expect("GET Local 请求合法")
        } else {
            data_request(path, body)
        };
        let verified = harness
            .verifier
            .verify_local_operation(request, &connection)
            .expect("Local ingress 合法");
        let digest = if method == HttpMethod::Get {
            local_request_digest(path)
        } else {
            RequestDigest::from_bytes(request_material(path, body, true).request_digest)
        };
        let snapshot = harness
            .snapshots
            .issue_local(
                ClassifierRequestBinding::new(OperationId::new(operation), digest),
                listener,
                token,
                scope,
            )
            .expect("Local snapshot 合法");
        let disposition = harness
            .classifier
            .classify(verified, &snapshot)
            .expect("Local 应分类成功");
        let VerifiedIngressDisposition::Local(local) = disposition else {
            panic!("必须产生 Local typestate")
        };
        assert_eq!(local.operation(), OperationId::new(operation));
        assert_eq!(local.auth_scope(), scope);
        assert_eq!(local.dispatch_domain(), RequestDispatchDomain::Local);
    }

    let harness = build_harness();
    let connection =
        harness
            .listener_authority
            .local(listener, token, LocalOperationAuthScope::PublicLiveness);
    let verified = harness
        .verifier
        .verify_local_operation(
            RawIngressRequest::try_new(HttpMethod::Get, b"/health", vec![], vec![])
                .expect("请求合法"),
            &connection,
        )
        .expect("Local ingress 合法");
    let wrong = harness
        .snapshots
        .issue_local(
            ClassifierRequestBinding::new(OperationId::new(2), local_request_digest(b"/health")),
            listener,
            token,
            LocalOperationAuthScope::LocalAdmin,
        )
        .expect("快照结构合法但 scope 错误");
    assert!(matches!(
        harness.classifier.classify(verified, &wrong),
        Err(RouteReject::LocalScopeMismatch)
    ));
}

#[test]
fn managed_and_gateway_routed_positive_paths_are_typed() {
    let harness = build_harness();
    let (verified, facts) = managed_verified(
        &harness,
        b"/v1/messages",
        br#"{"messages":["hello"]}"#,
        1,
        RequestKind::ModelInference,
        1,
    );
    let snapshot = harness
        .snapshots
        .issue_managed_routed(managed_snapshot(1, &facts))
        .expect("Managed snapshot 合法");
    let disposition = harness
        .classifier
        .classify(verified, &snapshot)
        .expect("Managed 路由应通过");
    let VerifiedIngressDisposition::Routed(route) = disposition else {
        panic!("必须产生 Routed typestate")
    };
    assert_eq!(route.context_mode(), ContextExecutionMode::Managed);
    assert_eq!(route.route_policy_revision(), POLICY);
    assert_eq!(route.consent_revision(), CONSENT);
    assert!(route.managed_egress_permit().is_some());

    let harness = build_harness();
    let gateway = gateway_verified(&harness, model_request());
    let digest = RequestDigest::from_bytes(
        request_material(b"/v1/messages", br#"{"messages":["hello"]}"#, true).request_digest,
    );
    let snapshot = harness
        .snapshots
        .issue_gateway_routed(gateway_context(1, digest, 2))
        .expect("Gateway snapshot 合法");
    let disposition = harness
        .classifier
        .classify(gateway, &snapshot)
        .expect("Gateway 路由应通过");
    let VerifiedIngressDisposition::Routed(route) = disposition else {
        panic!("必须产生 Routed typestate")
    };
    assert_eq!(route.context_mode(), ContextExecutionMode::GatewayOnly);
    assert_eq!(route.maximum_trust_tier(), Some(2));

    let harness = build_harness();
    let auxiliary_body = br#"{"messages":["aux"]}"#;
    let (managed, facts) = managed_verified(
        &harness,
        b"/v1/context/compact",
        auxiliary_body,
        8,
        RequestKind::AuxiliaryInference,
        2,
    );
    let snapshot = harness
        .snapshots
        .issue_managed_routed(managed_snapshot(8, &facts))
        .expect("Managed Auxiliary snapshot 合法");
    let disposition = harness
        .classifier
        .classify(managed, &snapshot)
        .expect("Managed Auxiliary 应通过");
    let VerifiedIngressDisposition::Routed(route) = disposition else {
        panic!("Auxiliary 必须产生 Routed typestate")
    };
    assert_eq!(route.request_kind(), RequestKind::AuxiliaryInference);

    let harness = build_harness();
    let gateway = gateway_verified(
        &harness,
        data_request(b"/v1/context/compact", auxiliary_body),
    );
    let digest = RequestDigest::from_bytes(
        request_material(b"/v1/context/compact", auxiliary_body, true).request_digest,
    );
    let snapshot = harness
        .snapshots
        .issue_gateway_routed(gateway_context(8, digest, 2))
        .expect("Gateway Auxiliary snapshot 合法");
    let disposition = harness
        .classifier
        .classify(gateway, &snapshot)
        .expect("Gateway Auxiliary 应通过");
    let VerifiedIngressDisposition::Routed(route) = disposition else {
        panic!("Auxiliary 必须产生 Routed typestate")
    };
    assert_eq!(route.request_kind(), RequestKind::AuxiliaryInference);
}

#[test]
fn capability_bound_is_exact_single_attempt_and_rejects_account_drift() {
    let harness = build_harness();
    let (verified, digest) = capability_verified(&harness, 2);
    let snapshot = harness
        .snapshots
        .issue_capability_bound(capability_snapshot(digest, bound_target()))
        .expect("Capability snapshot 合法");
    let disposition = harness
        .classifier
        .classify(verified, &snapshot)
        .expect("Capability 应通过");
    let VerifiedIngressDisposition::BoundDeployment(bound) = disposition else {
        panic!("必须产生 Bound typestate")
    };
    assert_eq!(bound.account(), AccountId::new(413));
    assert_eq!(bound.credential(), CredentialId::new(414));
    assert_eq!(bound.max_attempts(), 1);
    assert_eq!(bound.fallback_policy(), BoundFallbackPolicy::Forbidden);
    assert_eq!(bound.management_scope(), Some(CAP_SCOPE));
    assert_eq!(bound.context_mode(), None);

    let harness = build_harness();
    let (verified, digest) = capability_verified(&harness, 3);
    let wrong_target = ClassifierBoundTarget::new(
        SiteId::new(100),
        ModelDeploymentId::new(200),
        EndpointId::new(411),
        CanonicalOrigin::try_new(b"https://api.example.test").expect("Origin 合法"),
        AccountSelectorId::new(412),
        AccountId::new(999),
        CredentialId::new(414),
        AdapterContractRevision::new(3),
        1,
    );
    let snapshot = harness
        .snapshots
        .issue_capability_bound(capability_snapshot(digest, wrong_target))
        .expect("结构合法");
    assert!(matches!(
        harness.classifier.classify(verified, &snapshot),
        Err(RouteReject::BoundGateRejected)
    ));
}

#[test]
fn managed_and_gateway_exact_require_context_plus_bound_gate() {
    let harness = build_harness();
    let (verified, facts) = managed_verified(
        &harness,
        b"/v1/messages/count_tokens",
        br#"{"messages":["count"]}"#,
        7,
        RequestKind::ExactUpstreamTokenCount,
        4,
    );
    let context = managed_snapshot(7, &facts);
    let binding = ClassifierRequestBinding::new(OperationId::new(7), facts.request_digest);
    let snapshot = harness
        .snapshots
        .issue_managed_exact_upstream(context, binding, bound_target())
        .expect("Managed Exact snapshot 合法");
    let disposition = harness
        .classifier
        .classify(verified, &snapshot)
        .expect("Managed Exact 应通过双门禁");
    let VerifiedIngressDisposition::BoundDeployment(bound) = disposition else {
        panic!("必须产生 Bound typestate")
    };
    assert_eq!(bound.context_mode(), Some(ContextExecutionMode::Managed));
    assert_eq!(bound.max_attempts(), 1);
    assert_eq!(bound.fallback_policy(), BoundFallbackPolicy::Forbidden);
    assert_eq!(bound.site(), SiteId::new(100));
    assert_eq!(bound.deployment(), ModelDeploymentId::new(200));
    assert_eq!(bound.endpoint(), EndpointId::new(411));
    assert_eq!(bound.origin().as_bytes(), b"https://api.example.test");
    assert_eq!(bound.account_selector(), AccountSelectorId::new(412));
    assert_eq!(bound.account(), AccountId::new(413));
    assert_eq!(bound.credential(), CredentialId::new(414));
    assert_eq!(
        bound.adapter_contract_revision(),
        AdapterContractRevision::new(3)
    );
    assert_eq!(bound.trust_tier(), 1);

    let harness = build_harness();
    let gateway = gateway_verified(&harness, token_request());
    let digest = RequestDigest::from_bytes(
        request_material(
            b"/v1/messages/count_tokens",
            br#"{"messages":["count"]}"#,
            true,
        )
        .request_digest,
    );
    let context = gateway_context(7, digest, 2);
    let binding = ClassifierRequestBinding::new(OperationId::new(7), digest);
    let snapshot = harness
        .snapshots
        .issue_gateway_exact_upstream(context, binding, bound_target())
        .expect("Gateway Exact snapshot 合法");
    let disposition = harness
        .classifier
        .classify(gateway, &snapshot)
        .expect("Gateway Exact 应通过双门禁");
    let VerifiedIngressDisposition::BoundDeployment(bound) = disposition else {
        panic!("必须产生 Bound typestate")
    };
    assert_eq!(
        bound.context_mode(),
        Some(ContextExecutionMode::GatewayOnly)
    );
    assert_eq!(bound.management_scope(), None);

    let harness = build_harness();
    let gateway = gateway_verified(&harness, token_request());
    let wrong_mode = harness
        .snapshots
        .issue_gateway_routed(gateway_context(7, digest, 2))
        .expect("结构合法");
    assert!(matches!(
        harness.classifier.classify(gateway, &wrong_mode),
        Err(RouteReject::DispositionNotAllowed)
    ));
}

#[test]
fn exact_upstream_permit_target_and_gateway_trust_are_rechecked() {
    for case in 0u8..4 {
        let harness = build_harness();
        let (verified, facts) = managed_verified(
            &harness,
            b"/v1/messages/count_tokens",
            br#"{"messages":["count"]}"#,
            7,
            RequestKind::ExactUpstreamTokenCount,
            100 + case,
        );
        let context = managed_snapshot(7, &facts);
        let target = ClassifierBoundTarget::new(
            if case == 0 {
                SiteId::new(999)
            } else {
                SiteId::new(100)
            },
            if case == 1 {
                ModelDeploymentId::new(999)
            } else {
                ModelDeploymentId::new(200)
            },
            EndpointId::new(411),
            CanonicalOrigin::try_new(if case == 2 {
                b"https://other.example.test"
            } else {
                b"https://api.example.test"
            })
            .expect("Origin 合法"),
            AccountSelectorId::new(412),
            AccountId::new(413),
            CredentialId::new(414),
            AdapterContractRevision::new(3),
            if case == 3 { 2 } else { 1 },
        );
        let snapshot = harness
            .snapshots
            .issue_managed_exact_upstream(
                context,
                ClassifierRequestBinding::new(OperationId::new(7), facts.request_digest),
                target,
            )
            .expect("错配目标仍是结构合法快照");
        assert!(
            matches!(
                harness.classifier.classify(verified, &snapshot),
                Err(RouteReject::BoundGateRejected)
            ),
            "Managed Exact 目标错配 case={case} 未被拒绝"
        );
    }

    let harness = build_harness();
    let gateway = gateway_verified_with_trust(&harness, token_request(), 1);
    let digest = RequestDigest::from_bytes(
        request_material(
            b"/v1/messages/count_tokens",
            br#"{"messages":["count"]}"#,
            true,
        )
        .request_digest,
    );
    let snapshot = harness
        .snapshots
        .issue_gateway_exact_upstream(
            gateway_context(7, digest, 1),
            ClassifierRequestBinding::new(OperationId::new(7), digest),
            ClassifierBoundTarget::new(
                SiteId::new(100),
                ModelDeploymentId::new(200),
                EndpointId::new(411),
                CanonicalOrigin::try_new(b"https://api.example.test").expect("Origin 合法"),
                AccountSelectorId::new(412),
                AccountId::new(413),
                CredentialId::new(414),
                AdapterContractRevision::new(3),
                2,
            ),
        )
        .expect("结构合法");
    assert!(matches!(
        harness.classifier.classify(gateway, &snapshot),
        Err(RouteReject::BoundGateRejected)
    ));
}

#[test]
fn foreign_snapshot_and_foreign_receiver_stop_before_normalizer() {
    let harness_a = build_harness();
    let harness_b = build_harness();
    let listener = ListenerId::new(10);
    let token = IngressTokenScopeId::new(11);
    let connection = harness_a.listener_authority.local(
        listener,
        token,
        LocalOperationAuthScope::PublicLiveness,
    );
    let verified = harness_a
        .verifier
        .verify_local_operation(
            RawIngressRequest::try_new(HttpMethod::Get, b"/health", vec![], vec![])
                .expect("请求合法"),
            &connection,
        )
        .expect("Local ingress 合法");
    let snapshot = harness_b
        .snapshots
        .issue_local(
            ClassifierRequestBinding::new(OperationId::new(2), local_request_digest(b"/health")),
            listener,
            token,
            LocalOperationAuthScope::PublicLiveness,
        )
        .expect("快照合法");
    assert!(matches!(
        harness_a.classifier.classify(verified, &snapshot),
        Err(RouteReject::SnapshotDomainMismatch)
    ));
    assert_eq!(harness_a.calls.load(Ordering::SeqCst), 0);

    let harness_a = build_harness();
    let harness_b = build_harness();
    let connection = harness_a.listener_authority.local(
        listener,
        token,
        LocalOperationAuthScope::PublicLiveness,
    );
    let verified = harness_a
        .verifier
        .verify_local_operation(
            RawIngressRequest::try_new(HttpMethod::Get, b"/health", vec![], vec![])
                .expect("请求合法"),
            &connection,
        )
        .expect("Local ingress 合法");
    let snapshot = harness_b
        .snapshots
        .issue_local(
            ClassifierRequestBinding::new(OperationId::new(2), local_request_digest(b"/health")),
            listener,
            token,
            LocalOperationAuthScope::PublicLiveness,
        )
        .expect("快照合法");
    assert!(matches!(
        harness_b.classifier.classify(verified, &snapshot),
        Err(RouteReject::Ingress(_))
    ));
    assert_eq!(harness_b.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn request_binding_and_snapshot_fields_fail_closed() {
    let harness = build_harness();
    let gateway = gateway_verified(&harness, model_request());
    let digest = RequestDigest::from_bytes(
        request_material(b"/v1/messages", br#"{"messages":["hello"]}"#, true).request_digest,
    );
    let wrong_digest = RequestDigest::from_bytes([0x99; 32]);
    let snapshot = harness
        .snapshots
        .issue_gateway_routed(gateway_context(1, wrong_digest, 2))
        .expect("结构合法");
    assert!(matches!(
        harness.classifier.classify(gateway, &snapshot),
        Err(RouteReject::SnapshotBindingMismatch)
    ));
    assert_eq!(harness.calls.load(Ordering::SeqCst), 0);

    let harness = build_harness();
    let gateway = gateway_verified(&harness, model_request());
    let snapshot = harness
        .snapshots
        .issue_gateway_routed(gateway_context(1, digest, 1))
        .expect("结构合法");
    assert!(matches!(
        harness.classifier.classify(gateway, &snapshot),
        Err(RouteReject::ContextGateRejected)
    ));
    assert_eq!(harness.calls.load(Ordering::SeqCst), 1);

    let harness = build_harness();
    let (managed, facts) = managed_verified(
        &harness,
        b"/v1/messages",
        br#"{"messages":["hello"]}"#,
        1,
        RequestKind::ModelInference,
        5,
    );
    let wrong = ClassifierManagedContext::new(
        ClassifierRequestBinding::new(OperationId::new(1), facts.request_digest),
        ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AUDIENCE),
        ISSUER,
        facts.bundle_digest,
        RoutePolicyRevision::new(999),
        CONSENT,
        ClassifierManagedActivation::new(
            ClientFamilyId::new(10),
            ClientVersion::new(20),
            AdapterVersion::new(30),
            IngressSchemaVersion::new(1),
            ContextPolicyVersion::new(40),
            TransformOwnerId::new(50),
            TransformOwnerVersion::new(60),
        ),
    );
    let snapshot = harness
        .snapshots
        .issue_managed_routed(wrong)
        .expect("结构合法");
    assert!(matches!(
        harness.classifier.classify(managed, &snapshot),
        Err(RouteReject::ContextGateRejected)
    ));
}

#[test]
fn managed_context_and_activation_fields_are_checked_individually() {
    for case in 0u8..14 {
        let harness = build_harness();
        let (managed, facts) = managed_verified(
            &harness,
            b"/v1/messages",
            br#"{"messages":["hello"]}"#,
            1,
            RequestKind::ModelInference,
            60 + case,
        );
        let ingress = match case {
            0 => ClassifierIngressBinding::new(ListenerId::new(999), TOKEN_SCOPE, AUDIENCE),
            1 => ClassifierIngressBinding::new(LISTENER, IngressTokenScopeId::new(999), AUDIENCE),
            2 => ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AudienceId::new(999)),
            _ => ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AUDIENCE),
        };
        let activation = ClassifierManagedActivation::new(
            if case == 7 {
                ClientFamilyId::new(999)
            } else {
                ClientFamilyId::new(10)
            },
            if case == 8 {
                ClientVersion::new(999)
            } else {
                ClientVersion::new(20)
            },
            if case == 9 {
                AdapterVersion::new(999)
            } else {
                AdapterVersion::new(30)
            },
            if case == 10 {
                IngressSchemaVersion::new(999)
            } else {
                IngressSchemaVersion::new(1)
            },
            if case == 11 {
                ContextPolicyVersion::new(999)
            } else {
                ContextPolicyVersion::new(40)
            },
            if case == 12 {
                TransformOwnerId::new(999)
            } else {
                TransformOwnerId::new(50)
            },
            if case == 13 {
                TransformOwnerVersion::new(999)
            } else {
                TransformOwnerVersion::new(60)
            },
        );
        let context = ClassifierManagedContext::new(
            ClassifierRequestBinding::new(OperationId::new(1), facts.request_digest),
            ingress,
            if case == 3 {
                IssuerEpoch::new(999)
            } else {
                ISSUER
            },
            if case == 4 {
                ingress_contract::AuthorizationBundleDigest::from_bytes([0x99; 32])
            } else {
                facts.bundle_digest
            },
            if case == 5 {
                RoutePolicyRevision::new(999)
            } else {
                POLICY
            },
            if case == 6 {
                ConsentRevision::new(999)
            } else {
                CONSENT
            },
            activation,
        );
        let snapshot = harness
            .snapshots
            .issue_managed_routed(context)
            .expect("错配事实仍是结构合法快照");
        assert!(
            matches!(
                harness.classifier.classify(managed, &snapshot),
                Err(RouteReject::ContextGateRejected)
            ),
            "Managed 错配 case={case} 未被 Context gate 拒绝"
        );
    }
}

#[test]
fn registry_drift_is_rejected_before_normalizer() {
    let registry = build_registry();
    let correct_digest = registry.digest();
    let runtime = IngressVerifier::initialize(
        registry,
        ManagedVerificationKeyRing::try_new(vec![ManagedVerificationKey::try_new(
            ISSUER,
            MANAGED_KEY,
        )
        .expect("Managed key 合法")])
        .expect("Managed key ring 合法"),
        CapabilityVerificationKeyRing::try_new(vec![CapabilityVerificationKey::try_new(
            ISSUER,
            CAPABILITY_KEY,
        )
        .expect("Capability key 合法")])
        .expect("Capability key ring 合法"),
        Arc::new(FixedClock::new(NOW)),
        Arc::new(MemoryNonceStore::new(16).expect("nonce store 合法")),
    );
    let (verifier, listener_authority, _, _, receiver) = runtime.into_parts();
    let calls = Arc::new(AtomicUsize::new(0));
    let drifted_digest = RegistryDigest::from_bytes([0x7a; 32]);
    assert!(drifted_digest != correct_digest);
    let classifier_runtime = ClassifierRuntime::initialize(
        receiver,
        drifted_digest,
        CountingNormalizer {
            calls: Arc::clone(&calls),
            behavior: NormalizerBehavior::Expected,
        },
        FixedClassifierClock,
    )
    .expect("非零漂移摘要可初始化，但请求时必须拒绝");
    let (classifier, snapshots) = classifier_runtime.into_parts();
    let listener = ListenerId::new(10);
    let token = IngressTokenScopeId::new(11);
    let connection =
        listener_authority.local(listener, token, LocalOperationAuthScope::PublicLiveness);
    let verified = verifier
        .verify_local_operation(
            RawIngressRequest::try_new(HttpMethod::Get, b"/health", vec![], vec![])
                .expect("请求合法"),
            &connection,
        )
        .expect("Ingress 仍绑定原注册表");
    let snapshot = snapshots
        .issue_local(
            ClassifierRequestBinding::new(OperationId::new(2), local_request_digest(b"/health")),
            listener,
            token,
            LocalOperationAuthScope::PublicLiveness,
        )
        .expect("漂移 runtime 自身可签发快照");
    assert!(matches!(
        classifier.classify(verified, &snapshot),
        Err(RouteReject::RegistryMismatch)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn normalizer_failure_and_wrong_semantic_fail_closed() {
    let listener = ListenerId::new(10);
    let token = IngressTokenScopeId::new(11);

    let harness = build_harness_with_behavior(NormalizerBehavior::Reject(
        ProtocolNormalizeError::MalformedRequest,
    ));
    let connection =
        harness
            .listener_authority
            .local(listener, token, LocalOperationAuthScope::PublicLiveness);
    let verified = harness
        .verifier
        .verify_local_operation(
            RawIngressRequest::try_new(HttpMethod::Get, b"/health", vec![], vec![])
                .expect("请求合法"),
            &connection,
        )
        .expect("Local ingress 合法");
    let snapshot = harness
        .snapshots
        .issue_local(
            ClassifierRequestBinding::new(OperationId::new(2), local_request_digest(b"/health")),
            listener,
            token,
            LocalOperationAuthScope::PublicLiveness,
        )
        .expect("Local snapshot 合法");
    assert!(matches!(
        harness.classifier.classify(verified, &snapshot),
        Err(RouteReject::Normalize(
            ProtocolNormalizeError::MalformedRequest
        ))
    ));
    assert_eq!(harness.calls.load(Ordering::SeqCst), 1);

    let harness = build_harness_with_behavior(NormalizerBehavior::Force(
        NormalizedRequestSemantic::ModelInference,
    ));
    let connection =
        harness
            .listener_authority
            .local(listener, token, LocalOperationAuthScope::PublicLiveness);
    let verified = harness
        .verifier
        .verify_local_operation(
            RawIngressRequest::try_new(HttpMethod::Get, b"/health", vec![], vec![])
                .expect("请求合法"),
            &connection,
        )
        .expect("Local ingress 合法");
    let snapshot = harness
        .snapshots
        .issue_local(
            ClassifierRequestBinding::new(OperationId::new(2), local_request_digest(b"/health")),
            listener,
            token,
            LocalOperationAuthScope::PublicLiveness,
        )
        .expect("Local snapshot 合法");
    assert!(matches!(
        harness.classifier.classify(verified, &snapshot),
        Err(RouteReject::NormalizedBindingMismatch)
    ));
    assert_eq!(harness.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn gateway_context_fields_are_checked_individually() {
    for case in 0u8..6 {
        let harness = build_harness();
        let gateway = gateway_verified(&harness, model_request());
        let digest = RequestDigest::from_bytes(
            request_material(b"/v1/messages", br#"{"messages":["hello"]}"#, true).request_digest,
        );
        let ingress = match case {
            0 => ClassifierIngressBinding::new(ListenerId::new(999), TOKEN_SCOPE, AUDIENCE),
            1 => ClassifierIngressBinding::new(LISTENER, IngressTokenScopeId::new(999), AUDIENCE),
            2 => ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AudienceId::new(999)),
            _ => ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AUDIENCE),
        };
        let context = ClassifierGatewayContext::new(
            ClassifierRequestBinding::new(OperationId::new(1), digest),
            ingress,
            if case == 3 {
                ConsentRevision::new(999)
            } else {
                CONSENT
            },
            if case == 4 {
                RoutePolicyRevision::new(999)
            } else {
                POLICY
            },
            if case == 5 { 1 } else { 2 },
        );
        let snapshot = harness
            .snapshots
            .issue_gateway_routed(context)
            .expect("错配事实仍是结构合法快照");
        assert!(matches!(
            harness.classifier.classify(gateway, &snapshot),
            Err(RouteReject::ContextGateRejected)
        ));
    }
}

#[test]
fn capability_snapshot_fields_are_checked_individually() {
    for case in 0u8..14 {
        let harness = build_harness();
        let (verified, digest) = capability_verified(&harness, 20 + case);
        let target = ClassifierBoundTarget::new(
            if case == 0 {
                SiteId::new(999)
            } else {
                SiteId::new(100)
            },
            if case == 1 {
                ModelDeploymentId::new(999)
            } else {
                ModelDeploymentId::new(200)
            },
            if case == 2 {
                EndpointId::new(999)
            } else {
                EndpointId::new(411)
            },
            CanonicalOrigin::try_new(if case == 3 {
                b"https://other.example.test"
            } else {
                b"https://api.example.test"
            })
            .expect("Origin 合法"),
            if case == 4 {
                AccountSelectorId::new(999)
            } else {
                AccountSelectorId::new(412)
            },
            if case == 5 {
                AccountId::new(999)
            } else {
                AccountId::new(413)
            },
            if case == 6 {
                CredentialId::new(999)
            } else {
                CredentialId::new(414)
            },
            if case == 7 {
                AdapterContractRevision::new(999)
            } else {
                AdapterContractRevision::new(3)
            },
            if case == 13 { 2 } else { 1 },
        );
        let capability = ClassifierCapabilityBound::new(
            ClassifierRequestBinding::new(OperationId::new(6), digest),
            match case {
                8 => ClassifierIngressBinding::new(ListenerId::new(999), CAP_TOKEN, CAP_AUDIENCE),
                9 => ClassifierIngressBinding::new(
                    CAP_LISTENER,
                    IngressTokenScopeId::new(999),
                    CAP_AUDIENCE,
                ),
                _ => ClassifierIngressBinding::new(CAP_LISTENER, CAP_TOKEN, CAP_AUDIENCE),
            },
            if case == 10 {
                IssuerEpoch::new(999)
            } else {
                ISSUER
            },
            if case == 11 {
                CapabilityManagementScopeId::new(999)
            } else {
                CAP_SCOPE
            },
            if case == 12 { EXPIRES + 1 } else { EXPIRES },
            target,
        );
        let snapshot = harness
            .snapshots
            .issue_capability_bound(capability)
            .expect("错配事实仍是结构合法快照");
        let result = harness.classifier.classify(verified, &snapshot);
        assert!(
            matches!(result, Err(RouteReject::BoundGateRejected)),
            "Capability 错配 case={case} 未被 Bound gate 拒绝"
        );
    }
}

#[test]
fn proof_kind_domain_and_snapshot_mode_matrix_is_fail_closed() {
    let harness = build_harness();
    let gateway = gateway_verified(&harness, model_request());
    let digest = RequestDigest::from_bytes(
        request_material(b"/v1/messages", br#"{"messages":["hello"]}"#, true).request_digest,
    );
    let capability_mode = harness
        .snapshots
        .issue_capability_bound(ClassifierCapabilityBound::new(
            ClassifierRequestBinding::new(OperationId::new(1), digest),
            ClassifierIngressBinding::new(CAP_LISTENER, CAP_TOKEN, CAP_AUDIENCE),
            ISSUER,
            CAP_SCOPE,
            EXPIRES,
            bound_target(),
        ))
        .expect("快照结构合法");
    assert!(matches!(
        harness.classifier.classify(gateway, &capability_mode),
        Err(RouteReject::DispositionNotAllowed)
    ));

    let harness = build_harness();
    let (managed, facts) = managed_verified(
        &harness,
        b"/v1/messages",
        br#"{"messages":["hello"]}"#,
        1,
        RequestKind::ModelInference,
        40,
    );
    let gateway_mode = harness
        .snapshots
        .issue_gateway_routed(gateway_context(1, facts.request_digest, 2))
        .expect("快照结构合法");
    assert!(matches!(
        harness.classifier.classify(managed, &gateway_mode),
        Err(RouteReject::ContextGateRejected)
    ));
}

fn gateway_verified(
    harness: &RuntimeHarness,
    request: RawIngressRequest,
) -> ingress_contract::VerifiedIngressRequest {
    gateway_verified_with_trust(harness, request, 2)
}

fn gateway_verified_with_trust(
    harness: &RuntimeHarness,
    request: RawIngressRequest,
    maximum_trust_tier: u8,
) -> ingress_contract::VerifiedIngressRequest {
    let connection = harness
        .listener_authority
        .gateway_only(LISTENER, TOKEN_SCOPE, AUDIENCE);
    let consent = harness
        .consent_authority
        .issue_snapshot(
            LISTENER,
            TOKEN_SCOPE,
            AUDIENCE,
            POLICY,
            CONSENT,
            maximum_trust_tier,
            NOW - 1_000,
            EXPIRES,
        )
        .expect("Gateway consent 合法");
    harness
        .verifier
        .verify_gateway_only(request, &connection, &consent)
        .expect("Gateway ingress 合法")
}

fn gateway_context(
    operation: u64,
    digest: RequestDigest,
    maximum_trust_tier: u8,
) -> ClassifierGatewayContext {
    ClassifierGatewayContext::new(
        ClassifierRequestBinding::new(OperationId::new(operation), digest),
        ClassifierIngressBinding::new(LISTENER, TOKEN_SCOPE, AUDIENCE),
        CONSENT,
        POLICY,
        maximum_trust_tier,
    )
}

fn capability_verified(
    harness: &RuntimeHarness,
    nonce: u8,
) -> (ingress_contract::VerifiedIngressRequest, RequestDigest) {
    let material = request_material(b"/management/models/probe", b"", false);
    let digest = RequestDigest::from_bytes(material.request_digest);
    let mut claims = Encoder::new(b"BIANMA/CAPABILITY-CLAIMS/1\0");
    claims.field_u16(1, 1);
    claims.field_u64(2, CAP_AUDIENCE.get());
    claims.field_u64(3, ISSUER.get());
    claims.field_u64(4, NOW - 1_000);
    claims.field_u64(5, EXPIRES);
    claims.field_u64(6, CAP_LISTENER.get());
    claims.field_u64(7, CAP_TOKEN.get());
    claims.field_u64(8, 6);
    claims.field_u8(9, 2);
    claims.field(10, harness.registry_digest.as_bytes());
    claims.field_u64(11, 200);
    claims.field_u64(12, 411);
    claims.field(13, b"https://api.example.test");
    claims.field_u64(14, 412);
    claims.field_u64(15, 413);
    claims.field_u64(16, 414);
    claims.field_u64(17, 3);
    claims.field_u64(18, CAP_SCOPE.get());
    claims.field(19, material.request_digest.as_slice());
    claims.field(20, &[nonce; 16]);
    claims.field_u8(21, 0);
    claims.field_u64(22, 100);
    claims.field_u8(23, 1);
    let claims = claims.finish();
    let tag = hmac_sha256(
        &CAPABILITY_KEY,
        &mac_input(
            b"bianma.ingress.capability-authorization-mac.v1\0",
            &claims,
            None,
        ),
    );
    let mut wire = Encoder::new(b"BIANMA/CAPABILITY-AUTHORIZATION/1\0");
    wire.field(1, &claims);
    wire.field(2, &tag);
    let connection = harness.listener_authority.capability(
        CAP_LISTENER,
        CAP_TOKEN,
        CAP_AUDIENCE,
        ISSUER,
        CAP_SCOPE,
    );
    let verified = harness
        .verifier
        .verify_capability_probe(
            probe_request(),
            &connection,
            EncodedCapabilityAuthorization::try_new(wire.finish()).expect("Capability wire 有界"),
        )
        .expect("Capability fixture 应通过 ingress verifier");
    (verified, digest)
}

fn capability_snapshot(
    digest: RequestDigest,
    target: ClassifierBoundTarget,
) -> ClassifierCapabilityBound {
    ClassifierCapabilityBound::new(
        ClassifierRequestBinding::new(OperationId::new(6), digest),
        ClassifierIngressBinding::new(CAP_LISTENER, CAP_TOKEN, CAP_AUDIENCE),
        ISSUER,
        CAP_SCOPE,
        EXPIRES,
        target,
    )
}

fn local_request_digest(path: &[u8]) -> RequestDigest {
    let body_digest = hash_framed(b"bianma.ingress.raw-body.v1\0", &[b""]);
    let semantic_digest = sha256(b"bianma.ingress.semantic-headers.v1\0");
    let mut bytes = Vec::from(b"bianma.ingress.raw-request.v1\0".as_slice());
    bytes.push(1);
    append_framed(&mut bytes, path);
    bytes.extend_from_slice(&semantic_digest);
    bytes.extend_from_slice(&body_digest);
    bytes.extend_from_slice(&0u64.to_be_bytes());
    RequestDigest::from_bytes(sha256(&bytes))
}

fn managed_bundle(
    operation: u64,
    kind: RequestKind,
    material: &RequestMaterial,
    nonce: u8,
    bound: bool,
) -> Vec<u8> {
    let mut target = Encoder::new(b"BIANMA/EGRESS-TARGET/1\0");
    target.field_u64(1, 100);
    target.field_u64(2, 200);
    target.field(3, b"https://api.example.test");
    target.field_u8(4, 1);
    let target = target.finish();
    let mut targets = Vec::new();
    targets.extend_from_slice(&1u16.to_be_bytes());
    targets.extend_from_slice(&(target.len() as u32).to_be_bytes());
    targets.extend_from_slice(&target);

    let mut permit = Encoder::new(b"BIANMA/EGRESS-PERMIT/1\0");
    permit.field_u64(1, operation);
    permit.field(2, &material.request_digest);
    permit.field(3, &material.body_digest);
    permit.field(4, &[0x33; 32]);
    permit.field(5, &[nonce; 16]);
    permit.field_u8(
        6,
        match kind {
            RequestKind::ModelInference => 1,
            RequestKind::AuxiliaryInference => 2,
            RequestKind::ExactUpstreamTokenCount => 3,
            _ => unreachable!(),
        },
    );
    permit.field_u8(7, 3);
    permit.field_u64(8, 128 * 1024);
    permit.field(9, &targets);
    permit.field_u8(10, u8::from(!bound));
    permit.field_u64(11, POLICY.get());
    permit.field_u64(12, CONSENT.get());
    permit.field_u64(13, EXPIRES);

    let mut requirements = Encoder::new(b"BIANMA/CAPABILITY-REQUIREMENTS/1\0");
    for (tag, value) in [(1, 1), (2, 2), (3, 30), (4, 3), (5, 4), (6, 0), (7, 5)] {
        requirements.field_u64(tag, value);
    }
    requirements.field_u8(8, 1);
    requirements.field_u8(9, 1);

    let mut activation = Encoder::new(b"BIANMA/ACTIVATION-KEY/1\0");
    for (tag, value) in [(1, 10), (2, 20), (3, 30), (4, 1), (5, 40), (6, 50), (7, 60)] {
        activation.field_u64(tag, value);
    }

    let mut bundle = Encoder::new(b"BIANMA/AUTHORIZATION-BUNDLE/1\0");
    bundle.field(1, &permit.finish());
    bundle.field(2, &requirements.finish());
    bundle.field(3, &activation.finish());
    bundle.finish()
}

#[allow(clippy::too_many_arguments)]
fn managed_claims(
    registry: RegistryDigest,
    path: &[u8],
    body: &[u8],
    operation: u64,
    kind: RequestKind,
    material: &RequestMaterial,
    bundle_digest: [u8; 32],
    nonce: u8,
) -> Vec<u8> {
    let mut claims = Encoder::new(b"BIANMA/ATTESTATION-CLAIMS/1\0");
    claims.field_u16(1, 1);
    claims.field_u64(2, AUDIENCE.get());
    claims.field_u64(3, ISSUER.get());
    claims.field_u64(4, NOW - 1_000);
    claims.field_u64(5, EXPIRES);
    claims.field_u64(6, LISTENER.get());
    claims.field_u64(7, TOKEN_SCOPE.get());
    claims.field_u64(8, operation);
    claims.field_u8(
        9,
        if kind == RequestKind::ExactUpstreamTokenCount {
            2
        } else {
            3
        },
    );
    claims.field(10, registry.as_bytes());
    claims.field_u8(11, 2);
    claims.field(12, path);
    claims.field(13, &material.semantic_digest);
    claims.field(14, &material.body_digest);
    claims.field_u64(15, body.len() as u64);
    claims.field(16, &material.request_digest);
    claims.field(17, &[0x33; 32]);
    claims.field(18, &bundle_digest);
    claims.field(19, &[nonce; 16]);
    claims.field_u64(20, POLICY.get());
    claims.field_u64(21, 30);
    claims.field_u64(22, 60);
    claims.finish()
}

struct Encoder {
    bytes: Vec<u8>,
}

impl Encoder {
    fn new(prefix: &[u8]) -> Self {
        Self {
            bytes: prefix.to_vec(),
        }
    }

    fn field(&mut self, tag: u16, value: &[u8]) {
        self.bytes.extend_from_slice(&tag.to_be_bytes());
        self.bytes
            .extend_from_slice(&(value.len() as u32).to_be_bytes());
        self.bytes.extend_from_slice(value);
    }

    fn field_u8(&mut self, tag: u16, value: u8) {
        self.field(tag, &[value]);
    }
    fn field_u16(&mut self, tag: u16, value: u16) {
        self.field(tag, &value.to_be_bytes());
    }
    fn field_u64(&mut self, tag: u16, value: u64) {
        self.field(tag, &value.to_be_bytes());
    }
    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

fn append_framed(output: &mut Vec<u8>, bytes: &[u8]) {
    output.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    output.extend_from_slice(bytes);
}

fn hash_framed(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut bytes = domain.to_vec();
    for part in parts {
        append_framed(&mut bytes, part);
    }
    sha256(&bytes)
}

fn hash_length_bound(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut input = domain.to_vec();
    input.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    input.extend_from_slice(bytes);
    sha256(&input)
}

fn mac_input(domain: &[u8], claims: &[u8], bound: Option<&[u8; 32]>) -> Vec<u8> {
    let mut input = domain.to_vec();
    input.extend_from_slice(&(claims.len() as u64).to_be_bytes());
    input.extend_from_slice(claims);
    if let Some(bound) = bound {
        input.extend_from_slice(bound);
    }
    input
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    block[..key.len()].copy_from_slice(key);
    let mut inner = [0x36u8; 64];
    let mut outer = [0x5cu8; 64];
    for index in 0..64 {
        inner[index] ^= block[index];
        outer[index] ^= block[index];
    }
    let mut inner_message = inner.to_vec();
    inner_message.extend_from_slice(message);
    let inner_hash = sha256(&inner_message);
    let mut outer_message = outer.to_vec();
    outer_message.extend_from_slice(&inner_hash);
    sha256(&outer_message)
}

fn sha256(message: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut data = message.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0);
    }
    data.extend_from_slice(&bit_len.to_be_bytes());
    let mut h = [
        0x6a09e667u32,
        0xbb67ae85,
        0x3c6ef372,
        0xa54ff53a,
        0x510e527f,
        0x9b05688c,
        0x1f83d9ab,
        0x5be0cd19,
    ];
    for chunk in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes(word.try_into().expect("四字节块"));
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(majority);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (state, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *state = state.wrapping_add(value);
        }
    }
    let mut output = [0u8; 32];
    for (chunk, value) in output.chunks_exact_mut(4).zip(h) {
        chunk.copy_from_slice(&value.to_be_bytes());
    }
    output
}
