//! 安全合同单元测试；测试签发器只在 `cfg(test)` 下存在。

use std::{
    sync::{Arc, Barrier},
    thread,
};

use crate::{
    operation::RequestBinding,
    signed::{
        authorization_bundle_digest, decode_attestation, decode_authorization_bundle,
        decode_capability_authorization, encode_authorization_bundle, sign_attestation_for_test,
        sign_capability_for_test, AllowedEgressTarget, AttestationClaims, AuthorizationBundle,
        CanonicalOrigin, CapabilityClaims, ContextActivationKey, ContextCapabilityRequirements,
        ContextEgressPermit, ContinuationConstraint, EgressPurpose, SensitivityClass,
        ATTESTATION_PREFIX, AUTHORIZATION_BUNDLE_PREFIX,
    },
    AccountId, AccountSelectorId, AdapterContractRevision, AdapterVersion, AudienceId, BodyPolicy,
    CapabilityManagementScopeId, CapabilityVerificationKey, CapabilityVerificationKeyRing,
    ClientFamilyId, ClientVersion, ConsentRevision, ContextActivationBinding, ContextPolicyVersion,
    CredentialId, EncodedCapabilityAuthorization, EndpointId, EnvelopeDigest, FixedClock,
    HandleEpoch, HttpMethod, IngressReject, IngressSchemaVersion, IngressTokenScopeId,
    IngressVerifier, IssuerEpoch, ListenerBindingAuthority, ListenerId, LocalOperationAuthScope,
    ManagedActivationBinding, ManagedVerificationKey, ManagedVerificationKeyRing, MemoryNonceStore,
    ModelDeploymentId, NonceNamespace, NonceReject, OneShotNonce, OneShotNonceStore, OperationId,
    ProtocolFrameRevision, QueryPolicy, RawHeader, RawIngressRequest, RequestDispatchDomain,
    RequestKind, RetrievalSchemaRevision, RoutePolicyRevision, RouteSpec, RouteSpecRegistry,
    SignedIngressRequest, SiteId, ToolSchemaRevision, TransformOwnerId, TransformOwnerVersion,
    VerifiedAuthorizationKind, VerifiedProofKind, VerifierClock,
};

const NOW: u64 = 1_800_000_000_000;
const EXPIRES: u64 = NOW + 60_000;
const MANAGED_KEY: [u8; 32] = [0x11; 32];
const MANAGED_KEY_2: [u8; 32] = [0x12; 32];
const CAPABILITY_KEY: [u8; 32] = [0x21; 32];

trait ResultTestExt<T, E> {
    fn expect_error(self, message: &str) -> E;
}

impl<T, E> ResultTestExt<T, E> for Result<T, E> {
    fn expect_error(self, message: &str) -> E {
        match self {
            Ok(_) => panic!("{message}"),
            Err(error) => error,
        }
    }
}

fn operation(value: u64) -> OperationId {
    OperationId::new(value)
}

fn json_body_policy(max_bytes: usize) -> BodyPolicy {
    BodyPolicy::bounded(max_bytes, Some(b"application/json")).expect("测试 body policy 合法")
}

fn build_registry() -> RouteSpecRegistry {
    RouteSpecRegistry::compile(vec![
        RouteSpec::try_new(
            operation(1),
            HttpMethod::Post,
            b"/v1/messages",
            QueryPolicy::Forbidden,
            json_body_policy(32 * 1024),
            RequestKind::ModelInference,
            RequestDispatchDomain::RoutedPolicy,
            vec![b"x-client-version".to_vec()],
            None,
        )
        .expect("模型 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(2),
            HttpMethod::Get,
            b"/health",
            QueryPolicy::Forbidden,
            BodyPolicy::Forbidden,
            RequestKind::Liveness,
            RequestDispatchDomain::Local,
            vec![],
            Some(LocalOperationAuthScope::PublicLiveness),
        )
        .expect("健康检查 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(3),
            HttpMethod::Get,
            b"/status",
            QueryPolicy::Forbidden,
            BodyPolicy::Forbidden,
            RequestKind::LocalAdmin,
            RequestDispatchDomain::Local,
            vec![],
            Some(LocalOperationAuthScope::LocalAdmin),
        )
        .expect("管理 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(4),
            HttpMethod::Get,
            b"/oauth/callback",
            QueryPolicy::Allowed,
            BodyPolicy::Forbidden,
            RequestKind::AuthFlow,
            RequestDispatchDomain::Local,
            vec![],
            Some(LocalOperationAuthScope::AuthFlow),
        )
        .expect("认证 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(5),
            HttpMethod::Get,
            b"/v1/models",
            QueryPolicy::Forbidden,
            BodyPolicy::Forbidden,
            RequestKind::UnifiedModelCatalog,
            RequestDispatchDomain::Local,
            vec![],
            Some(LocalOperationAuthScope::LocalData),
        )
        .expect("模型目录 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(6),
            HttpMethod::Post,
            b"/management/models/probe",
            QueryPolicy::Forbidden,
            BodyPolicy::Forbidden,
            RequestKind::DeploymentModelProbe,
            RequestDispatchDomain::BoundDeployment,
            vec![],
            None,
        )
        .expect("模型探测 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(7),
            HttpMethod::Post,
            b"/v1/messages/count_tokens",
            QueryPolicy::Forbidden,
            json_body_policy(32 * 1024),
            RequestKind::ExactUpstreamTokenCount,
            RequestDispatchDomain::BoundDeployment,
            vec![b"x-client-version".to_vec()],
            None,
        )
        .expect("远程计数 RouteSpec 合法"),
        RouteSpec::try_new(
            operation(8),
            HttpMethod::Post,
            b"/v1/context/compact",
            QueryPolicy::Forbidden,
            json_body_policy(32 * 1024),
            RequestKind::AuxiliaryInference,
            RequestDispatchDomain::RoutedPolicy,
            vec![b"x-client-version".to_vec()],
            None,
        )
        .expect("辅助推理 RouteSpec 合法"),
    ])
    .expect("测试注册表合法")
}

fn header(name: &[u8], value: &[u8]) -> RawHeader {
    RawHeader::try_new(name, value).expect("测试 Header 合法")
}

fn model_request_with(body: &[u8], target: &[u8], client_version: &[u8]) -> RawIngressRequest {
    RawIngressRequest::try_new(
        HttpMethod::Post,
        target,
        vec![
            header(b"content-type", b"application/json; charset=utf-8"),
            header(b"x-client-version", client_version),
            header(b"authorization", b"Bearer LOCAL_INGRESS_TOKEN_CANARY"),
            header(b"cookie", b"SESSION_COOKIE_CANARY"),
            header(b"x-unbound-canary", b"UNSIGNED_HEADER_MUST_BE_STRIPPED"),
            header(b"content-length", body.len().to_string().as_bytes()),
        ],
        body.to_vec(),
    )
    .expect("测试模型请求合法")
}

fn model_request() -> RawIngressRequest {
    model_request_with(
        br#"{"messages":[{"role":"user","content":"hello"}]}"#,
        b"/v1/messages",
        b"1",
    )
}

