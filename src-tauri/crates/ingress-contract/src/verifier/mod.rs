//! 四个封闭 verifier 入口、typed listener binding 与 MAC key ring。

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use zeroize::Zeroizing;

use crate::{
    signed::{
        authorization_bundle_digest, decode_attestation, decode_authorization_bundle,
        decode_capability_authorization, verify_hmac, AuthorizationBundle, ContextActivationKey,
        EgressPurpose, ATTESTATION_MAC_DOMAIN, CAPABILITY_MAC_DOMAIN,
    },
    verified::{VerificationDomainSeal, VerifiedIngressReceiver, VerifiedIngressRequest},
    AdapterVersion, AudienceId, CapabilityManagementScopeId, ClientFamilyId, ClientVersion,
    ConsentRevision, EncodedCapabilityAuthorization, IngressReject, IngressSchemaVersion,
    IngressTokenScopeId, IssuerEpoch, ListenerId, LocalOperationAuthScope, NonceNamespace,
    OneShotNonceStore, RawIngressRequest, RequestDispatchDomain, RequestKind, RoutePolicyRevision,
    RouteSpecRegistry, SignedIngressRequest, TransformOwnerId, TransformOwnerVersion,
    MAX_ATTESTATION_TTL_MILLIS, MAX_CAPABILITY_TTL_MILLIS, MAX_CLOCK_SKEW_MILLIS,
};

const EXPECTED_SCHEMA_VERSION: u16 = 1;
const MAX_KEY_RING_ENTRIES: usize = 8;
const MAX_GATEWAY_CONSENT_TTL_MILLIS: u64 = 24 * 60 * 60 * 1_000;

/// verifier 的墙钟 Port；`IngressVerifier` 会在实例内提升为不可回拨 high-water。
pub trait VerifierClock: Send + Sync {
    /// 返回 Unix 毫秒；宿主失败时 verifier 必须 fail closed。
    fn now_millis(&self) -> Result<u64, IngressReject>;
}

/// 基于系统墙钟的生产实现。
pub struct SystemClock;

impl VerifierClock for SystemClock {
    fn now_millis(&self) -> Result<u64, IngressReject> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| IngressReject::ClockUnavailable)?;
        u64::try_from(duration.as_millis()).map_err(|_| IngressReject::ClockUnavailable)
    }
}

/// 测试、仿真和确定性宿主验证使用的固定时钟。
pub struct FixedClock {
    now_millis: AtomicU64,
}

impl FixedClock {
    /// 构造指定 Unix 毫秒值的确定性时钟。
    pub const fn new(now_millis: u64) -> Self {
        Self {
            now_millis: AtomicU64::new(now_millis),
        }
    }

    /// 设置下一次观测值；verifier 自身仍只会提升 high-water。
    pub fn set(&self, now_millis: u64) {
        self.now_millis.store(now_millis, Ordering::SeqCst);
    }
}

impl VerifierClock for FixedClock {
    fn now_millis(&self) -> Result<u64, IngressReject> {
        Ok(self.now_millis.load(Ordering::SeqCst))
    }
}

struct VerificationKeyMaterial {
    issuer_epoch: IssuerEpoch,
    key: Zeroizing<[u8; 32]>,
}

impl VerificationKeyMaterial {
    fn try_new(issuer_epoch: IssuerEpoch, key: [u8; 32]) -> Result<Self, IngressReject> {
        if issuer_epoch.get() == 0 || key == [0; 32] {
            return Err(IngressReject::InternalFailClosed);
        }
        Ok(Self {
            issuer_epoch,
            key: Zeroizing::new(key),
        })
    }
}

/// Managed Attestation 的进程内验证 key；不实现 `Clone`、`Debug` 或字节 getter。
pub struct ManagedVerificationKey(VerificationKeyMaterial);

impl ManagedVerificationKey {
    /// 构造 Managed 验证 key；全零 key/epoch 会 fail closed。
    pub fn try_new(issuer_epoch: IssuerEpoch, key: [u8; 32]) -> Result<Self, IngressReject> {
        Ok(Self(VerificationKeyMaterial::try_new(issuer_epoch, key)?))
    }
}

