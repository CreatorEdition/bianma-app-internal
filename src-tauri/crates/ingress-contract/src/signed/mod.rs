//! 不可信 signed wire schema 与严格 canonical decoder。

mod canonical;

use hmac::{Hmac, Mac};
use sha2::{Digest as _, Sha256};

use crate::{
    AccountId, AccountSelectorId, AdapterContractRevision, AdapterVersion, AudienceId,
    AuthorizationBundleDigest, BodyDigest, CapabilityManagementScopeId, ClientFamilyId,
    ClientVersion, ConsentRevision, ContextPolicyVersion, CredentialId, EndpointId, EnvelopeDigest,
    HandleEpoch, HttpMethod, IngressReject, IngressSchemaVersion, IngressTokenScopeId, IssuerEpoch,
    ListenerId, ModelDeploymentId, OneShotNonce, OperationId, ProtocolFrameRevision,
    RawIngressRequest, RegistryDigest, RequestDigest, RequestDispatchDomain,
    RetrievalSchemaRevision, RoutePolicyRevision, SiteId, ToolSchemaRevision, TransformOwnerId,
    TransformOwnerVersion, MAX_ALLOWED_EGRESS_TARGETS, MAX_ATTESTATION_BYTES,
    MAX_AUTHORIZATION_BUNDLE_BYTES, MAX_CANONICAL_ORIGIN_BYTES, MAX_CAPABILITY_AUTHORIZATION_BYTES,
    MAX_REQUEST_TARGET_BYTES,
};

#[cfg(test)]
use canonical::Encoder;
use canonical::{bool_from_byte, fixed, Decoder};

pub(crate) const AUTHORIZATION_BUNDLE_PREFIX: &[u8] = b"BIANMA/AUTHORIZATION-BUNDLE/1\0";
pub(crate) const EGRESS_PERMIT_PREFIX: &[u8] = b"BIANMA/EGRESS-PERMIT/1\0";
pub(crate) const CAPABILITY_REQUIREMENTS_PREFIX: &[u8] = b"BIANMA/CAPABILITY-REQUIREMENTS/1\0";
pub(crate) const ACTIVATION_KEY_PREFIX: &[u8] = b"BIANMA/ACTIVATION-KEY/1\0";
pub(crate) const ATTESTATION_CLAIMS_PREFIX: &[u8] = b"BIANMA/ATTESTATION-CLAIMS/1\0";
pub(crate) const ATTESTATION_PREFIX: &[u8] = b"BIANMA/ATTESTATION/1\0";
pub(crate) const CAPABILITY_CLAIMS_PREFIX: &[u8] = b"BIANMA/CAPABILITY-CLAIMS/1\0";
pub(crate) const CAPABILITY_AUTHORIZATION_PREFIX: &[u8] = b"BIANMA/CAPABILITY-AUTHORIZATION/1\0";
const EGRESS_TARGET_PREFIX: &[u8] = b"BIANMA/EGRESS-TARGET/1\0";

pub(crate) const BUNDLE_DIGEST_DOMAIN: &[u8] = b"bianma.ingress.authorization-bundle-digest.v1\0";
pub(crate) const ATTESTATION_MAC_DOMAIN: &[u8] = b"bianma.ingress.context-attestation-mac.v1\0";
pub(crate) const CAPABILITY_MAC_DOMAIN: &[u8] = b"bianma.ingress.capability-authorization-mac.v1\0";

/// 经过严格格式校验的 canonical Origin。
///
/// Origin 可能揭示用户站点，因此本类型不实现 `Debug` 或 `Display`。
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct CanonicalOrigin(Vec<u8>);

impl CanonicalOrigin {
    /// 构造精确 Origin。HTTPS 可使用公开主机；HTTP 仅允许 loopback。
    pub fn try_new(origin: &[u8]) -> Result<Self, IngressReject> {
        validate_canonical_origin(origin)?;
        Ok(Self(origin.to_vec()))
    }