fn token_count_request() -> RawIngressRequest {
    model_request_with(
        br#"{"messages":[{"role":"user","content":"count"}]}"#,
        b"/v1/messages/count_tokens",
        b"1",
    )
}

fn probe_request() -> RawIngressRequest {
    RawIngressRequest::try_new(
        HttpMethod::Post,
        b"/management/models/probe",
        vec![header(b"authorization", b"Bearer MANAGEMENT_TOKEN")],
        vec![],
    )
    .expect("测试探测请求合法")
}

fn local_request(path: &[u8]) -> RawIngressRequest {
    RawIngressRequest::try_new(HttpMethod::Get, path, vec![], vec![]).expect("测试本地请求合法")
}

fn nonce(value: u8) -> OneShotNonce {
    OneShotNonce::from_bytes([value; 16]).expect("测试 nonce 非零")
}

fn activation_key() -> ContextActivationKey {
    ContextActivationKey {
        client_family: ClientFamilyId::new(10),
        client_version: ClientVersion::new(20),
        adapter_version: AdapterVersion::new(30),
        ingress_schema_version: IngressSchemaVersion::new(1),
        context_policy_version: ContextPolicyVersion::new(40),
        transform_owner: TransformOwnerId::new(50),
        transform_owner_version: TransformOwnerVersion::new(60),
    }
}

fn activation_binding() -> ContextActivationBinding {
    let key = activation_key();
    ContextActivationBinding::new(
        key.client_family,
        key.client_version,
        key.adapter_version,
        key.ingress_schema_version,
        key.context_policy_version,
        key.transform_owner,
        key.transform_owner_version,
    )
}

fn allowed_targets(bound: bool) -> Vec<AllowedEgressTarget> {
    let mut targets = vec![AllowedEgressTarget {
        site: SiteId::new(100),
        deployment: ModelDeploymentId::new(200),
        origin: CanonicalOrigin::try_new(b"https://api.example.test").expect("Origin 合法"),
        trust_tier: 1,
    }];
    if !bound {
        targets.push(AllowedEgressTarget {
            site: SiteId::new(101),
            deployment: ModelDeploymentId::new(201),
            origin: CanonicalOrigin::try_new(b"https://fallback.example.test")
                .expect("Origin 合法"),
            trust_tier: 2,
        });
    }
    targets.sort();
    targets
}

fn managed_material(
    registry: &RouteSpecRegistry,
    request: &RawIngressRequest,
    one_shot_nonce: OneShotNonce,
    issuer_epoch: IssuerEpoch,
) -> (AuthorizationBundle, AttestationClaims) {
    let RequestBinding {
        operation,
        body_digest,
        semantic_headers_digest,
        request_digest,
        body_length,
    } = registry.bind_request(request).expect("测试请求匹配注册表");
    let bound = operation.spec.dispatch_domain == RequestDispatchDomain::BoundDeployment;
    let purpose = match operation.spec.kind {
        RequestKind::ModelInference => EgressPurpose::ModelInference,
        RequestKind::AuxiliaryInference => EgressPurpose::AuxiliaryInference,
        RequestKind::ExactUpstreamTokenCount => EgressPurpose::ExactUpstreamTokenCount,
        _ => panic!("测试 Managed material 只支持远程数据面"),
    };
    let envelope_digest = EnvelopeDigest::from_bytes([0x33; 32]);
    let bundle = AuthorizationBundle {
        permit: ContextEgressPermit {
            operation: operation.spec.operation,
            request_digest,
            body_digest,
            envelope_digest,
            nonce: one_shot_nonce,
            purpose,
            sensitivity: SensitivityClass::PrivateCode,
            max_outbound_bytes: 128 * 1024,
            allowed_targets: allowed_targets(bound),
            fallback_allowed: !bound,
            policy_revision: RoutePolicyRevision::new(70),
            consent_revision: ConsentRevision::new(80),
            expires_at_millis: EXPIRES,
        },
        requirements: ContextCapabilityRequirements {
            tool_schema_revision: ToolSchemaRevision::new(1),
            retrieval_schema_revision: RetrievalSchemaRevision::new(2),
            client_adapter_version: AdapterVersion::new(30),
            upstream_adapter_revision: AdapterContractRevision::new(3),
            handle_epoch: HandleEpoch::new(4),
            handle_earliest_expiry_millis: EXPIRES + 60_000,
            protocol_frame_revision: ProtocolFrameRevision::new(5),
            continuation: ContinuationConstraint::FullHistoryPortable,
            local_handle_required: true,
        },
        activation_key: activation_key(),
    };
    let claims = AttestationClaims {
        schema_version: 1,
        audience: AudienceId::new(90),
        issuer_epoch,
        issued_at_millis: NOW - 1_000,
        expires_at_millis: EXPIRES,
        listener: ListenerId::new(91),
        token_scope: IngressTokenScopeId::new(92),
        operation: operation.spec.operation,
        dispatch_domain: operation.spec.dispatch_domain,
        registry_digest: registry.digest(),
        method: request.method(),
        target: request.target().to_vec(),
        semantic_headers_digest,
        body_digest,
        body_length,
        request_digest,
        envelope_digest,
        authorization_bundle_digest: crate::AuthorizationBundleDigest::from_bytes([0; 32]),
        nonce: one_shot_nonce,
        policy_revision: RoutePolicyRevision::new(70),
        adapter_version: AdapterVersion::new(30),
        transform_owner_version: TransformOwnerVersion::new(60),
    };
    (bundle, claims)
}

fn encode_managed(
    bundle: &AuthorizationBundle,
    claims: &AttestationClaims,
    key: &[u8; 32],
) -> (Vec<u8>, Vec<u8>) {
    let bundle_wire = encode_authorization_bundle(bundle);
    let mut claims = claims.clone();
    claims.authorization_bundle_digest = authorization_bundle_digest(&bundle_wire);
    let attestation_wire = sign_attestation_for_test(&claims, key);
    (attestation_wire, bundle_wire)
}

fn managed_connection(runtime: &TestRuntime, epoch: IssuerEpoch) -> crate::ManagedListenerContext {
    runtime.listener_authority.managed(
        ListenerId::new(91),
        IngressTokenScopeId::new(92),
        AudienceId::new(90),
        epoch,
    )
}

struct TestRuntime {
    verifier: IngressVerifier,
    listener_authority: ListenerBindingAuthority,
    gateway_consent_authority: crate::GatewayConsentAuthority,
    managed_activation_authority: crate::ManagedActivationAuthority,
    receiver: crate::VerifiedIngressReceiver,
}

impl std::ops::Deref for TestRuntime {
    type Target = IngressVerifier;

    fn deref(&self) -> &Self::Target {
        &self.verifier
    }
}