/// Capability Authorization 的独立验证 key；必须与 Managed key 分域或分 key。
pub struct CapabilityVerificationKey(VerificationKeyMaterial);

impl CapabilityVerificationKey {
    /// 构造 Capability 验证 key；必须与 Managed key 分域或分 key。
    pub fn try_new(issuer_epoch: IssuerEpoch, key: [u8; 32]) -> Result<Self, IngressReject> {
        Ok(Self(VerificationKeyMaterial::try_new(issuer_epoch, key)?))
    }
}

/// 支持有限轮换窗口的 Managed key ring。
pub struct ManagedVerificationKeyRing {
    keys: Vec<ManagedVerificationKey>,
}

impl ManagedVerificationKeyRing {
    /// 构造最多八个、不含重复 epoch 的 Managed 轮换窗口。
    pub fn try_new(mut keys: Vec<ManagedVerificationKey>) -> Result<Self, IngressReject> {
        validate_key_ring(keys.iter().map(|key| key.0.issuer_epoch))?;
        keys.sort_by_key(|key| key.0.issuer_epoch);
        Ok(Self { keys })
    }

    fn key_for(&self, epoch: IssuerEpoch) -> Result<&[u8], IngressReject> {
        self.keys
            .iter()
            .find(|key| key.0.issuer_epoch == epoch)
            .map(|key| key.0.key.as_slice())
            .ok_or(IngressReject::IssuerEpochUnknown)
    }
}

/// 支持有限轮换窗口的 Capability key ring。
pub struct CapabilityVerificationKeyRing {
    keys: Vec<CapabilityVerificationKey>,
}

impl CapabilityVerificationKeyRing {
    /// 构造最多八个、不含重复 epoch 的 Capability 轮换窗口。
    pub fn try_new(mut keys: Vec<CapabilityVerificationKey>) -> Result<Self, IngressReject> {
        validate_key_ring(keys.iter().map(|key| key.0.issuer_epoch))?;
        keys.sort_by_key(|key| key.0.issuer_epoch);
        Ok(Self { keys })
    }

    fn key_for(&self, epoch: IssuerEpoch) -> Result<&[u8], IngressReject> {
        self.keys
            .iter()
            .find(|key| key.0.issuer_epoch == epoch)
            .map(|key| key.0.key.as_slice())
            .ok_or(IngressReject::IssuerEpochUnknown)
    }
}

fn validate_key_ring(epochs: impl Iterator<Item = IssuerEpoch>) -> Result<(), IngressReject> {
    let epochs = epochs.collect::<Vec<_>>();
    if epochs.is_empty() || epochs.len() > MAX_KEY_RING_ENTRIES {
        return Err(IngressReject::InternalFailClosed);
    }
    let unique = epochs.iter().copied().collect::<HashSet<_>>();
    if unique.len() != epochs.len() {
        return Err(IngressReject::InternalFailClosed);
    }
    Ok(())
}

/// 仅由 composition root 持有的 listener binding capability。
///
/// 它生成互不兼容的 typed context，因此请求 Header、UA、path 或 body 无法选择 IngressMode。
pub struct ListenerBindingAuthority {
    domain_seal: Arc<VerificationDomainSeal>,
}

impl ListenerBindingAuthority {
    /// 为已完成真实 accept/token auth 的 Managed listener 生成 opaque context。
    pub fn managed(
        &self,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        audience: AudienceId,
        issuer_epoch: IssuerEpoch,
    ) -> ManagedListenerContext {
        ManagedListenerContext {
            domain_seal: Arc::clone(&self.domain_seal),
            listener,
            token_scope,
            audience,
            issuer_epoch,
        }
    }

    /// 为已完成真实 accept/token auth 的 GatewayOnly listener 生成 opaque context。
    pub fn gateway_only(
        &self,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        audience: AudienceId,
    ) -> GatewayOnlyListenerContext {
        GatewayOnlyListenerContext {
            domain_seal: Arc::clone(&self.domain_seal),
            listener,
            token_scope,
            audience,
        }
    }