    /// 返回 canonical Origin 字节。
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Managed signed request 的不可信 wire 容器。
///
/// 证明与 bundle 必须从原始 wire 提取后单独传入；同名 Header 混入原始请求会被拒绝。
pub struct SignedIngressRequest {
    pub(crate) request: RawIngressRequest,
    pub(crate) encoded_attestation: Vec<u8>,
    pub(crate) encoded_authorization_bundle: Vec<u8>,
}

impl SignedIngressRequest {
    /// 构造有界 wire 容器；密码学与语义验证只发生在 Managed verifier 内。
    pub fn try_new(
        request: RawIngressRequest,
        encoded_attestation: Vec<u8>,
        encoded_authorization_bundle: Vec<u8>,
    ) -> Result<Self, IngressReject> {
        if encoded_attestation.is_empty()
            || encoded_attestation.len() > MAX_ATTESTATION_BYTES
            || encoded_authorization_bundle.is_empty()
            || encoded_authorization_bundle.len() > MAX_AUTHORIZATION_BUNDLE_BYTES
        {
            return Err(IngressReject::RequestTooLarge);
        }
        Ok(Self {
            request,
            encoded_attestation,
            encoded_authorization_bundle,
        })
    }
}

/// CapabilityScoped 独立授权的有界、不可信 wire 容器。
pub struct EncodedCapabilityAuthorization(Vec<u8>);

impl EncodedCapabilityAuthorization {
    /// 构造有界 wire；MAC 与字段绑定只在 capability verifier 内验证。
    pub fn try_new(bytes: Vec<u8>) -> Result<Self, IngressReject> {
        if bytes.is_empty() || bytes.len() > MAX_CAPABILITY_AUTHORIZATION_BYTES {
            return Err(IngressReject::RequestTooLarge);
        }
        Ok(Self(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum EgressPurpose {
    ModelInference,
    AuxiliaryInference,
    ExactUpstreamTokenCount,
}

impl EgressPurpose {
    #[cfg(test)]
    fn code(self) -> u8 {
        match self {
            Self::ModelInference => 1,
            Self::AuxiliaryInference => 2,
            Self::ExactUpstreamTokenCount => 3,
        }
    }

    fn from_code(code: u8) -> Result<Self, IngressReject> {
        match code {
            1 => Ok(Self::ModelInference),
            2 => Ok(Self::AuxiliaryInference),
            3 => Ok(Self::ExactUpstreamTokenCount),
            _ => Err(IngressReject::AuthorizationBundleMalformed),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum SensitivityClass {
    Public,
    Internal,
    PrivateCode,
}

impl SensitivityClass {
    #[cfg(test)]
    fn code(self) -> u8 {
        match self {
            Self::Public => 1,
            Self::Internal => 2,
            Self::PrivateCode => 3,
        }
    }

    fn from_code(code: u8) -> Result<Self, IngressReject> {
        match code {
            1 => Ok(Self::Public),
            2 => Ok(Self::Internal),
            3 => Ok(Self::PrivateCode),
            _ => Err(IngressReject::AuthorizationBundleMalformed),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContinuationConstraint {
    None,
    FullHistoryPortable,
    ProviderStateful,
}

impl ContinuationConstraint {
    #[cfg(test)]
    fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::FullHistoryPortable => 1,
            Self::ProviderStateful => 2,
        }
    }

    fn from_code(code: u8) -> Result<Self, IngressReject> {
        match code {
            0 => Ok(Self::None),
            1 => Ok(Self::FullHistoryPortable),
            2 => Ok(Self::ProviderStateful),
            _ => Err(IngressReject::AuthorizationBundleMalformed),
        }
    }
}

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct AllowedEgressTarget {
    pub(crate) site: SiteId,
    pub(crate) deployment: ModelDeploymentId,
    pub(crate) origin: CanonicalOrigin,
    pub(crate) trust_tier: u8,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ContextEgressPermit {
    pub(crate) operation: OperationId,
    pub(crate) request_digest: RequestDigest,
    pub(crate) body_digest: BodyDigest,
    pub(crate) envelope_digest: EnvelopeDigest,
    pub(crate) nonce: OneShotNonce,
    pub(crate) purpose: EgressPurpose,
    pub(crate) sensitivity: SensitivityClass,
    pub(crate) max_outbound_bytes: u64,
    pub(crate) allowed_targets: Vec<AllowedEgressTarget>,
    pub(crate) fallback_allowed: bool,
    pub(crate) policy_revision: RoutePolicyRevision,
    pub(crate) consent_revision: ConsentRevision,
    pub(crate) expires_at_millis: u64,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ContextCapabilityRequirements {
    pub(crate) tool_schema_revision: ToolSchemaRevision,
    pub(crate) retrieval_schema_revision: RetrievalSchemaRevision,
    pub(crate) client_adapter_version: AdapterVersion,
    pub(crate) upstream_adapter_revision: AdapterContractRevision,
    pub(crate) handle_epoch: HandleEpoch,
    pub(crate) handle_earliest_expiry_millis: u64,
    pub(crate) protocol_frame_revision: ProtocolFrameRevision,
    pub(crate) continuation: ContinuationConstraint,
    pub(crate) local_handle_required: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ContextActivationKey {
    pub(crate) client_family: ClientFamilyId,
    pub(crate) client_version: ClientVersion,
    pub(crate) adapter_version: AdapterVersion,
    pub(crate) ingress_schema_version: IngressSchemaVersion,
    pub(crate) context_policy_version: ContextPolicyVersion,
    pub(crate) transform_owner: TransformOwnerId,
    pub(crate) transform_owner_version: TransformOwnerVersion,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct AuthorizationBundle {
    pub(crate) permit: ContextEgressPermit,
    pub(crate) requirements: ContextCapabilityRequirements,
    pub(crate) activation_key: ContextActivationKey,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct AttestationClaims {
    pub(crate) schema_version: u16,
    pub(crate) audience: AudienceId,
    pub(crate) issuer_epoch: IssuerEpoch,
    pub(crate) issued_at_millis: u64,
    pub(crate) expires_at_millis: u64,
    pub(crate) listener: ListenerId,
    pub(crate) token_scope: IngressTokenScopeId,
    pub(crate) operation: OperationId,
    pub(crate) dispatch_domain: RequestDispatchDomain,
    pub(crate) registry_digest: RegistryDigest,
    pub(crate) method: HttpMethod,
    pub(crate) target: Vec<u8>,
    pub(crate) semantic_headers_digest: crate::SemanticHeadersDigest,
    pub(crate) body_digest: BodyDigest,
    pub(crate) body_length: u64,
    pub(crate) request_digest: RequestDigest,
    pub(crate) envelope_digest: EnvelopeDigest,
    pub(crate) authorization_bundle_digest: AuthorizationBundleDigest,
    pub(crate) nonce: OneShotNonce,
    pub(crate) policy_revision: RoutePolicyRevision,
    pub(crate) adapter_version: AdapterVersion,
    pub(crate) transform_owner_version: TransformOwnerVersion,
}

pub(crate) struct SignedAttestation {
    pub(crate) claims: AttestationClaims,
    pub(crate) claims_wire: Vec<u8>,
    pub(crate) tag: [u8; 32],
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CapabilityClaims {
    pub(crate) schema_version: u16,
    pub(crate) audience: AudienceId,
    pub(crate) issuer_epoch: IssuerEpoch,
    pub(crate) issued_at_millis: u64,
    pub(crate) deadline_millis: u64,
    pub(crate) listener: ListenerId,
    pub(crate) token_scope: IngressTokenScopeId,
    pub(crate) operation: OperationId,
    pub(crate) dispatch_domain: RequestDispatchDomain,
    pub(crate) registry_digest: RegistryDigest,
    pub(crate) site: SiteId,
    pub(crate) deployment: ModelDeploymentId,
    pub(crate) endpoint: EndpointId,
    pub(crate) origin: CanonicalOrigin,
    pub(crate) account_selector: AccountSelectorId,
    pub(crate) account: AccountId,
    pub(crate) credential: CredentialId,
    pub(crate) adapter_contract_revision: AdapterContractRevision,
    pub(crate) trust_tier: u8,
    pub(crate) management_scope: CapabilityManagementScopeId,
    pub(crate) request_digest: RequestDigest,
    pub(crate) nonce: OneShotNonce,
    pub(crate) fallback_forbidden: bool,
}

pub(crate) struct SignedCapabilityAuthorization {
    pub(crate) claims: CapabilityClaims,
    pub(crate) claims_wire: Vec<u8>,
    pub(crate) tag: [u8; 32],
}

pub(crate) fn decode_authorization_bundle(
    wire: &[u8],
) -> Result<AuthorizationBundle, IngressReject> {
    let error = IngressReject::AuthorizationBundleMalformed;
    let mut decoder = Decoder::with_prefix(wire, AUTHORIZATION_BUNDLE_PREFIX, error)?;
    let permit_wire = decoder.field(1, MAX_AUTHORIZATION_BUNDLE_BYTES)?;
    let requirements_wire = decoder.field(2, MAX_AUTHORIZATION_BUNDLE_BYTES)?;
    let activation_wire = decoder.field(3, MAX_AUTHORIZATION_BUNDLE_BYTES)?;
    decoder.finish()?;
    Ok(AuthorizationBundle {
        permit: decode_egress_permit(permit_wire)?,
        requirements: decode_capability_requirements(requirements_wire)?,
        activation_key: decode_activation_key(activation_wire)?,
    })
}

pub(crate) fn decode_attestation(wire: &[u8]) -> Result<SignedAttestation, IngressReject> {
    let error = IngressReject::ProofMalformed;
    let mut decoder = Decoder::with_prefix(wire, ATTESTATION_PREFIX, error)?;
    let claims_wire = decoder.field(1, MAX_ATTESTATION_BYTES)?.to_vec();
    let tag = fixed::<32>(decoder.field(2, 32)?, error)?;
    decoder.finish()?;
    let claims = decode_attestation_claims(&claims_wire)?;
    Ok(SignedAttestation {
        claims,
        claims_wire,
        tag,
    })
}

pub(crate) fn decode_capability_authorization(
    wire: &[u8],
) -> Result<SignedCapabilityAuthorization, IngressReject> {
    let error = IngressReject::CapabilityAuthorizationMalformed;
    let mut decoder = Decoder::with_prefix(wire, CAPABILITY_AUTHORIZATION_PREFIX, error)?;
    let claims_wire = decoder
        .field(1, MAX_CAPABILITY_AUTHORIZATION_BYTES)?
        .to_vec();
    let tag = fixed::<32>(decoder.field(2, 32)?, error)?;
    decoder.finish()?;
    let claims = decode_capability_claims(&claims_wire)?;
    Ok(SignedCapabilityAuthorization {
        claims,
        claims_wire,
        tag,
    })
}

pub(crate) fn authorization_bundle_digest(wire: &[u8]) -> AuthorizationBundleDigest {
    let mut hasher = Sha256::new();
    hasher.update(BUNDLE_DIGEST_DOMAIN);
    hasher.update((wire.len() as u64).to_be_bytes());
    hasher.update(wire);
    AuthorizationBundleDigest::from_bytes(hasher.finalize().into())
}

pub(crate) fn verify_hmac(
    key: &[u8],
    domain: &[u8],
    claims_wire: &[u8],
    bound_digest: Option<&[u8; 32]>,
    tag: &[u8; 32],
) -> Result<(), IngressReject> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key).map_err(|_| IngressReject::InternalFailClosed)?;
    mac.update(domain);
    mac.update(&(claims_wire.len() as u64).to_be_bytes());
    mac.update(claims_wire);
    if let Some(digest) = bound_digest {
        mac.update(digest);
    }
    mac.verify_slice(tag).map_err(|_| IngressReject::MacInvalid)
}

fn decode_egress_permit(wire: &[u8]) -> Result<ContextEgressPermit, IngressReject> {
    let error = IngressReject::AuthorizationBundleMalformed;
    let mut decoder = Decoder::with_prefix(wire, EGRESS_PERMIT_PREFIX, error)?;
    let operation = OperationId::new(decoder.field_u64(1)?);
    let request_digest = RequestDigest::from_bytes(fixed(decoder.field(2, 32)?, error)?);
    let body_digest = BodyDigest::from_bytes(fixed(decoder.field(3, 32)?, error)?);
    let envelope_digest = EnvelopeDigest::from_bytes(fixed(decoder.field(4, 32)?, error)?);
    let nonce =
        OneShotNonce::from_bytes(fixed(decoder.field(5, 16)?, error)?).map_err(|_| error)?;
    let purpose = EgressPurpose::from_code(decoder.field_u8(6)?)?;
    let sensitivity = SensitivityClass::from_code(decoder.field_u8(7)?)?;
    let max_outbound_bytes = decoder.field_u64(8)?;
    let allowed_targets = decode_target_list(decoder.field(9, MAX_AUTHORIZATION_BUNDLE_BYTES)?)?;
    let fallback_allowed = bool_from_byte(decoder.field_u8(10)?, error)?;
    let policy_revision = RoutePolicyRevision::new(decoder.field_u64(11)?);
    let consent_revision = ConsentRevision::new(decoder.field_u64(12)?);
    let expires_at_millis = decoder.field_u64(13)?;
    decoder.finish()?;

    if max_outbound_bytes == 0 || allowed_targets.is_empty() {
        return Err(error);
    }
    Ok(ContextEgressPermit {
        operation,
        request_digest,
        body_digest,
        envelope_digest,
        nonce,
        purpose,
        sensitivity,
        max_outbound_bytes,
        allowed_targets,
        fallback_allowed,
        policy_revision,
        consent_revision,
        expires_at_millis,
    })
}

fn decode_capability_requirements(
    wire: &[u8],
) -> Result<ContextCapabilityRequirements, IngressReject> {
    let error = IngressReject::AuthorizationBundleMalformed;
    let mut decoder = Decoder::with_prefix(wire, CAPABILITY_REQUIREMENTS_PREFIX, error)?;
    let requirements = ContextCapabilityRequirements {
        tool_schema_revision: ToolSchemaRevision::new(decoder.field_u64(1)?),
        retrieval_schema_revision: RetrievalSchemaRevision::new(decoder.field_u64(2)?),
        client_adapter_version: AdapterVersion::new(decoder.field_u64(3)?),
        upstream_adapter_revision: AdapterContractRevision::new(decoder.field_u64(4)?),
        handle_epoch: HandleEpoch::new(decoder.field_u64(5)?),
        handle_earliest_expiry_millis: decoder.field_u64(6)?,
        protocol_frame_revision: ProtocolFrameRevision::new(decoder.field_u64(7)?),
        continuation: ContinuationConstraint::from_code(decoder.field_u8(8)?)?,
        local_handle_required: bool_from_byte(decoder.field_u8(9)?, error)?,
    };
    decoder.finish()?;
    Ok(requirements)
}

fn decode_activation_key(wire: &[u8]) -> Result<ContextActivationKey, IngressReject> {
    let error = IngressReject::AuthorizationBundleMalformed;
    let mut decoder = Decoder::with_prefix(wire, ACTIVATION_KEY_PREFIX, error)?;
    let activation_key = ContextActivationKey {
        client_family: ClientFamilyId::new(decoder.field_u64(1)?),
        client_version: ClientVersion::new(decoder.field_u64(2)?),
        adapter_version: AdapterVersion::new(decoder.field_u64(3)?),
        ingress_schema_version: IngressSchemaVersion::new(decoder.field_u64(4)?),
        context_policy_version: crate::ContextPolicyVersion::new(decoder.field_u64(5)?),
        transform_owner: TransformOwnerId::new(decoder.field_u64(6)?),
        transform_owner_version: TransformOwnerVersion::new(decoder.field_u64(7)?),
    };
    decoder.finish()?;
    Ok(activation_key)
}

fn decode_attestation_claims(wire: &[u8]) -> Result<AttestationClaims, IngressReject> {
    let error = IngressReject::ProofMalformed;
    let mut decoder = Decoder::with_prefix(wire, ATTESTATION_CLAIMS_PREFIX, error)?;
    let schema_version = decoder.field_u16(1)?;
    let audience = AudienceId::new(decoder.field_u64(2)?);
    let issuer_epoch = IssuerEpoch::new(decoder.field_u64(3)?);
    let issued_at_millis = decoder.field_u64(4)?;
    let expires_at_millis = decoder.field_u64(5)?;
    let listener = ListenerId::new(decoder.field_u64(6)?);
    let token_scope = IngressTokenScopeId::new(decoder.field_u64(7)?);
    let operation = OperationId::new(decoder.field_u64(8)?);
    let dispatch_domain = RequestDispatchDomain::from_code(decoder.field_u8(9)?)?;
    let registry_digest = RegistryDigest::from_bytes(fixed(decoder.field(10, 32)?, error)?);
    let method = HttpMethod::from_code(decoder.field_u8(11)?)?;
    let target = decoder.field(12, MAX_REQUEST_TARGET_BYTES)?.to_vec();
    let semantic_headers_digest =
        crate::SemanticHeadersDigest::from_bytes(fixed(decoder.field(13, 32)?, error)?);
    let body_digest = BodyDigest::from_bytes(fixed(decoder.field(14, 32)?, error)?);
    let body_length = decoder.field_u64(15)?;
    let request_digest = RequestDigest::from_bytes(fixed(decoder.field(16, 32)?, error)?);
    let envelope_digest = EnvelopeDigest::from_bytes(fixed(decoder.field(17, 32)?, error)?);
    let authorization_bundle_digest =
        AuthorizationBundleDigest::from_bytes(fixed(decoder.field(18, 32)?, error)?);
    let nonce =
        OneShotNonce::from_bytes(fixed(decoder.field(19, 16)?, error)?).map_err(|_| error)?;
    let policy_revision = RoutePolicyRevision::new(decoder.field_u64(20)?);
    let adapter_version = AdapterVersion::new(decoder.field_u64(21)?);
    let transform_owner_version = TransformOwnerVersion::new(decoder.field_u64(22)?);
    decoder.finish()?;
    Ok(AttestationClaims {
        schema_version,
        audience,
        issuer_epoch,
        issued_at_millis,
        expires_at_millis,
        listener,
        token_scope,
        operation,
        dispatch_domain,
        registry_digest,
        method,
        target,
        semantic_headers_digest,
        body_digest,
        body_length,
        request_digest,
        envelope_digest,
        authorization_bundle_digest,
        nonce,
        policy_revision,
        adapter_version,
        transform_owner_version,
    })
}

fn decode_capability_claims(wire: &[u8]) -> Result<CapabilityClaims, IngressReject> {
    let error = IngressReject::CapabilityAuthorizationMalformed;
    let mut decoder = Decoder::with_prefix(wire, CAPABILITY_CLAIMS_PREFIX, error)?;
    let claims = CapabilityClaims {
        schema_version: decoder.field_u16(1)?,
        audience: AudienceId::new(decoder.field_u64(2)?),
        issuer_epoch: IssuerEpoch::new(decoder.field_u64(3)?),
        issued_at_millis: decoder.field_u64(4)?,
        deadline_millis: decoder.field_u64(5)?,
        listener: ListenerId::new(decoder.field_u64(6)?),
        token_scope: IngressTokenScopeId::new(decoder.field_u64(7)?),
        operation: OperationId::new(decoder.field_u64(8)?),
        dispatch_domain: RequestDispatchDomain::from_code(decoder.field_u8(9)?)
            .map_err(|_| error)?,
        registry_digest: RegistryDigest::from_bytes(fixed(decoder.field(10, 32)?, error)?),
        deployment: ModelDeploymentId::new(decoder.field_u64(11)?),
        endpoint: EndpointId::new(decoder.field_u64(12)?),
        origin: CanonicalOrigin::try_new(decoder.field(13, MAX_CANONICAL_ORIGIN_BYTES)?)
            .map_err(|_| error)?,
        account_selector: AccountSelectorId::new(decoder.field_u64(14)?),
        account: AccountId::new(decoder.field_u64(15)?),
        credential: CredentialId::new(decoder.field_u64(16)?),
        adapter_contract_revision: AdapterContractRevision::new(decoder.field_u64(17)?),
        management_scope: CapabilityManagementScopeId::new(decoder.field_u64(18)?),
        request_digest: RequestDigest::from_bytes(fixed(decoder.field(19, 32)?, error)?),
        nonce: OneShotNonce::from_bytes(fixed(decoder.field(20, 16)?, error)?)
            .map_err(|_| error)?,
        fallback_forbidden: match decoder.field_u8(21)? {
            0 => true,
            _ => return Err(error),
        },
        site: SiteId::new(decoder.field_u64(22)?),
        trust_tier: decoder.field_u8(23)?,
    };
    if claims.trust_tier > 2 {
        return Err(error);
    }
    decoder.finish()?;
    Ok(claims)
}

fn decode_target_list(wire: &[u8]) -> Result<Vec<AllowedEgressTarget>, IngressReject> {
    let error = IngressReject::AuthorizationBundleMalformed;
    if wire.len() < 2 {
        return Err(error);
    }
    let count = u16::from_be_bytes([wire[0], wire[1]]) as usize;
    if count == 0 || count > MAX_ALLOWED_EGRESS_TARGETS {
        return Err(error);
    }
    let mut offset = 2usize;
    let mut targets = Vec::with_capacity(count);
    for _ in 0..count {
        let length_end = offset.checked_add(4).ok_or(error)?;
        let length_bytes = wire.get(offset..length_end).ok_or(error)?;
        let length = u32::from_be_bytes(length_bytes.try_into().map_err(|_| error)?) as usize;
        if length > MAX_CANONICAL_ORIGIN_BYTES + 128 {
            return Err(error);
        }
        let target_end = length_end.checked_add(length).ok_or(error)?;
        let target_wire = wire.get(length_end..target_end).ok_or(error)?;
        targets.push(decode_target(target_wire)?);
        offset = target_end;
    }
    if offset != wire.len()
        || !targets.windows(2).all(|pair| pair[0] < pair[1])
        || targets
            .windows(2)
            .any(|pair| same_logical_target(&pair[0], &pair[1]))
    {
        return Err(error);
    }
    Ok(targets)
}

fn same_logical_target(left: &AllowedEgressTarget, right: &AllowedEgressTarget) -> bool {
    left.site == right.site && left.deployment == right.deployment && left.origin == right.origin
}

fn decode_target(wire: &[u8]) -> Result<AllowedEgressTarget, IngressReject> {
    let error = IngressReject::AuthorizationBundleMalformed;
    let mut decoder = Decoder::with_prefix(wire, EGRESS_TARGET_PREFIX, error)?;
    let target = AllowedEgressTarget {
        site: SiteId::new(decoder.field_u64(1)?),
        deployment: ModelDeploymentId::new(decoder.field_u64(2)?),
        origin: CanonicalOrigin::try_new(decoder.field(3, MAX_CANONICAL_ORIGIN_BYTES)?)
            .map_err(|_| error)?,
        trust_tier: decoder.field_u8(4)?,
    };
    decoder.finish()?;
    if target.trust_tier > 2 {
        return Err(error);
    }
    Ok(target)
}

fn validate_canonical_origin(origin: &[u8]) -> Result<(), IngressReject> {
    let error = IngressReject::CapabilityConstraintMismatch;
    if origin.is_empty()
        || origin.len() > MAX_CANONICAL_ORIGIN_BYTES
        || origin
            .iter()
            .any(|byte| !byte.is_ascii() || byte.is_ascii_control() || *byte == b' ')
    {
        return Err(error);
    }
    let text = std::str::from_utf8(origin).map_err(|_| error)?;
    let parsed = url::Url::parse(text).map_err(|_| error)?;
    if !matches!(parsed.scheme(), "https" | "http")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err(error);
    }
    if parsed.origin().ascii_serialization().as_bytes() != origin {
        return Err(error);
    }
    if matches!(parsed.host(), Some(url::Host::Domain(domain)) if domain.ends_with('.')) {
        return Err(error);
    }
    if parsed.scheme() == "http" {
        let loopback = match parsed.host() {
            Some(url::Host::Domain(domain)) => domain == "localhost",
            Some(url::Host::Ipv4(address)) => address.is_loopback(),
            Some(url::Host::Ipv6(address)) => address.is_loopback(),
            None => false,
        };
        if !loopback {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
fn encode_egress_target(target: &AllowedEgressTarget) -> Vec<u8> {
    let mut encoder = Encoder::with_prefix(EGRESS_TARGET_PREFIX);
    encoder.field_u64(1, target.site.get());
    encoder.field_u64(2, target.deployment.get());
    encoder.field(3, target.origin.as_bytes());
    encoder.field_u8(4, target.trust_tier);
    encoder.into_bytes()
}

#[cfg(test)]
fn encode_target_list(targets: &[AllowedEgressTarget]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(targets.len() as u16).to_be_bytes());
    for target in targets {
        let encoded = encode_egress_target(target);
        bytes.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
        bytes.extend_from_slice(&encoded);
    }
    bytes
}

#[cfg(test)]
pub(crate) fn encode_authorization_bundle(bundle: &AuthorizationBundle) -> Vec<u8> {
    let mut permit = Encoder::with_prefix(EGRESS_PERMIT_PREFIX);
    permit.field_u64(1, bundle.permit.operation.get());
    permit.field(2, bundle.permit.request_digest.as_bytes());
    permit.field(3, bundle.permit.body_digest.as_bytes());
    permit.field(4, bundle.permit.envelope_digest.as_bytes());
    permit.field(5, bundle.permit.nonce.as_bytes());
    permit.field_u8(6, bundle.permit.purpose.code());
    permit.field_u8(7, bundle.permit.sensitivity.code());
    permit.field_u64(8, bundle.permit.max_outbound_bytes);
    permit.field(9, &encode_target_list(&bundle.permit.allowed_targets));
    permit.field_u8(10, u8::from(bundle.permit.fallback_allowed));
    permit.field_u64(11, bundle.permit.policy_revision.get());
    permit.field_u64(12, bundle.permit.consent_revision.get());
    permit.field_u64(13, bundle.permit.expires_at_millis);

    let mut requirements = Encoder::with_prefix(CAPABILITY_REQUIREMENTS_PREFIX);
    requirements.field_u64(1, bundle.requirements.tool_schema_revision.get());
    requirements.field_u64(2, bundle.requirements.retrieval_schema_revision.get());
    requirements.field_u64(3, bundle.requirements.client_adapter_version.get());
    requirements.field_u64(4, bundle.requirements.upstream_adapter_revision.get());
    requirements.field_u64(5, bundle.requirements.handle_epoch.get());
    requirements.field_u64(6, bundle.requirements.handle_earliest_expiry_millis);
    requirements.field_u64(7, bundle.requirements.protocol_frame_revision.get());
    requirements.field_u8(8, bundle.requirements.continuation.code());
    requirements.field_u8(9, u8::from(bundle.requirements.local_handle_required));

    let mut activation = Encoder::with_prefix(ACTIVATION_KEY_PREFIX);
    activation.field_u64(1, bundle.activation_key.client_family.get());
    activation.field_u64(2, bundle.activation_key.client_version.get());
    activation.field_u64(3, bundle.activation_key.adapter_version.get());
    activation.field_u64(4, bundle.activation_key.ingress_schema_version.get());
    activation.field_u64(5, bundle.activation_key.context_policy_version.get());
    activation.field_u64(6, bundle.activation_key.transform_owner.get());
    activation.field_u64(7, bundle.activation_key.transform_owner_version.get());

    let mut outer = Encoder::with_prefix(AUTHORIZATION_BUNDLE_PREFIX);
    outer.field(1, &permit.into_bytes());
    outer.field(2, &requirements.into_bytes());
    outer.field(3, &activation.into_bytes());
    outer.into_bytes()
}

#[cfg(test)]
pub(crate) fn encode_attestation_claims(claims: &AttestationClaims) -> Vec<u8> {
    let mut encoder = Encoder::with_prefix(ATTESTATION_CLAIMS_PREFIX);
    encoder.field_u16(1, claims.schema_version);
    encoder.field_u64(2, claims.audience.get());
    encoder.field_u64(3, claims.issuer_epoch.get());
    encoder.field_u64(4, claims.issued_at_millis);
    encoder.field_u64(5, claims.expires_at_millis);
    encoder.field_u64(6, claims.listener.get());
    encoder.field_u64(7, claims.token_scope.get());
    encoder.field_u64(8, claims.operation.get());
    encoder.field_u8(9, claims.dispatch_domain.code());
    encoder.field(10, claims.registry_digest.as_bytes());
    encoder.field_u8(11, claims.method.code());
    encoder.field(12, &claims.target);
    encoder.field(13, claims.semantic_headers_digest.as_bytes());
    encoder.field(14, claims.body_digest.as_bytes());
    encoder.field_u64(15, claims.body_length);
    encoder.field(16, claims.request_digest.as_bytes());
    encoder.field(17, claims.envelope_digest.as_bytes());
    encoder.field(18, claims.authorization_bundle_digest.as_bytes());
    encoder.field(19, claims.nonce.as_bytes());
    encoder.field_u64(20, claims.policy_revision.get());
    encoder.field_u64(21, claims.adapter_version.get());
    encoder.field_u64(22, claims.transform_owner_version.get());
    encoder.into_bytes()
}

#[cfg(test)]
pub(crate) fn encode_capability_claims(claims: &CapabilityClaims) -> Vec<u8> {
    let mut encoder = Encoder::with_prefix(CAPABILITY_CLAIMS_PREFIX);
    encoder.field_u16(1, claims.schema_version);
    encoder.field_u64(2, claims.audience.get());
    encoder.field_u64(3, claims.issuer_epoch.get());
    encoder.field_u64(4, claims.issued_at_millis);
    encoder.field_u64(5, claims.deadline_millis);
    encoder.field_u64(6, claims.listener.get());
    encoder.field_u64(7, claims.token_scope.get());
    encoder.field_u64(8, claims.operation.get());
    encoder.field_u8(9, claims.dispatch_domain.code());
    encoder.field(10, claims.registry_digest.as_bytes());
    encoder.field_u64(11, claims.deployment.get());
    encoder.field_u64(12, claims.endpoint.get());
    encoder.field(13, claims.origin.as_bytes());
    encoder.field_u64(14, claims.account_selector.get());
    encoder.field_u64(15, claims.account.get());
    encoder.field_u64(16, claims.credential.get());
    encoder.field_u64(17, claims.adapter_contract_revision.get());
    encoder.field_u64(18, claims.management_scope.get());
    encoder.field(19, claims.request_digest.as_bytes());
    encoder.field(20, claims.nonce.as_bytes());
    encoder.field_u8(21, if claims.fallback_forbidden { 0 } else { 1 });
    encoder.field_u64(22, claims.site.get());
    encoder.field_u8(23, claims.trust_tier);
    encoder.into_bytes()
}

#[cfg(test)]
pub(crate) fn sign_attestation_for_test(claims: &AttestationClaims, key: &[u8; 32]) -> Vec<u8> {
    let claims_wire = encode_attestation_claims(claims);
    let tag = calculate_hmac_for_test(
        key,
        ATTESTATION_MAC_DOMAIN,
        &claims_wire,
        Some(claims.authorization_bundle_digest.as_bytes()),
    );
    let mut encoder = Encoder::with_prefix(ATTESTATION_PREFIX);
    encoder.field(1, &claims_wire);
    encoder.field(2, &tag);
    encoder.into_bytes()
}

#[cfg(test)]
pub(crate) fn sign_capability_for_test(claims: &CapabilityClaims, key: &[u8; 32]) -> Vec<u8> {
    let claims_wire = encode_capability_claims(claims);
    let tag = calculate_hmac_for_test(key, CAPABILITY_MAC_DOMAIN, &claims_wire, None);
    let mut encoder = Encoder::with_prefix(CAPABILITY_AUTHORIZATION_PREFIX);
    encoder.field(1, &claims_wire);
    encoder.field(2, &tag);
    encoder.into_bytes()
}

#[cfg(test)]
fn calculate_hmac_for_test(
    key: &[u8],
    domain: &[u8],
    claims_wire: &[u8],
    bound_digest: Option<&[u8; 32]>,
) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("固定测试 key 合法");
    mac.update(domain);
    mac.update(&(claims_wire.len() as u64).to_be_bytes());
    mac.update(claims_wire);
    if let Some(digest) = bound_digest {
        mac.update(digest);
    }
    mac.finalize().into_bytes().into()
}