fn managed_active(runtime: &TestRuntime, epoch: IssuerEpoch) -> ManagedActivationBinding {
    runtime.managed_activation_authority.issue_binding(
        ListenerId::new(91),
        IngressTokenScopeId::new(92),
        AudienceId::new(90),
        epoch,
        RoutePolicyRevision::new(70),
        ConsentRevision::new(80),
        activation_binding(),
        EXPIRES + 60_000,
    )
}

fn verifier_with(
    registry: RouteSpecRegistry,
    managed_keys: Vec<(IssuerEpoch, [u8; 32])>,
    store: Arc<dyn OneShotNonceStore>,
) -> TestRuntime {
    verifier_with_clock(
        registry,
        managed_keys,
        Arc::new(FixedClock::new(NOW)),
        store,
    )
}

fn verifier_with_clock(
    registry: RouteSpecRegistry,
    managed_keys: Vec<(IssuerEpoch, [u8; 32])>,
    clock: Arc<dyn VerifierClock>,
    store: Arc<dyn OneShotNonceStore>,
) -> TestRuntime {
    let managed_keys = managed_keys
        .into_iter()
        .map(|(epoch, key)| ManagedVerificationKey::try_new(epoch, key).expect("Managed key 合法"))
        .collect();
    let runtime = IngressVerifier::initialize(
        registry,
        ManagedVerificationKeyRing::try_new(managed_keys).expect("Managed key ring 合法"),
        CapabilityVerificationKeyRing::try_new(vec![CapabilityVerificationKey::try_new(
            IssuerEpoch::new(1),
            CAPABILITY_KEY,
        )
        .expect("Capability key 合法")])
        .expect("Capability key ring 合法"),
        clock,
        store,
    );
    let (
        verifier,
        listener_authority,
        gateway_consent_authority,
        managed_activation_authority,
        receiver,
    ) = runtime.into_parts();
    TestRuntime {
        verifier,
        listener_authority,
        gateway_consent_authority,
        managed_activation_authority,
        receiver,
    }
}

fn default_verifier(store: Arc<dyn OneShotNonceStore>) -> TestRuntime {
    verifier_with(
        build_registry(),
        vec![(IssuerEpoch::new(1), MANAGED_KEY)],
        store,
    )
}

fn fresh_store() -> Arc<dyn OneShotNonceStore> {
    Arc::new(MemoryNonceStore::new(4_096).expect("nonce store 合法"))
}

fn signed_from_wires(
    request: RawIngressRequest,
    attestation_wire: &[u8],
    bundle_wire: &[u8],
) -> SignedIngressRequest {
    SignedIngressRequest::try_new(request, attestation_wire.to_vec(), bundle_wire.to_vec())
        .expect("测试 signed wire 有界")
}

#[test]
fn managed_success_strips_ingress_secrets_and_preserves_verified_contract() {
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(1), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let verifier = default_verifier(fresh_store());
    let verified = verifier
        .verify_managed(
            signed_from_wires(request, &attestation_wire, &bundle_wire),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        )
        .expect("合法 Managed 请求应通过");

    assert_eq!(verified.proof_kind(), VerifiedProofKind::Managed);
    assert_eq!(
        verified.authorization_kind(),
        VerifiedAuthorizationKind::ManagedEgress
    );
    assert_eq!(verified.operation(), operation(1));
    assert_eq!(
        verified.dispatch_domain(),
        RequestDispatchDomain::RoutedPolicy
    );
    assert_eq!(verified.managed_target_count(), Some(2));
    assert!(verified.local_handle_allowed());
    assert!(verified.proof_binding_is_nonzero());
    let names = verified
        .headers()
        .map(|header| header.name().to_vec())
        .collect::<Vec<_>>();
    assert!(names.contains(&b"content-type".to_vec()));
    assert!(names.contains(&b"x-client-version".to_vec()));
    assert!(!names.contains(&b"authorization".to_vec()));
    assert!(!names.contains(&b"cookie".to_vec()));
    assert!(!names.contains(&b"x-unbound-canary".to_vec()));
    assert!(!names.contains(&b"content-length".to_vec()));
}

#[test]
fn verification_runtime_seal_rejects_cross_instance_contexts_and_requests() {
    let runtime_a = default_verifier(fresh_store());
    let runtime_b = default_verifier(fresh_store());
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(40), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);

    assert_eq!(
        runtime_a
            .verify_managed(
                signed_from_wires(model_request(), &attestation_wire, &bundle_wire),
                &managed_connection(&runtime_b, IssuerEpoch::new(1)),
                &managed_active(&runtime_a, IssuerEpoch::new(1)),
            )
            .expect_error("其他 runtime 的 listener context 必须拒绝"),
        IngressReject::VerificationDomainMismatch
    );

    let verified = runtime_a
        .verify_managed(
            signed_from_wires(request, &attestation_wire, &bundle_wire),
            &managed_connection(&runtime_a, IssuerEpoch::new(1)),
            &managed_active(&runtime_a, IssuerEpoch::new(1)),
        )
        .expect("同一 runtime 的 context 应通过");
    assert_eq!(
        runtime_b
            .receiver
            .accept(verified)
            .expect_error("其他 runtime 的 classifier receiver 必须拒绝"),
        IngressReject::VerificationDomainMismatch
    );

    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(41), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let verified = runtime_a
        .verify_managed(
            signed_from_wires(request, &attestation_wire, &bundle_wire),
            &managed_connection(&runtime_a, IssuerEpoch::new(1)),
            &managed_active(&runtime_a, IssuerEpoch::new(1)),
        )
        .expect("第二个同域请求应通过 verifier");
    runtime_a
        .receiver
        .accept(verified)
        .expect("同域 classifier receiver 应接受");
}

#[test]
fn managed_every_attestation_and_bundle_byte_is_authenticated() {
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(2), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);

    for index in 0..attestation_wire.len() {
        let mut tampered = attestation_wire.clone();
        tampered[index] ^= 1;
        let verifier = default_verifier(fresh_store());
        let result = verifier.verify_managed(
            signed_from_wires(model_request(), &tampered, &bundle_wire),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        );
        assert!(result.is_err(), "Attestation byte {index} 未被保护");
    }

    for index in 0..bundle_wire.len() {
        let mut tampered = bundle_wire.clone();
        tampered[index] ^= 1;
        let verifier = default_verifier(fresh_store());
        let result = verifier.verify_managed(
            signed_from_wires(model_request(), &attestation_wire, &tampered),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        );
        assert!(
            result.is_err(),
            "Authorization bundle byte {index} 未被保护"
        );
    }
}