    /// 为已完成本地鉴权的 Local listener 生成 opaque context。
    pub fn local(
        &self,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        auth_scope: LocalOperationAuthScope,
    ) -> LocalListenerContext {
        LocalListenerContext {
            domain_seal: Arc::clone(&self.domain_seal),
            listener,
            token_scope,
            auth_scope,
        }
    }

    /// 为已完成管理鉴权的 capability listener 生成 opaque context。
    pub fn capability(
        &self,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        audience: AudienceId,
        issuer_epoch: IssuerEpoch,
        management_scope: CapabilityManagementScopeId,
    ) -> CapabilityListenerContext {
        CapabilityListenerContext {
            domain_seal: Arc::clone(&self.domain_seal),
            listener,
            token_scope,
            audience,
            issuer_epoch,
            management_scope,
        }
    }
}

/// Managed listener accept/auth 边界产生的 typed context。
pub struct ManagedListenerContext {
    domain_seal: Arc<VerificationDomainSeal>,
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
    issuer_epoch: IssuerEpoch,
}

/// 独立 GatewayOnly listener accept/auth 边界产生的 typed context。
pub struct GatewayOnlyListenerContext {
    domain_seal: Arc<VerificationDomainSeal>,
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
}

/// 本地 Operation accept/auth 边界产生的 typed context。
pub struct LocalListenerContext {
    domain_seal: Arc<VerificationDomainSeal>,
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    auth_scope: LocalOperationAuthScope,
}

/// 管理能力探测 accept/auth 边界产生的 typed context。
pub struct CapabilityListenerContext {
    domain_seal: Arc<VerificationDomainSeal>,
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
    issuer_epoch: IssuerEpoch,
    management_scope: CapabilityManagementScopeId,
}

/// 激活快照中必须与 authorization bundle 全等的版本键。
#[derive(Clone, Eq, PartialEq)]
pub struct ContextActivationBinding {
    client_family: ClientFamilyId,
    client_version: ClientVersion,
    adapter_version: AdapterVersion,
    ingress_schema_version: IngressSchemaVersion,
    context_policy_version: crate::ContextPolicyVersion,
    transform_owner: TransformOwnerId,
    transform_owner_version: TransformOwnerVersion,
}