#[test]
fn managed_raw_body_target_and_semantic_header_changes_fail_before_nonce_consumption() {
    let registry = build_registry();
    let request = model_request();
    let one_shot_nonce = nonce(3);
    let (bundle, claims) =
        managed_material(&registry, &request, one_shot_nonce, IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let store = fresh_store();
    let verifier = default_verifier(Arc::clone(&store));

    let changed_body = model_request_with(
        br#"{"messages": [{"content":"hello","role":"user"}]}"#,
        b"/v1/messages",
        b"1",
    );
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(changed_body, &attestation_wire, &bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("JSON 重排必须改变原始正文绑定"),
        IngressReject::RequestBindingMismatch
    );

    let changed_header = model_request_with(
        br#"{"messages":[{"role":"user","content":"hello"}]}"#,
        b"/v1/messages",
        b"2",
    );
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(changed_header, &attestation_wire, &bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("语义 Header 改变必须拒绝"),
        IngressReject::RequestBindingMismatch
    );

    let verified = verifier
        .verify_managed(
            signed_from_wires(model_request(), &attestation_wire, &bundle_wire),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        )
        .expect("此前的绑定失败不得烧毁合法 nonce");
    assert_eq!(verified.proof_kind(), VerifiedProofKind::Managed);
}

#[test]
fn managed_scope_and_authorization_mismatches_do_not_consume_nonce() {
    let registry = build_registry();
    let request = model_request();
    let one_shot_nonce = nonce(4);
    let (bundle, claims) =
        managed_material(&registry, &request, one_shot_nonce, IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let store = fresh_store();
    let verifier = default_verifier(Arc::clone(&store));
    let wrong_connection = verifier.listener_authority.managed(
        ListenerId::new(999),
        IngressTokenScopeId::new(92),
        AudienceId::new(90),
        IssuerEpoch::new(1),
    );
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(model_request(), &attestation_wire, &bundle_wire),
                &wrong_connection,
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("错误 listener 必须拒绝"),
        IngressReject::ScopeMismatch
    );

    let mut mismatched_bundle = bundle.clone();
    mismatched_bundle.permit.request_digest = crate::RequestDigest::from_bytes([0x88; 32]);
    let (mismatched_attestation, mismatched_bundle_wire) =
        encode_managed(&mismatched_bundle, &claims, &MANAGED_KEY);
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(
                    model_request(),
                    &mismatched_attestation,
                    &mismatched_bundle_wire,
                ),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("合法 MAC 也不能掩盖 bundle/request 错配"),
        IngressReject::AuthorizationBindingMismatch
    );

    verifier
        .verify_managed(
            signed_from_wires(model_request(), &attestation_wire, &bundle_wire),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        )
        .expect("scope/bundle 错配不得烧毁 nonce");
}

#[test]
fn managed_handle_requirement_requires_expiry_binding() {
    let registry = build_registry();
    let request = model_request();
    let one_shot_nonce = nonce(42);
    let (bundle, claims) =
        managed_material(&registry, &request, one_shot_nonce, IssuerEpoch::new(1));
    let verifier = default_verifier(fresh_store());

    let mut missing_expiry = bundle.clone();
    missing_expiry.requirements.handle_earliest_expiry_millis = 0;
    let (missing_attestation, missing_bundle_wire) =
        encode_managed(&missing_expiry, &claims, &MANAGED_KEY);
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(model_request(), &missing_attestation, &missing_bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("要求 local handle 时缺少过期绑定必须拒绝"),
        IngressReject::AuthorizationBindingMismatch
    );

    let mut optional_handle = bundle;
    optional_handle.requirements.local_handle_required = false;
    optional_handle.requirements.handle_earliest_expiry_millis = 0;
    let (optional_attestation, optional_bundle_wire) =
        encode_managed(&optional_handle, &claims, &MANAGED_KEY);
    let verified = verifier
        .verify_managed(
            signed_from_wires(
                model_request(),
                &optional_attestation,
                &optional_bundle_wire,
            ),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        )
        .expect("非必需 local handle 且无过期绑定应放行");
    assert!(!verified.local_handle_allowed());
}

#[test]
fn managed_bound_token_count_forbids_fallback_and_multiple_targets() {
    let registry = build_registry();
    let request = token_count_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(5), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let verifier = default_verifier(fresh_store());
    let verified = verifier
        .verify_managed(
            signed_from_wires(request, &attestation_wire, &bundle_wire),
            &managed_connection(&verifier, IssuerEpoch::new(1)),
            &managed_active(&verifier, IssuerEpoch::new(1)),
        )
        .expect("单目标、无 fallback 的 ExactUpstream 应通过 Context gate");
    assert_eq!(
        verified.dispatch_domain(),
        RequestDispatchDomain::BoundDeployment
    );
    assert_eq!(verified.managed_target_count(), Some(1));

    let registry = build_registry();
    let request = token_count_request();
    let (mut bad_bundle, bad_claims) =
        managed_material(&registry, &request, nonce(6), IssuerEpoch::new(1));
    bad_bundle.permit.fallback_allowed = true;
    bad_bundle.permit.allowed_targets = allowed_targets(false);
    let (bad_attestation, bad_bundle_wire) = encode_managed(&bad_bundle, &bad_claims, &MANAGED_KEY);
    let verifier = default_verifier(fresh_store());
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(request, &bad_attestation, &bad_bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("BoundDeployment 不得携带 fallback 或第二目标"),
        IngressReject::CapabilityConstraintMismatch
    );
}

#[test]
fn managed_nonce_is_atomic_under_concurrency() {
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(7), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let verifier = Arc::new(default_verifier(fresh_store()));
    let connection = Arc::new(managed_connection(&verifier, IssuerEpoch::new(1)));
    let active = Arc::new(managed_active(&verifier, IssuerEpoch::new(1)));
    let attestation_wire = Arc::new(attestation_wire);
    let bundle_wire = Arc::new(bundle_wire);
    let barrier = Arc::new(Barrier::new(32));

    let handles = (0..32)
        .map(|_| {
            let verifier = Arc::clone(&verifier);
            let connection = Arc::clone(&connection);
            let active = Arc::clone(&active);
            let attestation_wire = Arc::clone(&attestation_wire);
            let bundle_wire = Arc::clone(&bundle_wire);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                verifier.verify_managed(
                    signed_from_wires(model_request(), &attestation_wire, &bundle_wire),
                    &connection,
                    &active,
                )
            })
        })
        .collect::<Vec<_>>();

    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("验证线程不应 panic"))
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(IngressReject::NonceRejected)))
            .count(),
        31
    );
}

#[test]
fn managed_key_rotation_accepts_bound_epoch_and_rejects_unknown_epoch() {
    let epoch = IssuerEpoch::new(2);
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(8), epoch);
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY_2);
    let verifier = verifier_with(
        build_registry(),
        vec![
            (IssuerEpoch::new(1), MANAGED_KEY),
            (IssuerEpoch::new(2), MANAGED_KEY_2),
        ],
        fresh_store(),
    );
    verifier
        .verify_managed(
            signed_from_wires(request, &attestation_wire, &bundle_wire),
            &managed_connection(&verifier, epoch),
            &managed_active(&verifier, epoch),
        )
        .expect("轮换窗口内正确 epoch 应通过");

    let unknown_epoch = IssuerEpoch::new(3);
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(9), unknown_epoch);
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &[0x13; 32]);
    let verifier = verifier_with(
        build_registry(),
        vec![(IssuerEpoch::new(1), MANAGED_KEY)],
        fresh_store(),
    );
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(request, &attestation_wire, &bundle_wire),
                &managed_connection(&verifier, unknown_epoch),
                &managed_active(&verifier, unknown_epoch),
            )
            .expect_error("未知 key epoch 必须拒绝"),
        IngressReject::IssuerEpochUnknown
    );
}

#[test]
fn canonical_wire_rejects_unknown_duplicate_out_of_order_and_trailing_fields() {
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(10), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    assert!(decode_authorization_bundle(&bundle_wire).is_ok());
    assert!(decode_attestation(&attestation_wire).is_ok());

    let offsets = tlv_offsets(&bundle_wire, AUTHORIZATION_BUNDLE_PREFIX.len());
    assert_eq!(offsets.len(), 3);

    let mut unknown = bundle_wire.clone();
    unknown[offsets[0]..offsets[0] + 2].copy_from_slice(&u16::MAX.to_be_bytes());
    assert_eq!(
        decode_authorization_bundle(&unknown).expect_error("未知字段必须拒绝"),
        IngressReject::AuthorizationBundleMalformed
    );

    let mut duplicate = bundle_wire.clone();
    duplicate[offsets[1]..offsets[1] + 2].copy_from_slice(&1u16.to_be_bytes());
    assert_eq!(
        decode_authorization_bundle(&duplicate).expect_error("重复字段必须拒绝"),
        IngressReject::AuthorizationBundleMalformed
    );

    let mut out_of_order = bundle_wire.clone();
    out_of_order[offsets[0]..offsets[0] + 2].copy_from_slice(&2u16.to_be_bytes());
    assert_eq!(
        decode_authorization_bundle(&out_of_order).expect_error("乱序字段必须拒绝"),
        IngressReject::AuthorizationBundleMalformed
    );

    let mut trailing = attestation_wire.clone();
    trailing.push(0);
    assert_eq!(
        decode_attestation(&trailing).expect_error("trailing bytes 必须拒绝"),
        IngressReject::ProofMalformed
    );

    let mut bad_prefix = attestation_wire;
    bad_prefix[ATTESTATION_PREFIX.len() - 2] ^= 1;
    assert_eq!(
        decode_attestation(&bad_prefix).expect_error("错误 domain/version 必须拒绝"),
        IngressReject::ProofMalformed
    );
}

#[test]
fn authorization_target_list_must_be_sorted_and_unique() {
    let registry = build_registry();
    let request = model_request();
    let (mut bundle, _) = managed_material(&registry, &request, nonce(11), IssuerEpoch::new(1));
    bundle.permit.allowed_targets.reverse();
    let wire = encode_authorization_bundle(&bundle);
    assert_eq!(
        decode_authorization_bundle(&wire).expect_error("未排序目标必须拒绝"),
        IngressReject::AuthorizationBundleMalformed
    );

    let registry = build_registry();
    let request = model_request();
    let (mut bundle, _) = managed_material(&registry, &request, nonce(12), IssuerEpoch::new(1));
    bundle.permit.allowed_targets = vec![
        bundle.permit.allowed_targets[0].clone(),
        bundle.permit.allowed_targets[0].clone(),
    ];
    let wire = encode_authorization_bundle(&bundle);
    assert_eq!(
        decode_authorization_bundle(&wire).expect_error("重复目标必须拒绝"),
        IngressReject::AuthorizationBundleMalformed
    );

    let registry = build_registry();
    let request = model_request();
    let (mut bundle, _) = managed_material(&registry, &request, nonce(13), IssuerEpoch::new(1));
    let mut same_target_other_tier = bundle.permit.allowed_targets[0].clone();
    same_target_other_tier.trust_tier = 2;
    bundle.permit.allowed_targets = vec![
        bundle.permit.allowed_targets[0].clone(),
        same_target_other_tier,
    ];
    let wire = encode_authorization_bundle(&bundle);
    assert_eq!(
        decode_authorization_bundle(&wire)
            .expect_error("同一逻辑目标不能用不同 TrustTier 重复授权"),
        IngressReject::AuthorizationBundleMalformed
    );
}

#[test]
fn gateway_only_is_typed_scoped_and_never_grants_local_handles() {
    let verifier = default_verifier(fresh_store());
    let connection = verifier.listener_authority.gateway_only(
        ListenerId::new(200),
        IngressTokenScopeId::new(201),
        AudienceId::new(202),
    );
    let consent = verifier
        .gateway_consent_authority
        .issue_snapshot(
            ListenerId::new(200),
            IngressTokenScopeId::new(201),
            AudienceId::new(202),
            RoutePolicyRevision::new(203),
            ConsentRevision::new(204),
            2,
            NOW - 1_000,
            NOW + 60_000,
        )
        .expect("Gateway consent 合法");
    let verified = verifier
        .verify_gateway_only(model_request(), &connection, &consent)
        .expect("合法 GatewayOnly 请求应通过");
    assert_eq!(
        verified.proof_kind(),
        VerifiedProofKind::GatewayOnlyScopedConsent
    );
    assert_eq!(
        verified.authorization_kind(),
        VerifiedAuthorizationKind::GatewayOnlyExplicit
    );
    assert!(!verified.local_handle_allowed());
    assert_eq!(verified.gateway_only_maximum_trust_tier(), Some(2));
    assert_eq!(verified.gateway_constraint_snapshot(), Some((204, 203, 2)));
    assert!(verified.proof_binding_is_nonzero());
}

#[test]
fn gateway_only_rejects_wrong_scope_expired_consent_and_local_routes() {
    let verifier = default_verifier(fresh_store());
    let connection = verifier.listener_authority.gateway_only(
        ListenerId::new(200),
        IngressTokenScopeId::new(201),
        AudienceId::new(202),
    );
    let wrong_scope = verifier
        .gateway_consent_authority
        .issue_snapshot(
            ListenerId::new(999),
            IngressTokenScopeId::new(201),
            AudienceId::new(202),
            RoutePolicyRevision::new(203),
            ConsentRevision::new(204),
            1,
            NOW - 1_000,
            NOW + 60_000,
        )
        .expect("测试 consent 可构造");
    assert_eq!(
        verifier
            .verify_gateway_only(model_request(), &connection, &wrong_scope)
            .expect_error("错误 listener scope 必须拒绝"),
        IngressReject::ScopeMismatch
    );

    let expired = verifier
        .gateway_consent_authority
        .issue_snapshot(
            ListenerId::new(200),
            IngressTokenScopeId::new(201),
            AudienceId::new(202),
            RoutePolicyRevision::new(203),
            ConsentRevision::new(204),
            1,
            NOW - 10_000,
            NOW - 1,
        )
        .expect("过期值由 verifier 判断");
    assert_eq!(
        verifier
            .verify_gateway_only(model_request(), &connection, &expired)
            .expect_error("过期 consent 必须拒绝"),
        IngressReject::GatewayConsentInvalid
    );

    let valid = verifier
        .gateway_consent_authority
        .issue_snapshot(
            ListenerId::new(200),
            IngressTokenScopeId::new(201),
            AudienceId::new(202),
            RoutePolicyRevision::new(203),
            ConsentRevision::new(204),
            1,
            NOW - 1_000,
            NOW + 60_000,
        )
        .expect("测试 consent 合法");
    assert_eq!(
        verifier
            .verify_gateway_only(local_request(b"/health"), &connection, &valid)
            .expect_error("GatewayOnly 不能取得 Local disposition"),
        IngressReject::ProofModeConflict
    );
}