impl ContextActivationBinding {
    #[allow(clippy::too_many_arguments)]
    /// 构造必须与 authorization bundle 全等的激活版本键。
    pub const fn new(
        client_family: ClientFamilyId,
        client_version: ClientVersion,
        adapter_version: AdapterVersion,
        ingress_schema_version: IngressSchemaVersion,
        context_policy_version: crate::ContextPolicyVersion,
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

    fn matches(&self, key: &ContextActivationKey) -> bool {
        self.client_family == key.client_family
            && self.client_version == key.client_version
            && self.adapter_version == key.adapter_version
            && self.ingress_schema_version == key.ingress_schema_version
            && self.context_policy_version == key.context_policy_version
            && self.transform_owner == key.transform_owner
            && self.transform_owner_version == key.transform_owner_version
    }
}

/// 当前 Managed Execute 的逐 listener/token 激活快照。
pub struct ManagedActivationBinding {
    domain_seal: Arc<VerificationDomainSeal>,
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
    issuer_epoch: IssuerEpoch,
    route_policy_revision: RoutePolicyRevision,
    consent_revision: ConsentRevision,
    activation_key: ContextActivationBinding,
    expires_at_millis: u64,
}

/// 只交给可信 ActivationStore 的实例绑定签发 capability。
pub struct ManagedActivationAuthority {
    domain_seal: Arc<VerificationDomainSeal>,
}

impl ManagedActivationAuthority {
    /// 从可信激活快照签发不可由请求 primitive 构造的 Managed binding。
    #[allow(clippy::too_many_arguments)]
    pub fn issue_binding(
        &self,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        audience: AudienceId,
        issuer_epoch: IssuerEpoch,
        route_policy_revision: RoutePolicyRevision,
        consent_revision: ConsentRevision,
        activation_key: ContextActivationBinding,
        expires_at_millis: u64,
    ) -> ManagedActivationBinding {
        ManagedActivationBinding {
            domain_seal: Arc::clone(&self.domain_seal),
            listener,
            token_scope,
            audience,
            issuer_epoch,
            route_policy_revision,
            consent_revision,
            activation_key,
            expires_at_millis,
        }
    }
}

/// 用户本机明确开启的 GatewayOnly consent 快照。
pub struct GatewayOnlyConsentSnapshot {
    domain_seal: Arc<VerificationDomainSeal>,
    listener: ListenerId,
    token_scope: IngressTokenScopeId,
    audience: AudienceId,
    route_policy_revision: RoutePolicyRevision,
    consent_revision: ConsentRevision,
    maximum_trust_tier: u8,
    issued_at_millis: u64,
    expires_at_millis: u64,
}

/// 只交给可信 ConsentStore 的实例绑定签发 capability。
pub struct GatewayConsentAuthority {
    domain_seal: Arc<VerificationDomainSeal>,
}

impl GatewayConsentAuthority {
    /// 从可信本机 consent 记录签发不可由请求 primitive 构造的快照。
    #[allow(clippy::too_many_arguments)]
    pub fn issue_snapshot(
        &self,
        listener: ListenerId,
        token_scope: IngressTokenScopeId,
        audience: AudienceId,
        route_policy_revision: RoutePolicyRevision,
        consent_revision: ConsentRevision,
        maximum_trust_tier: u8,
        issued_at_millis: u64,
        expires_at_millis: u64,
    ) -> Result<GatewayOnlyConsentSnapshot, IngressReject> {
        if maximum_trust_tier > 2 {
            return Err(IngressReject::GatewayConsentInvalid);
        }
        Ok(GatewayOnlyConsentSnapshot {
            domain_seal: Arc::clone(&self.domain_seal),
            listener,
            token_scope,
            audience,
            route_policy_revision,
            consent_revision,
            maximum_trust_tier,
            issued_at_millis,
            expires_at_millis,
        })
    }
}

/// 四个入口共用的纯验证内核。
pub struct IngressVerifier {
    domain_seal: Arc<VerificationDomainSeal>,
    registry: RouteSpecRegistry,
    managed_keys: ManagedVerificationKeyRing,
    capability_keys: CapabilityVerificationKeyRing,
    clock: Arc<dyn VerifierClock>,
    clock_high_water_millis: AtomicU64,
    nonce_store: Arc<dyn OneShotNonceStore>,
}

/// composition root 一次性拆分出的验证运行域。
pub struct IngressVerifierRuntime {
    verifier: IngressVerifier,
    listener_authority: ListenerBindingAuthority,
    gateway_consent_authority: GatewayConsentAuthority,
    managed_activation_authority: ManagedActivationAuthority,
    receiver: VerifiedIngressReceiver,
}

impl IngressVerifierRuntime {
    /// 一次性拆分所有 capability；调用方不得把 authority 传给请求 handler。
    pub fn into_parts(
        self,
    ) -> (
        IngressVerifier,
        ListenerBindingAuthority,
        GatewayConsentAuthority,
        ManagedActivationAuthority,
        VerifiedIngressReceiver,
    ) {
        (
            self.verifier,
            self.listener_authority,
            self.gateway_consent_authority,
            self.managed_activation_authority,
            self.receiver,
        )
    }
}

impl IngressVerifier {
    /// 创建实例绑定的 verifier/authority/receiver 运行域；不创建网络、数据库或发送能力。
    pub fn initialize(
        registry: RouteSpecRegistry,
        managed_keys: ManagedVerificationKeyRing,
        capability_keys: CapabilityVerificationKeyRing,
        clock: Arc<dyn VerifierClock>,
        nonce_store: Arc<dyn OneShotNonceStore>,
    ) -> IngressVerifierRuntime {
        let domain_seal = Arc::new(VerificationDomainSeal::new());
        let verifier = Self {
            domain_seal: Arc::clone(&domain_seal),
            registry,
            managed_keys,
            capability_keys,
            clock,
            clock_high_water_millis: AtomicU64::new(0),
            nonce_store,
        };
        IngressVerifierRuntime {
            verifier,
            listener_authority: ListenerBindingAuthority {
                domain_seal: Arc::clone(&domain_seal),
            },
            gateway_consent_authority: GatewayConsentAuthority {
                domain_seal: Arc::clone(&domain_seal),
            },
            managed_activation_authority: ManagedActivationAuthority {
                domain_seal: Arc::clone(&domain_seal),
            },
            receiver: VerifiedIngressReceiver::new(domain_seal),
        }
    }