#[test]
fn local_operation_scope_is_closed_and_has_no_egress_authority() {
    let verifier = default_verifier(fresh_store());
    let public = verifier.listener_authority.local(
        ListenerId::new(300),
        IngressTokenScopeId::new(301),
        LocalOperationAuthScope::PublicLiveness,
    );
    let verified = verifier
        .verify_local_operation(local_request(b"/health"), &public)
        .expect("public_liveness 应通过");
    assert_eq!(
        verified.proof_kind(),
        VerifiedProofKind::LocalOperationScoped
    );
    assert_eq!(
        verified.authorization_kind(),
        VerifiedAuthorizationKind::None
    );
    assert_eq!(
        verified.local_auth_scope(),
        Some(LocalOperationAuthScope::PublicLiveness)
    );

    let local_admin = verifier.listener_authority.local(
        ListenerId::new(300),
        IngressTokenScopeId::new(301),
        LocalOperationAuthScope::LocalAdmin,
    );
    assert_eq!(
        verifier
            .verify_local_operation(local_request(b"/health"), &local_admin)
            .expect_error("LocalAdmin scope 不能冒充 public_liveness"),
        IngressReject::LocalScopeMismatch
    );
    assert_eq!(
        verifier
            .verify_local_operation(model_request(), &public)
            .expect_error("Local proof 不能进入 RoutedPolicy"),
        IngressReject::DispatchDomainMismatch
    );
}

fn capability_claims(
    registry: &RouteSpecRegistry,
    request: &RawIngressRequest,
    one_shot_nonce: OneShotNonce,
) -> CapabilityClaims {
    let binding = registry.bind_request(request).expect("探测请求匹配注册表");
    CapabilityClaims {
        schema_version: 1,
        audience: AudienceId::new(402),
        issuer_epoch: IssuerEpoch::new(1),
        issued_at_millis: NOW - 1_000,
        deadline_millis: NOW + 60_000,
        listener: ListenerId::new(400),
        token_scope: IngressTokenScopeId::new(401),
        operation: operation(6),
        dispatch_domain: RequestDispatchDomain::BoundDeployment,
        registry_digest: registry.digest(),
        deployment: ModelDeploymentId::new(410),
        endpoint: EndpointId::new(411),
        origin: CanonicalOrigin::try_new(b"https://probe.example.test:8443")
            .expect("探测 Origin 合法"),
        account_selector: AccountSelectorId::new(412),
        account: AccountId::new(413),
        credential: CredentialId::new(414),
        adapter_contract_revision: AdapterContractRevision::new(415),
        management_scope: CapabilityManagementScopeId::new(416),
        request_digest: binding.request_digest,
        nonce: one_shot_nonce,
        fallback_forbidden: true,
    }
}

fn capability_connection(runtime: &TestRuntime) -> crate::CapabilityListenerContext {
    runtime.listener_authority.capability(
        ListenerId::new(400),
        IngressTokenScopeId::new(401),
        AudienceId::new(402),
        IssuerEpoch::new(1),
        CapabilityManagementScopeId::new(416),
    )
}

#[test]
fn capability_scoped_binds_exact_target_account_credential_and_forbids_fallback() {
    let registry = build_registry();
    let request = probe_request();
    let claims = capability_claims(&registry, &request, nonce(20));
    let wire = sign_capability_for_test(&claims, &CAPABILITY_KEY);
    let verifier = default_verifier(fresh_store());
    let verified = verifier
        .verify_capability_probe(
            request,
            &capability_connection(&verifier),
            EncodedCapabilityAuthorization::try_new(wire).expect("Capability wire 有界"),
        )
        .expect("合法 CapabilityScoped 探测应通过");
    assert_eq!(verified.proof_kind(), VerifiedProofKind::CapabilityScoped);
    assert_eq!(
        verified.authorization_kind(),
        VerifiedAuthorizationKind::CapabilityBound
    );
    let binding = verified
        .capability_binding()
        .expect("Capability proof 应有绑定视图");
    assert_eq!(binding.deployment(), ModelDeploymentId::new(410));
    assert_eq!(binding.endpoint(), EndpointId::new(411));
    assert_eq!(binding.account_selector(), AccountSelectorId::new(412));
    assert_eq!(binding.account(), AccountId::new(413));
    assert_eq!(binding.credential(), CredentialId::new(414));
    assert_eq!(
        binding.origin().as_bytes(),
        b"https://probe.example.test:8443"
    );
    assert!(binding.fallback_forbidden());
}

#[test]
fn capability_every_wire_byte_is_authenticated_and_replay_is_rejected() {
    let registry = build_registry();
    let request = probe_request();
    let claims = capability_claims(&registry, &request, nonce(21));
    let wire = sign_capability_for_test(&claims, &CAPABILITY_KEY);
    assert!(decode_capability_authorization(&wire).is_ok());

    for index in 0..wire.len() {
        let mut tampered = wire.clone();
        tampered[index] ^= 1;
        let verifier = default_verifier(fresh_store());
        let result = verifier.verify_capability_probe(
            probe_request(),
            &capability_connection(&verifier),
            EncodedCapabilityAuthorization::try_new(tampered).expect("变异 wire 仍有界"),
        );
        assert!(result.is_err(), "Capability wire byte {index} 未被保护");
    }

    let store = fresh_store();
    let verifier = default_verifier(Arc::clone(&store));
    verifier
        .verify_capability_probe(
            probe_request(),
            &capability_connection(&verifier),
            EncodedCapabilityAuthorization::try_new(wire.clone()).expect("wire 有界"),
        )
        .expect("首次 Capability 应通过");
    assert_eq!(
        verifier
            .verify_capability_probe(
                probe_request(),
                &capability_connection(&verifier),
                EncodedCapabilityAuthorization::try_new(wire).expect("wire 有界"),
            )
            .expect_error("重放 Capability 必须拒绝"),
        IngressReject::NonceRejected
    );
}