    /// 返回 verifier 使用的固定注册表摘要。
    pub const fn registry_digest(&self) -> crate::RegistryDigest {
        self.registry.digest()
    }

    /// 验证受信 ContextPipeline 的 Managed 请求。
    pub fn verify_managed(
        &self,
        signed: SignedIngressRequest,
        connection: &ManagedListenerContext,
        active: &ManagedActivationBinding,
    ) -> Result<VerifiedIngressRequest, IngressReject> {
        self.ensure_domain(&connection.domain_seal)?;
        self.ensure_domain(&active.domain_seal)?;
        let now = self.now()?;
        let bundle_digest = authorization_bundle_digest(&signed.encoded_authorization_bundle);
        let attestation = decode_attestation(&signed.encoded_attestation)?;
        let key = self.managed_keys.key_for(attestation.claims.issuer_epoch)?;
        verify_hmac(
            key,
            ATTESTATION_MAC_DOMAIN,
            &attestation.claims_wire,
            Some(bundle_digest.as_bytes()),
            &attestation.tag,
        )?;

        validate_managed_scope(connection, active, &attestation.claims, now)?;
        if attestation.claims.authorization_bundle_digest != bundle_digest {
            return Err(IngressReject::AuthorizationBindingMismatch);
        }

        let bundle = decode_authorization_bundle(&signed.encoded_authorization_bundle)?;
        let binding = self.registry.bind_request(&signed.request)?;
        validate_remote_data_operation(
            binding.operation.spec.kind,
            binding.operation.spec.dispatch_domain,
        )?;
        validate_managed_request_binding(
            &binding,
            &signed.request,
            self.registry.digest(),
            &attestation.claims,
        )?;
        validate_managed_authorization(&binding, &bundle, &attestation.claims, active, now)?;

        self.consume_nonce(
            NonceNamespace::ManagedAttestation,
            attestation.claims.issuer_epoch,
            attestation.claims.nonce,
            attestation.claims.expires_at_millis,
            now,
        )?;

        let sanitized_request = signed
            .request
            .into_sanitized(&binding.operation.spec.semantic_headers);
        Ok(VerifiedIngressRequest::from_managed(
            Arc::clone(&self.domain_seal),
            binding.operation,
            sanitized_request,
            attestation.claims.issuer_epoch,
            attestation.claims.nonce,
            bundle_digest,
            bundle,
        ))
    }

    /// 验证用户显式启用的独立 GatewayOnly listener 请求。
    pub fn verify_gateway_only(
        &self,
        request: RawIngressRequest,
        connection: &GatewayOnlyListenerContext,
        consent: &GatewayOnlyConsentSnapshot,
    ) -> Result<VerifiedIngressRequest, IngressReject> {
        self.ensure_domain(&connection.domain_seal)?;
        self.ensure_domain(&consent.domain_seal)?;
        let now = self.now()?;
        let binding = self.registry.bind_request(&request)?;
        validate_remote_data_operation(
            binding.operation.spec.kind,
            binding.operation.spec.dispatch_domain,
        )?;
        validate_gateway_consent(connection, consent, now)?;

        let sanitized_request = request.into_sanitized(&binding.operation.spec.semantic_headers);
        Ok(VerifiedIngressRequest::from_gateway_only(
            Arc::clone(&self.domain_seal),
            binding.operation,
            sanitized_request,
            connection.listener,
            connection.token_scope,
            consent.consent_revision,
            consent.route_policy_revision,
            self.registry.digest(),
            binding.request_digest,
            consent.maximum_trust_tier,
        ))
    }

    /// 验证 Local/Liveness/AuthFlow 请求；成功结果没有任何 egress authority。
    pub fn verify_local_operation(
        &self,
        request: RawIngressRequest,
        connection: &LocalListenerContext,
    ) -> Result<VerifiedIngressRequest, IngressReject> {
        self.ensure_domain(&connection.domain_seal)?;
        let binding = self.registry.bind_request(&request)?;
        if binding.operation.spec.dispatch_domain != RequestDispatchDomain::Local {
            return Err(IngressReject::DispatchDomainMismatch);
        }
        if binding.operation.spec.local_scope != Some(connection.auth_scope) {
            return Err(IngressReject::LocalScopeMismatch);
        }

        let sanitized_request = request.into_sanitized(&binding.operation.spec.semantic_headers);
        Ok(VerifiedIngressRequest::from_local(
            Arc::clone(&self.domain_seal),
            binding.operation,
            sanitized_request,
            connection.listener,
            connection.token_scope,
            connection.auth_scope,
            binding.request_digest,
        ))
    }

    /// 验证不携带用户正文、固定单部署且禁止 fallback 的管理能力探测。
    pub fn verify_capability_probe(
        &self,
        request: RawIngressRequest,
        connection: &CapabilityListenerContext,
        authorization: EncodedCapabilityAuthorization,
    ) -> Result<VerifiedIngressRequest, IngressReject> {
        self.ensure_domain(&connection.domain_seal)?;
        let now = self.now()?;
        let binding = self.registry.bind_request(&request)?;
        if binding.operation.spec.kind != RequestKind::DeploymentModelProbe
            || binding.operation.spec.dispatch_domain != RequestDispatchDomain::BoundDeployment
            || !request.body().is_empty()
        {
            return Err(IngressReject::CapabilityConstraintMismatch);
        }

        let signed = decode_capability_authorization(authorization.as_bytes())?;
        let key = self.capability_keys.key_for(signed.claims.issuer_epoch)?;
        verify_hmac(
            key,
            CAPABILITY_MAC_DOMAIN,
            &signed.claims_wire,
            None,
            &signed.tag,
        )?;
        validate_capability_claims(
            connection,
            &binding,
            self.registry.digest(),
            &signed.claims,
            now,
        )?;

        self.consume_nonce(
            NonceNamespace::CapabilityAuthorization,
            signed.claims.issuer_epoch,
            signed.claims.nonce,
            signed.claims.deadline_millis,
            now,
        )?;

        let sanitized_request = request.into_sanitized(&binding.operation.spec.semantic_headers);
        Ok(VerifiedIngressRequest::from_capability(
            Arc::clone(&self.domain_seal),
            binding.operation,
            sanitized_request,
            signed.claims,
        ))
    }

    fn now(&self) -> Result<u64, IngressReject> {
        let observed = self
            .clock
            .now_millis()
            .map_err(|_| IngressReject::ClockUnavailable)?;
        let previous = self
            .clock_high_water_millis
            .fetch_max(observed, Ordering::SeqCst);
        Ok(previous.max(observed))
    }

    fn ensure_domain(
        &self,
        domain_seal: &Arc<VerificationDomainSeal>,
    ) -> Result<(), IngressReject> {
        if Arc::ptr_eq(&self.domain_seal, domain_seal) {
            Ok(())
        } else {
            Err(IngressReject::VerificationDomainMismatch)
        }
    }