#[test]
fn capability_rejects_wrong_scope_expiry_fallback_and_non_probe_operations() {
    let registry = build_registry();
    let request = probe_request();
    let claims = capability_claims(&registry, &request, nonce(22));
    let wire = sign_capability_for_test(&claims, &CAPABILITY_KEY);
    let verifier = default_verifier(fresh_store());
    let wrong_scope = verifier.listener_authority.capability(
        ListenerId::new(400),
        IngressTokenScopeId::new(401),
        AudienceId::new(402),
        IssuerEpoch::new(1),
        CapabilityManagementScopeId::new(999),
    );
    assert_eq!(
        verifier
            .verify_capability_probe(
                request,
                &wrong_scope,
                EncodedCapabilityAuthorization::try_new(wire).expect("wire 有界"),
            )
            .expect_error("错误 management scope 必须拒绝"),
        IngressReject::CapabilityConstraintMismatch
    );

    let registry = build_registry();
    let request = probe_request();
    let mut expired = capability_claims(&registry, &request, nonce(23));
    expired.issued_at_millis = NOW - 2_000;
    expired.deadline_millis = NOW - 1;
    let wire = sign_capability_for_test(&expired, &CAPABILITY_KEY);
    let verifier = default_verifier(fresh_store());
    assert_eq!(
        verifier
            .verify_capability_probe(
                request,
                &capability_connection(&verifier),
                EncodedCapabilityAuthorization::try_new(wire).expect("wire 有界"),
            )
            .expect_error("过期 capability 必须拒绝"),
        IngressReject::TimeWindowInvalid
    );

    let registry = build_registry();
    let request = probe_request();
    let mut fallback = capability_claims(&registry, &request, nonce(24));
    fallback.fallback_forbidden = false;
    let wire = sign_capability_for_test(&fallback, &CAPABILITY_KEY);
    let verifier = default_verifier(fresh_store());
    assert_eq!(
        verifier
            .verify_capability_probe(
                request,
                &capability_connection(&verifier),
                EncodedCapabilityAuthorization::try_new(wire).expect("wire 有界"),
            )
            .expect_error("fallback!=Forbidden 必须拒绝"),
        IngressReject::CapabilityAuthorizationMalformed
    );

    let registry = build_registry();
    let request = token_count_request();
    let binding = registry.bind_request(&request).expect("计数请求匹配");
    let mut wrong_operation = capability_claims(&build_registry(), &probe_request(), nonce(25));
    wrong_operation.operation = operation(7);
    wrong_operation.request_digest = binding.request_digest;
    let wire = sign_capability_for_test(&wrong_operation, &CAPABILITY_KEY);
    let verifier = default_verifier(fresh_store());
    assert_eq!(
        verifier
            .verify_capability_probe(
                request,
                &capability_connection(&verifier),
                EncodedCapabilityAuthorization::try_new(wire).expect("wire 有界"),
            )
            .expect_error("携正文 ExactUpstream 不能冒充无正文 capability probe"),
        IngressReject::CapabilityConstraintMismatch
    );
}

#[test]
fn nonce_store_is_bounded_namespaced_and_fail_closed() {
    let store = MemoryNonceStore::new(1).expect("容量 1 合法");
    store
        .consume(
            NonceNamespace::ManagedAttestation,
            IssuerEpoch::new(1),
            nonce(30),
            NOW + 1_000,
            NOW,
        )
        .expect("首次消费应通过");
    assert_eq!(store.len().expect("store 可读"), 1);
    assert_eq!(
        store
            .consume(
                NonceNamespace::ManagedAttestation,
                IssuerEpoch::new(1),
                nonce(30),
                NOW + 1_000,
                NOW,
            )
            .expect_error("同 namespace/epoch/nonce 必须重放"),
        NonceReject::Replayed
    );
    assert_eq!(
        store
            .consume(
                NonceNamespace::CapabilityAuthorization,
                IssuerEpoch::new(1),
                nonce(30),
                NOW + 1_000,
                NOW,
            )
            .expect_error("仍有效条目不能因 namespace 不同被淘汰"),
        NonceReject::CapacityExhausted
    );
    store
        .consume(
            NonceNamespace::CapabilityAuthorization,
            IssuerEpoch::new(1),
            nonce(31),
            NOW + 3_000,
            NOW + 2_000,
        )
        .expect("过期条目清理后新 namespace 应通过");
}

#[test]
fn nonce_store_high_water_prevents_clock_rollback_replay() {
    let store = MemoryNonceStore::new(4).expect("容量合法");
    let replayed_nonce = nonce(33);
    store
        .consume(
            NonceNamespace::ManagedAttestation,
            IssuerEpoch::new(1),
            replayed_nonce,
            200,
            100,
        )
        .expect("首次 nonce 应通过");
    store
        .consume(
            NonceNamespace::ManagedAttestation,
            IssuerEpoch::new(1),
            nonce(34),
            300,
            201,
        )
        .expect("推进 high-water 并清理过期条目");
    assert_eq!(
        store
            .consume(
                NonceNamespace::ManagedAttestation,
                IssuerEpoch::new(1),
                replayed_nonce,
                200,
                150,
            )
            .expect_error("墙钟回拨不得复活已过期 nonce"),
        NonceReject::Expired
    );
}

#[test]
fn verifier_clock_high_water_keeps_expired_proofs_expired_after_rollback() {
    let clock = Arc::new(FixedClock::new(NOW));
    let verifier_clock: Arc<dyn VerifierClock> = clock.clone();
    let verifier = verifier_with_clock(
        build_registry(),
        vec![(IssuerEpoch::new(1), MANAGED_KEY)],
        verifier_clock,
        fresh_store(),
    );
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(35), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);

    clock.set(EXPIRES + 1);
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(model_request(), &attestation_wire, &bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("过期证明必须拒绝"),
        IngressReject::TimeWindowInvalid
    );
    clock.set(NOW);
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(request, &attestation_wire, &bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("墙钟回拨不得复活已过期证明"),
        IngressReject::TimeWindowInvalid
    );
}

struct FailingNonceStore;

impl OneShotNonceStore for FailingNonceStore {
    fn consume(
        &self,
        _namespace: NonceNamespace,
        _issuer_epoch: IssuerEpoch,
        _nonce: OneShotNonce,
        _expires_at_millis: u64,
        _now_millis: u64,
    ) -> Result<(), NonceReject> {
        Err(NonceReject::StoreUnavailable)
    }
}

#[test]
fn nonce_store_failure_never_produces_verified_ingress() {
    let registry = build_registry();
    let request = model_request();
    let (bundle, claims) = managed_material(&registry, &request, nonce(32), IssuerEpoch::new(1));
    let (attestation_wire, bundle_wire) = encode_managed(&bundle, &claims, &MANAGED_KEY);
    let verifier = default_verifier(Arc::new(FailingNonceStore));
    assert_eq!(
        verifier
            .verify_managed(
                signed_from_wires(request, &attestation_wire, &bundle_wire),
                &managed_connection(&verifier, IssuerEpoch::new(1)),
                &managed_active(&verifier, IssuerEpoch::new(1)),
            )
            .expect_error("nonce store 故障必须 fail closed"),
        IngressReject::NonceRejected
    );
}

#[test]
fn raw_request_and_route_spec_reject_smuggling_and_ambiguous_inputs() {
    assert_eq!(
        RawHeader::try_new(b"x-test", b"ok\x01bad").expect_error("C0 控制字符必须拒绝"),
        IngressReject::HeaderMalformed
    );
    assert_eq!(
        RawIngressRequest::try_new(
            HttpMethod::Post,
            b"/v1/messages",
            vec![header(b"authorization", b"a"), header(b"x-api-key", b"b"),],
            vec![],
        )
        .expect_error("冲突认证输入必须拒绝"),
        IngressReject::ConflictingInboundAuthentication
    );
    assert_eq!(
        RawIngressRequest::try_new(
            HttpMethod::Post,
            b"/v1/messages",
            vec![header(b"x-bianma-ingress-mode", b"managed")],
            vec![],
        )
        .expect_error("请求不能声明自己的 IngressMode"),
        IngressReject::ReservedProofHeader
    );
    assert_eq!(
        RawIngressRequest::try_new(HttpMethod::Post, b"/v1/%252fmessages", vec![], vec![],)
            .expect_error("重复解码路径必须拒绝"),
        IngressReject::RequestMalformed
    );
    assert_eq!(
        RawIngressRequest::try_new(
            HttpMethod::Post,
            b"/v1/messages",
            vec![header(b"content-length", b"1")],
            vec![],
        )
        .expect_error("伪造较小/较大 Content-Length 必须拒绝"),
        IngressReject::RequestMalformed
    );
    assert_eq!(
        RouteSpec::try_new(
            operation(100),
            HttpMethod::Post,
            b"/unsafe",
            QueryPolicy::Forbidden,
            json_body_policy(100),
            RequestKind::ModelInference,
            RequestDispatchDomain::RoutedPolicy,
            vec![b"authorization".to_vec()],
            None,
        )
        .expect_error("RouteSpec 不能把认证 Header 变成语义输入"),
        IngressReject::RouteSpecInvalid
    );
    assert_eq!(
        RouteSpec::try_new(
            operation(101),
            HttpMethod::Get,
            b"/unsafe-models",
            QueryPolicy::Forbidden,
            BodyPolicy::Forbidden,
            RequestKind::UnifiedModelCatalog,
            RequestDispatchDomain::Local,
            vec![],
            Some(LocalOperationAuthScope::PublicLiveness),
        )
        .expect_error("模型目录不能借用 public_liveness scope"),
        IngressReject::RouteSpecInvalid
    );
}

#[test]
fn route_registry_is_closed_and_preflight_is_exact() {
    assert_eq!(
        build_registry()
            .bind_request(&local_request(b"/unknown"))
            .expect_error("未知路径必须拒绝"),
        IngressReject::RouteNotFound
    );
    let wrong_method = RawIngressRequest::try_new(HttpMethod::Get, b"/v1/messages", vec![], vec![])
        .expect("请求语法合法");
    assert_eq!(
        build_registry()
            .bind_request(&wrong_method)
            .expect_error("错误方法必须拒绝"),
        IngressReject::MethodNotAllowed
    );
    let query = model_request_with(br#"{"messages":[]}"#, b"/v1/messages?x=1", b"1");
    assert_eq!(
        build_registry()
            .bind_request(&query)
            .expect_error("禁止 query 的 Operation 必须拒绝"),
        IngressReject::QueryNotAllowed
    );
    let wrong_type = RawIngressRequest::try_new(
        HttpMethod::Post,
        b"/v1/messages",
        vec![header(b"content-type", b"text/plain")],
        b"hello".to_vec(),
    )
    .expect("请求语法合法");
    assert_eq!(
        build_registry()
            .bind_request(&wrong_type)
            .expect_error("错误 Content-Type 必须拒绝"),
        IngressReject::ContentTypeNotAllowed
    );

    let duplicate = RouteSpec::try_new(
        operation(1),
        HttpMethod::Get,
        b"/another",
        QueryPolicy::Forbidden,
        BodyPolicy::Forbidden,
        RequestKind::UnifiedModelCatalog,
        RequestDispatchDomain::Local,
        vec![],
        Some(LocalOperationAuthScope::LocalData),
    )
    .expect("单个 RouteSpec 合法");
    let existing = RouteSpec::try_new(
        operation(1),
        HttpMethod::Get,
        b"/models-2",
        QueryPolicy::Forbidden,
        BodyPolicy::Forbidden,
        RequestKind::UnifiedModelCatalog,
        RequestDispatchDomain::Local,
        vec![],
        Some(LocalOperationAuthScope::LocalData),
    )
    .expect("单个 RouteSpec 合法");
    assert_eq!(
        RouteSpecRegistry::compile(vec![duplicate, existing])
            .expect_error("重复 Operation ID 必须拒绝"),
        IngressReject::RegistryInvalid
    );
}

#[test]
fn canonical_origin_rejects_credentials_noncanonical_ports_and_non_loopback_http() {
    for invalid in [
        b"https://user@example.test".as_slice(),
        b"https://example.test:443".as_slice(),
        b"https://example.test:0443".as_slice(),
        b"https://example.test:bad".as_slice(),
        b"https://Example.test".as_slice(),
        b"http://example.test".as_slice(),
        b"https://example.test/path".as_slice(),
        b"https://example.test.".as_slice(),
        b"https://127.0.0.01".as_slice(),
        b"https://0177.0.0.1".as_slice(),
        b"https://0x7f.0.0.1".as_slice(),
        b"https://2130706433".as_slice(),
        b"https://127.1".as_slice(),
        b"https://0x".as_slice(),
        b"https://1.2.3.4.5".as_slice(),
        b"https://example.127".as_slice(),
        b"https://[::ffff:127.0.0.1]".as_slice(),
    ] {
        assert!(
            CanonicalOrigin::try_new(invalid).is_err(),
            "非 canonical Origin 被接受: {}",
            String::from_utf8_lossy(invalid)
        );
    }
    assert!(CanonicalOrigin::try_new(b"https://example.test:8443").is_ok());
    assert!(CanonicalOrigin::try_new(b"http://127.0.0.1:8080").is_ok());
    assert!(CanonicalOrigin::try_new(b"http://[::1]:8080").is_ok());
}

#[test]
fn errors_are_stable_and_do_not_echo_canaries() {
    let request = RawIngressRequest::try_new(
        HttpMethod::Post,
        b"/CANARY_SECRET_PATH",
        vec![header(b"x-canary", b"CANARY_SECRET_HEADER")],
        b"CANARY_SECRET_BODY".to_vec(),
    )
    .expect("Canary 请求语法合法");
    let error = build_registry()
        .bind_request(&request)
        .expect_error("Canary 未注册路径必须拒绝");
    let display = error.to_string();
    let debug = format!("{error:?}");
    for canary in [
        "CANARY_SECRET_PATH",
        "CANARY_SECRET_HEADER",
        "CANARY_SECRET_BODY",
    ] {
        assert!(!display.contains(canary));
        assert!(!debug.contains(canary));
    }
    assert_eq!(display, error.code());
}

fn tlv_offsets(wire: &[u8], mut offset: usize) -> Vec<usize> {
    let mut offsets = Vec::new();
    while offset < wire.len() {
        offsets.push(offset);
        let header = &wire[offset..offset + 6];
        let length = u32::from_be_bytes(header[2..6].try_into().expect("TLV 长度字段")) as usize;
        offset += 6 + length;
    }
    offsets
}