    fn consume_nonce(
        &self,
        namespace: NonceNamespace,
        issuer_epoch: IssuerEpoch,
        nonce: crate::OneShotNonce,
        expires_at_millis: u64,
        now_millis: u64,
    ) -> Result<(), IngressReject> {
        self.nonce_store
            .consume(
                namespace,
                issuer_epoch,
                nonce,
                expires_at_millis,
                now_millis,
            )
            .map_err(|_| IngressReject::NonceRejected)
    }
}

fn validate_remote_data_operation(
    kind: RequestKind,
    domain: RequestDispatchDomain,
) -> Result<(), IngressReject> {
    let allowed = matches!(
        (kind, domain),
        (
            RequestKind::ModelInference | RequestKind::AuxiliaryInference,
            RequestDispatchDomain::RoutedPolicy
        ) | (
            RequestKind::ExactUpstreamTokenCount,
            RequestDispatchDomain::BoundDeployment
        )
    );
    if allowed {
        Ok(())
    } else {
        Err(IngressReject::ProofModeConflict)
    }
}

fn validate_managed_scope(
    connection: &ManagedListenerContext,
    active: &ManagedActivationBinding,
    claims: &crate::signed::AttestationClaims,
    now: u64,
) -> Result<(), IngressReject> {
    if claims.schema_version != EXPECTED_SCHEMA_VERSION {
        return Err(IngressReject::ActivationBindingMismatch);
    }
    if connection.listener != active.listener
        || connection.token_scope != active.token_scope
        || claims.listener != connection.listener
        || claims.token_scope != connection.token_scope
    {
        return Err(IngressReject::ScopeMismatch);
    }
    if connection.audience != active.audience || claims.audience != connection.audience {
        return Err(IngressReject::AudienceMismatch);
    }
    if connection.issuer_epoch != active.issuer_epoch
        || claims.issuer_epoch != connection.issuer_epoch
    {
        return Err(IngressReject::IssuerEpochUnknown);
    }
    if active.expires_at_millis <= now {
        return Err(IngressReject::ActivationBindingMismatch);
    }
    validate_time_window(
        claims.issued_at_millis,
        claims.expires_at_millis,
        now,
        MAX_ATTESTATION_TTL_MILLIS,
    )
}

fn validate_managed_request_binding(
    binding: &crate::operation::RequestBinding,
    request: &RawIngressRequest,
    registry_digest: crate::RegistryDigest,
    claims: &crate::signed::AttestationClaims,
) -> Result<(), IngressReject> {
    if claims.operation != binding.operation.spec.operation {
        return Err(IngressReject::OperationMismatch);
    }
    if claims.dispatch_domain != binding.operation.spec.dispatch_domain {
        return Err(IngressReject::DispatchDomainMismatch);
    }
    if claims.registry_digest != registry_digest {
        return Err(IngressReject::RegistryMismatch);
    }
    if claims.method != request.method()
        || claims.target.as_slice() != request.target()
        || claims.semantic_headers_digest != binding.semantic_headers_digest
        || claims.body_digest != binding.body_digest
        || claims.body_length != binding.body_length
        || claims.request_digest != binding.request_digest
    {
        return Err(IngressReject::RequestBindingMismatch);
    }
    Ok(())
}

fn validate_managed_authorization(
    binding: &crate::operation::RequestBinding,
    bundle: &AuthorizationBundle,
    claims: &crate::signed::AttestationClaims,
    active: &ManagedActivationBinding,
    now: u64,
) -> Result<(), IngressReject> {
    let permit = &bundle.permit;
    if permit.operation != claims.operation
        || permit.request_digest != claims.request_digest
        || permit.body_digest != claims.body_digest
        || permit.envelope_digest != claims.envelope_digest
        || permit.nonce != claims.nonce
        || permit.policy_revision != claims.policy_revision
        || permit.policy_revision != active.route_policy_revision
        || permit.consent_revision != active.consent_revision
        || permit.expires_at_millis != claims.expires_at_millis
        || permit.max_outbound_bytes < binding.body_length
    {
        return Err(IngressReject::AuthorizationBindingMismatch);
    }
    if !active.activation_key.matches(&bundle.activation_key)
        || u64::from(claims.schema_version) != active.activation_key.ingress_schema_version.get()
        || claims.adapter_version != active.activation_key.adapter_version
        || claims.transform_owner_version != active.activation_key.transform_owner_version
        || bundle.requirements.client_adapter_version != active.activation_key.adapter_version
    {
        return Err(IngressReject::ActivationBindingMismatch);
    }
    let expected_purpose = match binding.operation.spec.kind {
        RequestKind::ModelInference => EgressPurpose::ModelInference,
        RequestKind::AuxiliaryInference => EgressPurpose::AuxiliaryInference,
        RequestKind::ExactUpstreamTokenCount => EgressPurpose::ExactUpstreamTokenCount,
        _ => return Err(IngressReject::ProofModeConflict),
    };
    if permit.purpose != expected_purpose {
        return Err(IngressReject::AuthorizationBindingMismatch);
    }
    if binding.operation.spec.dispatch_domain == RequestDispatchDomain::BoundDeployment
        && (permit.fallback_allowed || permit.allowed_targets.len() != 1)
    {
        return Err(IngressReject::CapabilityConstraintMismatch);
    }
    let handle_expiry = bundle.requirements.handle_earliest_expiry_millis;
    if handle_expiry != 0 && (handle_expiry <= now || handle_expiry < claims.expires_at_millis) {
        return Err(IngressReject::AuthorizationBindingMismatch);
    }
    Ok(())
}

fn validate_gateway_consent(
    connection: &GatewayOnlyListenerContext,
    consent: &GatewayOnlyConsentSnapshot,
    now: u64,
) -> Result<(), IngressReject> {
    if connection.listener != consent.listener || connection.token_scope != consent.token_scope {
        return Err(IngressReject::ScopeMismatch);
    }
    if connection.audience != consent.audience {
        return Err(IngressReject::AudienceMismatch);
    }
    validate_time_window(
        consent.issued_at_millis,
        consent.expires_at_millis,
        now,
        MAX_GATEWAY_CONSENT_TTL_MILLIS,
    )
    .map_err(|_| IngressReject::GatewayConsentInvalid)
}

fn validate_capability_claims(
    connection: &CapabilityListenerContext,
    binding: &crate::operation::RequestBinding,
    registry_digest: crate::RegistryDigest,
    claims: &crate::signed::CapabilityClaims,
    now: u64,
) -> Result<(), IngressReject> {
    validate_time_window(
        claims.issued_at_millis,
        claims.deadline_millis,
        now,
        MAX_CAPABILITY_TTL_MILLIS,
    )?;
    if claims.schema_version != EXPECTED_SCHEMA_VERSION
        || claims.listener != connection.listener
        || claims.token_scope != connection.token_scope
        || claims.audience != connection.audience
        || claims.issuer_epoch != connection.issuer_epoch
    {
        return Err(IngressReject::ScopeMismatch);
    }
    if claims.operation != binding.operation.spec.operation {
        return Err(IngressReject::OperationMismatch);
    }
    if claims.registry_digest != registry_digest {
        return Err(IngressReject::RegistryMismatch);
    }
    if claims.dispatch_domain != RequestDispatchDomain::BoundDeployment
        || claims.dispatch_domain != binding.operation.spec.dispatch_domain
    {
        return Err(IngressReject::DispatchDomainMismatch);
    }
    if claims.management_scope != connection.management_scope
        || claims.request_digest != binding.request_digest
        || !claims.fallback_forbidden
    {
        return Err(IngressReject::CapabilityConstraintMismatch);
    }
    Ok(())
}

fn validate_time_window(
    issued_at_millis: u64,
    expires_at_millis: u64,
    now: u64,
    max_ttl_millis: u64,
) -> Result<(), IngressReject> {
    if expires_at_millis <= issued_at_millis
        || expires_at_millis <= now
        || issued_at_millis > now.saturating_add(MAX_CLOCK_SKEW_MILLIS)
        || expires_at_millis.saturating_sub(issued_at_millis) > max_ttl_millis
    {
        return Err(IngressReject::TimeWindowInvalid);
    }
    Ok(())
}
