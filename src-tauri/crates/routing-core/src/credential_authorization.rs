//! 静态 Credential 精确授权合同与固定容量索引。
//!
//! 本模块只接收宿主已经规范化的 opaque Origin，并在快照编译时把每个可达
//! `RouteTarget × AccountSelectorMember` 的显式用户 Grant 固化为固定槽位。它不解析
//! URI/IDNA、不保存或解析 Secret，也不构造最终 URL、请求 nonce 或 CredentialUseContext。

use super::selection_lease::SelectedMember;
use super::{
    AccountCatalog, AccountId, AccountSelectorCatalog, CredentialCatalog, CredentialId, EndpointId,
    ModelDeploymentCatalog, ModelDeploymentId, RouteCandidate, SiteId,
    MAX_ACCOUNT_SELECTOR_MEMBERS, MAX_ROUTE_TARGETS,
};
use core::num::{NonZeroU16, NonZeroU64};

/// 单个快照内允许固化的最大 Credential 精确授权数。
pub(crate) const MAX_STATIC_CREDENTIAL_AUTHORIZATIONS: usize =
    MAX_ROUTE_TARGETS * MAX_ACCOUNT_SELECTOR_MEMBERS;

/// 当前设备上 Credential 绑定的稳定标识。
///
/// 它只表达宿主已验证的设备绑定事实，不是 SecretRef，也不能据此读取密钥材料。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct DeviceBindingId(NonZeroU64);

impl DeviceBindingId {
    /// 从非零稳定标识构造设备绑定。
    pub(crate) const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// 宿主规范化 Endpoint Origin 的不可解释载体。
///
/// routing-core 不验证、解析或暴露其内部字符串；相等性仅用于已规范化值的精确比较。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct EndpointOrigin<'a>(&'a str);

impl<'a> EndpointOrigin<'a> {
    /// 接收宿主唯一 canonicalizer 已生成的非空 Origin。
    pub(crate) const fn from_host_canonical(value: &'a str) -> Option<Self> {
        if value.is_empty() {
            None
        } else {
            Some(Self(value))
        }
    }
}

/// Endpoint Origin 的不可变修订号。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct EndpointOriginRevision(NonZeroU64);

impl EndpointOriginRevision {
    /// 从非零修订值构造 Origin 修订号。
    pub(crate) const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// 受注册认证方案的稳定标识。
///
/// 该类型只承担精确匹配，不能表达任意 Header 模板或认证注入逻辑。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct CredentialAuthScheme(NonZeroU16);

impl CredentialAuthScheme {
    /// 从宿主已注册的非零方案标识构造认证方案。
    pub(crate) const fn registered(value: u16) -> Option<Self> {
        match NonZeroU16::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// 上游 adapter 合同的不可变修订号。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct AdapterContractRevision(NonZeroU64);

impl AdapterContractRevision {
    /// 从非零修订值构造 adapter 合同修订号。
    pub(crate) const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// 用户确认的 Credential Grant 修订号。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct CredentialUseGrantRevision(NonZeroU64);

impl CredentialUseGrantRevision {
    /// 从非零修订值构造 Grant 修订号。
    pub(crate) const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

/// Grant 当前的生命周期状态。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum CredentialUseGrantState {
    /// 仍在等待用户确认，不能用于授权。
    Pending,
    /// 已得到本机用户确认。
    Approved,
    /// Endpoint Origin 或其他绑定事实已经变化。
    Stale,
    /// 用户或系统已撤销授权。
    Revoked,
}

/// 一个 Endpoint 与其规范化 Origin 的静态绑定。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct EndpointOriginDefinition<'a> {
    site: SiteId,
    endpoint: EndpointId,
    origin: EndpointOrigin<'a>,
    revision: EndpointOriginRevision,
}

impl<'a> EndpointOriginDefinition<'a> {
    /// 构造宿主已规范化的 Endpoint Origin 静态绑定。
    pub(crate) const fn new(
        site: SiteId,
        endpoint: EndpointId,
        origin: EndpointOrigin<'a>,
        revision: EndpointOriginRevision,
    ) -> Self {
        Self {
            site,
            endpoint,
            origin,
            revision,
        }
    }
}

/// 一个模型部署与其受信 adapter 合同修订的静态绑定。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct DeploymentAdapterContractDefinition {
    deployment: ModelDeploymentId,
    revision: AdapterContractRevision,
}

impl DeploymentAdapterContractDefinition {
    /// 构造模型部署的 adapter 合同修订绑定。
    pub(crate) const fn new(
        deployment: ModelDeploymentId,
        revision: AdapterContractRevision,
    ) -> Self {
        Self {
            deployment,
            revision,
        }
    }
}

/// 一个 Credential 在当前设备上的认证静态绑定。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct CredentialDeviceBindingDefinition {
    credential: CredentialId,
    device_binding: DeviceBindingId,
    auth_scheme: CredentialAuthScheme,
}

impl CredentialDeviceBindingDefinition {
    /// 构造 Credential、设备绑定与认证方案的静态关系。
    pub(crate) const fn new(
        credential: CredentialId,
        device_binding: DeviceBindingId,
        auth_scheme: CredentialAuthScheme,
    ) -> Self {
        Self {
            credential,
            device_binding,
            auth_scheme,
        }
    }
}

/// 本机用户授予的 Credential 精确使用许可。
///
/// 它不含 Secret、SecretRef、URL 路径或 Header；其 Origin 已由宿主规范化后作为 opaque
/// 值传入。Target、Selector member 与 Snapshot 身份仅在编译时固化到授权输出，不能由
/// 持久化 Grant 伪造。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct CredentialUseGrant<'a> {
    device_binding: DeviceBindingId,
    site: SiteId,
    endpoint: EndpointId,
    origin: EndpointOrigin<'a>,
    origin_revision: EndpointOriginRevision,
    account: AccountId,
    credential: CredentialId,
    auth_scheme: CredentialAuthScheme,
    adapter_contract_revision: AdapterContractRevision,
    revision: CredentialUseGrantRevision,
    state: CredentialUseGrantState,
    user_confirmed: bool,
}

impl<'a> CredentialUseGrant<'a> {
    /// 构造一个已由宿主持久层读取、尚待快照编译校验的 Grant。
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        device_binding: DeviceBindingId,
        site: SiteId,
        endpoint: EndpointId,
        origin: EndpointOrigin<'a>,
        origin_revision: EndpointOriginRevision,
        account: AccountId,
        credential: CredentialId,
        auth_scheme: CredentialAuthScheme,
        adapter_contract_revision: AdapterContractRevision,
        revision: CredentialUseGrantRevision,
        state: CredentialUseGrantState,
        user_confirmed: bool,
    ) -> Self {
        Self {
            device_binding,
            site,
            endpoint,
            origin,
            origin_revision,
            account,
            credential,
            auth_scheme,
            adapter_contract_revision,
            revision,
            state,
            user_confirmed,
        }
    }

    fn matches_static_binding(
        &self,
        endpoint_origin: EndpointOriginDefinition<'a>,
        adapter_contract: DeploymentAdapterContractDefinition,
        credential_binding: CredentialDeviceBindingDefinition,
        account: AccountId,
        credential: CredentialId,
    ) -> bool {
        self.device_binding == credential_binding.device_binding
            && self.site == endpoint_origin.site
            && self.endpoint == endpoint_origin.endpoint
            && self.origin == endpoint_origin.origin
            && self.origin_revision == endpoint_origin.revision
            && self.account == account
            && self.credential == credential
            && self.auth_scheme == credential_binding.auth_scheme
            && self.adapter_contract_revision == adapter_contract.revision
    }
}

/// 静态 Credential 精确授权编译所需的宿主事实。
///
/// 所有数据仅在快照激活前读取。运行期只消费已固化的固定槽，不重新查询这些切片。
#[derive(Clone, Copy)]
pub(crate) struct StaticCredentialAuthorizationDefinitions<'a> {
    device_binding: DeviceBindingId,
    endpoint_origins: &'a [EndpointOriginDefinition<'a>],
    deployment_adapter_contracts: &'a [DeploymentAdapterContractDefinition],
    credential_bindings: &'a [CredentialDeviceBindingDefinition],
    grants: &'a [CredentialUseGrant<'a>],
}

impl<'a> StaticCredentialAuthorizationDefinitions<'a> {
    /// 接收当前设备绑定与静态 Endpoint、adapter、Credential 绑定、Grant 定义。
    pub(crate) const fn new(
        device_binding: DeviceBindingId,
        endpoint_origins: &'a [EndpointOriginDefinition<'a>],
        deployment_adapter_contracts: &'a [DeploymentAdapterContractDefinition],
        credential_bindings: &'a [CredentialDeviceBindingDefinition],
        grants: &'a [CredentialUseGrant<'a>],
    ) -> Self {
        Self {
            device_binding,
            endpoint_origins,
            deployment_adapter_contracts,
            credential_bindings,
            grants,
        }
    }
}

/// 编译静态 Credential 精确授权时的关闭失败原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StaticCredentialAuthorizationError {
    /// Endpoint Origin 定义超过固定上限。
    TooManyEndpointOrigins,
    /// 同一 Endpoint 提供了多个 Origin 定义。
    DuplicateEndpointOrigin,
    /// adapter 合同定义超过固定上限。
    TooManyDeploymentAdapterContracts,
    /// 同一模型部署提供了多个 adapter 合同定义。
    DuplicateDeploymentAdapterContract,
    /// Credential 设备绑定定义超过固定上限。
    TooManyCredentialBindings,
    /// 同一 Credential 提供了多个设备绑定定义。
    DuplicateCredentialBinding,
    /// Grant 定义超过固定授权槽上限。
    TooManyGrants,
    /// 可达 Target 缺少 Endpoint Origin 定义。
    MissingEndpointOrigin,
    /// Endpoint Origin 的 Site 与可达 Target 不一致。
    EndpointOriginSiteMismatch,
    /// 可达模型部署缺少 adapter 合同定义。
    MissingDeploymentAdapterContract,
    /// Selector member 缺少 Credential 设备绑定定义。
    MissingCredentialBinding,
    /// Credential 设备绑定不属于当前快照声明的本机设备。
    CredentialDeviceBindingMismatch,
    /// 可达 Target × Selector member 缺少精确 Grant。
    MissingExactGrant,
    /// Grant 与当前 Target/Endpoint/Account/Credential/认证方案/adapter 绑定不一致。
    GrantBindingMismatch,
    /// 同一精确静态绑定出现多条 Grant。
    DuplicateGrant,
    /// Grant 未处于已批准状态。
    GrantNotApproved,
    /// Grant 缺少本机用户确认事实。
    GrantNotUserConfirmed,
    /// 定义中存在未绑定到任何可达 Target × SelectorMember 的 Grant。
    UnusedOrMismatchedGrant,
    /// 固定槽位计算超出编译期上限。
    AuthorizationIndexOutOfBounds,
}

/// 静态授权工厂在运行期的关闭失败原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CredentialAuthorizationLookupError {
    /// 快照由旧公开入口编译，未启用静态授权。
    AuthorizationUnavailable,
    /// 已解析 Target 不属于当前 CompiledRoutingSnapshot 实例。
    StaleSnapshot,
    /// 实际 Lease、Tracker 与封存的选择成员来源不再一致。
    SelectionProvenanceMismatch,
    /// 固定授权槽缺失或绑定不变量遭到破坏。
    AuthorizationInvariantViolation,
}

/// 固定索引内的静态授权记录。
#[derive(Clone, Copy)]
pub(crate) struct StaticCredentialAuthorizationEntry<'config> {
    snapshot_version: super::SnapshotVersion,
    target: super::RouteTarget,
    selector_id: super::AccountSelectorId,
    selector_revision: super::SelectorRevision,
    member_index: u8,
    account: AccountId,
    credential: CredentialId,
    endpoint_origin: EndpointOriginDefinition<'config>,
    adapter_contract: DeploymentAdapterContractDefinition,
    credential_binding: CredentialDeviceBindingDefinition,
    grant_revision: CredentialUseGrantRevision,
}

impl<'config> StaticCredentialAuthorizationEntry<'config> {
    fn matches(&self, selected_member: &SelectedMember<'_, 'config>) -> bool {
        self.snapshot_version == selected_member.snapshot_version()
            && self.target.id() == selected_member.target()
            && self.selector_id == self.target.account_selector()
            && self.selector_revision == selected_member.selector_revision()
            && self.member_index == selected_member.member_index()
            && self.account == selected_member.account()
            && self.credential == selected_member.credential()
            && self.endpoint_origin.site == self.target.site()
            && self.endpoint_origin.endpoint == self.target.endpoint()
            && self.adapter_contract.deployment == self.target.deployment()
            && self.credential_binding.credential == selected_member.credential()
            && self.grant_revision.0.get() != 0
    }
}

/// 已由同一快照签发的静态 Credential 使用授权。
///
/// 本类型刻意不实现 `Clone`、`Copy`、`Debug`、Serde 或 Origin getter。P1 只负责签发该
/// 静态能力，不构造 `CredentialUseContext`，更不会读取 Secret 或调用 Transport。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct CredentialUseAuthorization<'attempt, 'snapshot, 'config> {
    selected_member: &'attempt SelectedMember<'snapshot, 'config>,
    entry: StaticCredentialAuthorizationEntry<'config>,
}

impl<'attempt, 'snapshot, 'config> CredentialUseAuthorization<'attempt, 'snapshot, 'config> {
    fn new(
        selected_member: &'attempt SelectedMember<'snapshot, 'config>,
        entry: StaticCredentialAuthorizationEntry<'config>,
    ) -> Self {
        Self {
            selected_member,
            entry,
        }
    }

    fn matches(&self) -> bool {
        self.entry.matches(self.selected_member)
    }

    #[cfg(test)]
    fn credential_for_test(&self) -> CredentialId {
        self.entry.credential
    }
}

/// 已激活快照使用的固定容量授权索引。
pub(crate) struct StaticCredentialAuthorizations<'a> {
    entries: [Option<StaticCredentialAuthorizationEntry<'a>>; MAX_STATIC_CREDENTIAL_AUTHORIZATIONS],
}

impl<'a> StaticCredentialAuthorizations<'a> {
    /// 编译所有可达 Target × Selector member 的精确 Grant，并写入固定槽位。
    pub(crate) fn compile<'config>(
        snapshot_version: super::SnapshotVersion,
        candidates: &[RouteCandidate],
        deployments: &ModelDeploymentCatalog<'_>,
        selectors: &AccountSelectorCatalog<'config>,
        accounts: &AccountCatalog<'_>,
        credentials: &CredentialCatalog<'_>,
        definitions: StaticCredentialAuthorizationDefinitions<'a>,
    ) -> Result<Self, StaticCredentialAuthorizationError> {
        validate_definition_shapes(definitions)?;

        let mut entries = [None; MAX_STATIC_CREDENTIAL_AUTHORIZATIONS];
        let mut used_grants = [false; MAX_STATIC_CREDENTIAL_AUTHORIZATIONS];

        for (candidate_index, candidate) in candidates.iter().enumerate() {
            let target = candidate.target();
            let deployment = deployments
                .get(target.deployment())
                .ok_or(StaticCredentialAuthorizationError::GrantBindingMismatch)?;
            if deployment.site() != target.site() || deployment.endpoint() != target.endpoint() {
                return Err(StaticCredentialAuthorizationError::GrantBindingMismatch);
            }

            let endpoint_origin = definitions
                .endpoint_origins
                .iter()
                .copied()
                .find(|definition| definition.endpoint == target.endpoint())
                .ok_or(StaticCredentialAuthorizationError::MissingEndpointOrigin)?;
            if endpoint_origin.site != target.site() {
                return Err(StaticCredentialAuthorizationError::EndpointOriginSiteMismatch);
            }

            let adapter_contract = definitions
                .deployment_adapter_contracts
                .iter()
                .copied()
                .find(|definition| definition.deployment == target.deployment())
                .ok_or(StaticCredentialAuthorizationError::MissingDeploymentAdapterContract)?;

            let selector = selectors
                .get(target.account_selector())
                .ok_or(StaticCredentialAuthorizationError::GrantBindingMismatch)?;
            for (member_index, member) in selector.members().iter().copied().enumerate() {
                let account = accounts
                    .get(member.account())
                    .ok_or(StaticCredentialAuthorizationError::GrantBindingMismatch)?;
                let credential = credentials
                    .get(member.credential())
                    .ok_or(StaticCredentialAuthorizationError::GrantBindingMismatch)?;
                if account.site() != target.site() || credential.account() != member.account() {
                    return Err(StaticCredentialAuthorizationError::GrantBindingMismatch);
                }

                let credential_binding = definitions
                    .credential_bindings
                    .iter()
                    .copied()
                    .find(|definition| definition.credential == member.credential())
                    .ok_or(StaticCredentialAuthorizationError::MissingCredentialBinding)?;
                if credential_binding.device_binding != definitions.device_binding {
                    return Err(
                        StaticCredentialAuthorizationError::CredentialDeviceBindingMismatch,
                    );
                }

                let mut matched_grant = None;
                for (grant_index, grant) in definitions.grants.iter().copied().enumerate() {
                    if grant.matches_static_binding(
                        endpoint_origin,
                        adapter_contract,
                        credential_binding,
                        member.account(),
                        member.credential(),
                    ) && matched_grant.replace((grant_index, grant)).is_some()
                    {
                        return Err(StaticCredentialAuthorizationError::DuplicateGrant);
                    }
                }
                let Some((grant_index, grant)) = matched_grant else {
                    if definitions
                        .grants
                        .iter()
                        .any(|grant| grant.credential == member.credential())
                    {
                        return Err(StaticCredentialAuthorizationError::GrantBindingMismatch);
                    }
                    return Err(StaticCredentialAuthorizationError::MissingExactGrant);
                };
                if grant.state != CredentialUseGrantState::Approved {
                    return Err(StaticCredentialAuthorizationError::GrantNotApproved);
                }
                if !grant.user_confirmed {
                    return Err(StaticCredentialAuthorizationError::GrantNotUserConfirmed);
                }

                let candidate_index = u8::try_from(candidate_index).map_err(|_| {
                    StaticCredentialAuthorizationError::AuthorizationIndexOutOfBounds
                })?;
                let member_index = u8::try_from(member_index).map_err(|_| {
                    StaticCredentialAuthorizationError::AuthorizationIndexOutOfBounds
                })?;
                let slot = authorization_index(candidate_index, member_index)
                    .ok_or(StaticCredentialAuthorizationError::AuthorizationIndexOutOfBounds)?;
                if entries[slot].is_some() {
                    return Err(StaticCredentialAuthorizationError::AuthorizationIndexOutOfBounds);
                }
                entries[slot] = Some(StaticCredentialAuthorizationEntry {
                    snapshot_version,
                    target,
                    selector_id: selector.id(),
                    selector_revision: selector.revision(),
                    member_index,
                    account: member.account(),
                    credential: member.credential(),
                    endpoint_origin,
                    adapter_contract,
                    credential_binding,
                    grant_revision: grant.revision,
                });
                used_grants[grant_index] = true;
            }
        }

        if used_grants[..definitions.grants.len()]
            .iter()
            .any(|was_used| !was_used)
        {
            return Err(StaticCredentialAuthorizationError::UnusedOrMismatchedGrant);
        }

        Ok(Self { entries })
    }

    /// 在固定槽位中取得某个已解析 Target 与 Selector member 的授权记录。
    fn entry_for(
        &self,
        candidate_index: u8,
        member_index: u8,
    ) -> Option<StaticCredentialAuthorizationEntry<'a>> {
        let index = authorization_index(candidate_index, member_index)?;
        self.entries[index]
    }

    /// 仅当固定槽位仍与同一快照、实际获取 Lease 的 Target 与成员完全一致时签发能力。
    pub(crate) fn authorization_for<'attempt, 'snapshot>(
        &self,
        selected_member: &'attempt SelectedMember<'snapshot, 'a>,
    ) -> Option<CredentialUseAuthorization<'attempt, 'snapshot, 'a>> {
        let entry = self.entry_for(
            selected_member.candidate_index(),
            selected_member.member_index(),
        )?;
        let authorization = CredentialUseAuthorization::new(selected_member, entry);
        authorization.matches().then_some(authorization)
    }
}

fn validate_definition_shapes(
    definitions: StaticCredentialAuthorizationDefinitions<'_>,
) -> Result<(), StaticCredentialAuthorizationError> {
    if definitions.endpoint_origins.len() > MAX_ROUTE_TARGETS {
        return Err(StaticCredentialAuthorizationError::TooManyEndpointOrigins);
    }
    if definitions.deployment_adapter_contracts.len() > MAX_ROUTE_TARGETS {
        return Err(StaticCredentialAuthorizationError::TooManyDeploymentAdapterContracts);
    }
    if definitions.credential_bindings.len() > MAX_ROUTE_TARGETS {
        return Err(StaticCredentialAuthorizationError::TooManyCredentialBindings);
    }
    if definitions.grants.len() > MAX_STATIC_CREDENTIAL_AUTHORIZATIONS {
        return Err(StaticCredentialAuthorizationError::TooManyGrants);
    }

    for (index, definition) in definitions.endpoint_origins.iter().enumerate() {
        if definitions.endpoint_origins[..index]
            .iter()
            .any(|previous| previous.endpoint == definition.endpoint)
        {
            return Err(StaticCredentialAuthorizationError::DuplicateEndpointOrigin);
        }
    }
    for (index, definition) in definitions.deployment_adapter_contracts.iter().enumerate() {
        if definitions.deployment_adapter_contracts[..index]
            .iter()
            .any(|previous| previous.deployment == definition.deployment)
        {
            return Err(StaticCredentialAuthorizationError::DuplicateDeploymentAdapterContract);
        }
    }
    for (index, definition) in definitions.credential_bindings.iter().enumerate() {
        if definitions.credential_bindings[..index]
            .iter()
            .any(|previous| previous.credential == definition.credential)
        {
            return Err(StaticCredentialAuthorizationError::DuplicateCredentialBinding);
        }
    }
    Ok(())
}

fn authorization_index(candidate_index: u8, member_index: u8) -> Option<usize> {
    let candidate_index = usize::from(candidate_index);
    let member_index = usize::from(member_index);
    if candidate_index >= MAX_ROUTE_TARGETS || member_index >= MAX_ACCOUNT_SELECTOR_MEMBERS {
        return None;
    }
    candidate_index
        .checked_mul(MAX_ACCOUNT_SELECTOR_MEMBERS)?
        .checked_add(member_index)
}

#[cfg(test)]
mod tests {
    use super::super::selection_input::AccountSelectionEligibility;
    use super::super::selection_lease::{
        PrioritySelectionStart, SelectionLeaseRegistry, TransportHandoffSuccess,
    };
    use super::*;
    use crate::{
        AccountCredentialDefinitions, AccountDefinition, AccountRuntimeDefinition,
        AccountSelectorDefinition, AccountSelectorMember, CompiledRoutingSnapshot,
        CredentialDefinition, CredentialRuntimeDefinition, CredentialSelectionPolicy,
        HealthRegistry, HealthTick, IngressClassifier, IngressRequest, ModelDeploymentDefinition,
        OperationId, QuotaGroupId, QuotaGroupRuntimeDefinition, QuotaSelectionUnit,
        QuotaSelectionUnitId, QuotaTopologySource, RetryPolicy, RouteCandidate, RoutePlanner,
        RouteStageId, RouteTarget, RoutingStrategy, SelectionRuntimeDefinitions, SelectionSession,
        SelectorAffinitySalt, SelectorRevision, VerifiedIngressDisposition,
    };
    use core::num::NonZeroU16;

    fn account(value: u64) -> AccountId {
        AccountId::new(value).expect("测试账户 ID 非零")
    }

    fn credential(value: u64) -> CredentialId {
        CredentialId::new(value).expect("测试凭据 ID 非零")
    }

    fn deployment(value: u64) -> ModelDeploymentId {
        ModelDeploymentId::new(value).expect("测试部署 ID 非零")
    }

    fn endpoint(value: u64) -> EndpointId {
        EndpointId::new(value).expect("测试端点 ID 非零")
    }

    fn site(value: u64) -> SiteId {
        SiteId::new(value).expect("测试站点 ID 非零")
    }

    fn target(value: u64) -> super::super::RouteTargetId {
        super::super::RouteTargetId::new(value).expect("测试 Target ID 非零")
    }

    fn selector(value: u64) -> super::super::AccountSelectorId {
        super::super::AccountSelectorId::new(value).expect("测试 Selector ID 非零")
    }

    fn stage(value: u64) -> RouteStageId {
        RouteStageId::new(value).expect("测试阶段 ID 非零")
    }

    fn unit(value: u64) -> QuotaSelectionUnitId {
        QuotaSelectionUnitId::new(value).expect("测试额度单元 ID 非零")
    }

    fn group(value: u64) -> QuotaGroupId {
        QuotaGroupId::new(value).expect("测试额度组 ID 非零")
    }

    fn device(value: u64) -> DeviceBindingId {
        DeviceBindingId::new(value).expect("测试设备绑定 ID 非零")
    }

    fn origin(value: &'static str) -> EndpointOrigin<'static> {
        EndpointOrigin::from_host_canonical(value).expect("测试 Origin 非空")
    }

    fn origin_revision(value: u64) -> EndpointOriginRevision {
        EndpointOriginRevision::new(value).expect("测试 Origin 修订非零")
    }

    fn auth_scheme(value: u16) -> CredentialAuthScheme {
        CredentialAuthScheme::registered(value).expect("测试认证方案非零")
    }

    fn adapter_revision(value: u64) -> AdapterContractRevision {
        AdapterContractRevision::new(value).expect("测试 adapter 修订非零")
    }

    fn grant_revision(value: u64) -> CredentialUseGrantRevision {
        CredentialUseGrantRevision::new(value).expect("测试 Grant 修订非零")
    }

    fn exact_grant(
        origin: EndpointOrigin<'static>,
        origin_revision: EndpointOriginRevision,
        account: AccountId,
        credential: CredentialId,
        auth_scheme: CredentialAuthScheme,
        adapter_revision: AdapterContractRevision,
        state: CredentialUseGrantState,
        user_confirmed: bool,
    ) -> CredentialUseGrant<'static> {
        CredentialUseGrant::new(
            device(1),
            site(1),
            endpoint(1),
            origin,
            origin_revision,
            account,
            credential,
            auth_scheme,
            adapter_revision,
            grant_revision(1),
            state,
            user_confirmed,
        )
    }

    fn compile_single<'a>(
        endpoint_origins: &'a [EndpointOriginDefinition<'a>],
        adapter_contracts: &'a [DeploymentAdapterContractDefinition],
        credential_bindings: &'a [CredentialDeviceBindingDefinition],
        grants: &'a [CredentialUseGrant<'a>],
    ) -> Result<StaticCredentialAuthorizations<'a>, StaticCredentialAuthorizationError> {
        let groups = [group(1)];
        let units = [QuotaSelectionUnit::new(
            unit(1),
            NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            account(1),
            credential(1),
            unit(1),
            0,
        )];
        let selectors = [AccountSelectorDefinition::new(
            selector(1),
            SelectorRevision::new(1).expect("测试 Selector 修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )
        .expect("测试 Selector 有效")];
        let candidates = [RouteCandidate::ready(
            stage(1),
            RouteTarget::new(target(1), site(1), deployment(1), endpoint(1), selector(1)),
            0,
        )];
        let deployments = [ModelDeploymentDefinition::new(
            deployment(1),
            site(1),
            endpoint(1),
        )];
        let accounts = [AccountDefinition::new(account(1), site(1))];
        let credentials = [CredentialDefinition::new(credential(1), account(1))];
        let deployment_catalog =
            ModelDeploymentCatalog::new(&deployments).expect("测试部署目录有效");
        let selector_catalog =
            AccountSelectorCatalog::new(&selectors).expect("测试 Selector 目录有效");
        let account_catalog = AccountCatalog::new(&accounts).expect("测试账户目录有效");
        let credential_catalog =
            CredentialCatalog::new(&credentials, &account_catalog).expect("测试凭据目录有效");

        StaticCredentialAuthorizations::compile(
            super::super::SnapshotVersion::new(1).expect("测试快照版本非零"),
            &candidates,
            &deployment_catalog,
            &selector_catalog,
            &account_catalog,
            &credential_catalog,
            StaticCredentialAuthorizationDefinitions::new(
                device(1),
                endpoint_origins,
                adapter_contracts,
                credential_bindings,
                grants,
            ),
        )
    }

    #[test]
    fn compiles_exact_approved_confirmed_grant_into_fixed_slot() {
        let origin = origin("https://api.example.test");
        let origin_revision = origin_revision(1);
        let auth_scheme = auth_scheme(1);
        let adapter_revision = adapter_revision(1);
        let endpoint_origins = [EndpointOriginDefinition::new(
            site(1),
            endpoint(1),
            origin,
            origin_revision,
        )];
        let adapter_contracts = [DeploymentAdapterContractDefinition::new(
            deployment(1),
            adapter_revision,
        )];
        let credential_bindings = [CredentialDeviceBindingDefinition::new(
            credential(1),
            device(1),
            auth_scheme,
        )];
        let grants = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        )];

        let authorizations = compile_single(
            &endpoint_origins,
            &adapter_contracts,
            &credential_bindings,
            &grants,
        )
        .expect("精确已批准 Grant 必须编译");
        assert!(authorizations.entry_for(0, 0).is_some());
        assert!(authorizations.entry_for(0, 1).is_none());
        assert!(authorizations.entry_for(1, 0).is_none());
    }

    #[test]
    fn rejects_missing_duplicate_pending_unconfirmed_and_mismatched_grants() {
        let origin = origin("https://api.example.test");
        let origin_revision = origin_revision(1);
        let auth_scheme = auth_scheme(1);
        let adapter_revision = adapter_revision(1);
        let endpoint_origins = [EndpointOriginDefinition::new(
            site(1),
            endpoint(1),
            origin,
            origin_revision,
        )];
        let adapter_contracts = [DeploymentAdapterContractDefinition::new(
            deployment(1),
            adapter_revision,
        )];
        let credential_bindings = [CredentialDeviceBindingDefinition::new(
            credential(1),
            device(1),
            auth_scheme,
        )];

        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &[],
            ),
            Err(StaticCredentialAuthorizationError::MissingExactGrant)
        ));

        let pending = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Pending,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &pending,
            ),
            Err(StaticCredentialAuthorizationError::GrantNotApproved)
        ));

        let stale = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Stale,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &stale,
            ),
            Err(StaticCredentialAuthorizationError::GrantNotApproved)
        ));

        let revoked = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Revoked,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &revoked,
            ),
            Err(StaticCredentialAuthorizationError::GrantNotApproved)
        ));

        let unconfirmed = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            false,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &unconfirmed,
            ),
            Err(StaticCredentialAuthorizationError::GrantNotUserConfirmed)
        ));

        let exact = exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        );
        let duplicates = [exact, exact];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &duplicates,
            ),
            Err(StaticCredentialAuthorizationError::DuplicateGrant)
        ));

        let mismatched_adapter = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            AdapterContractRevision::new(2).expect("测试 adapter 修订非零"),
            CredentialUseGrantState::Approved,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &mismatched_adapter,
            ),
            Err(StaticCredentialAuthorizationError::GrantBindingMismatch)
        ));

        let mismatched_origin = [exact_grant(
            EndpointOrigin::from_host_canonical("https://other.example.test")
                .expect("测试 Origin 非空"),
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &mismatched_origin,
            ),
            Err(StaticCredentialAuthorizationError::GrantBindingMismatch)
        ));

        let mismatched_revision = [exact_grant(
            origin,
            EndpointOriginRevision::new(2).expect("测试 Origin 修订非零"),
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &mismatched_revision,
            ),
            Err(StaticCredentialAuthorizationError::GrantBindingMismatch)
        ));

        let mismatched_account = [exact_grant(
            origin,
            origin_revision,
            account(2),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &mismatched_account,
            ),
            Err(StaticCredentialAuthorizationError::GrantBindingMismatch)
        ));

        let mismatched_auth_scheme = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            CredentialAuthScheme::registered(2).expect("测试认证方案非零"),
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &mismatched_auth_scheme,
            ),
            Err(StaticCredentialAuthorizationError::GrantBindingMismatch)
        ));

        let wrong_device_bindings = [CredentialDeviceBindingDefinition::new(
            credential(1),
            device(2),
            auth_scheme,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &wrong_device_bindings,
                &unconfirmed,
            ),
            Err(StaticCredentialAuthorizationError::CredentialDeviceBindingMismatch)
        ));
    }

    #[test]
    fn rejects_duplicate_static_definitions_and_unreachable_grants() {
        let origin = origin("https://api.example.test");
        let origin_revision = origin_revision(1);
        let auth_scheme = auth_scheme(1);
        let adapter_revision = adapter_revision(1);
        let endpoint_origins = [
            EndpointOriginDefinition::new(site(1), endpoint(1), origin, origin_revision),
            EndpointOriginDefinition::new(site(1), endpoint(1), origin, origin_revision),
        ];
        let adapter_contracts = [DeploymentAdapterContractDefinition::new(
            deployment(1),
            adapter_revision,
        )];
        let credential_bindings = [CredentialDeviceBindingDefinition::new(
            credential(1),
            device(1),
            auth_scheme,
        )];
        let grants = [exact_grant(
            origin,
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            CredentialUseGrantState::Approved,
            true,
        )];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &grants,
            ),
            Err(StaticCredentialAuthorizationError::DuplicateEndpointOrigin)
        ));

        let endpoint_origins = [EndpointOriginDefinition::new(
            site(1),
            endpoint(1),
            origin,
            origin_revision,
        )];
        let adapter_contracts = [
            DeploymentAdapterContractDefinition::new(deployment(1), adapter_revision),
            DeploymentAdapterContractDefinition::new(deployment(1), adapter_revision),
        ];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &grants,
            ),
            Err(StaticCredentialAuthorizationError::DuplicateDeploymentAdapterContract)
        ));

        let wrong_site_origins = [EndpointOriginDefinition::new(
            site(2),
            endpoint(1),
            origin,
            origin_revision,
        )];
        let adapter_contracts = [DeploymentAdapterContractDefinition::new(
            deployment(1),
            adapter_revision,
        )];
        assert!(matches!(
            compile_single(
                &wrong_site_origins,
                &adapter_contracts,
                &credential_bindings,
                &grants,
            ),
            Err(StaticCredentialAuthorizationError::EndpointOriginSiteMismatch)
        ));

        let adapter_contracts = [DeploymentAdapterContractDefinition::new(
            deployment(1),
            adapter_revision,
        )];
        let credential_bindings = [
            CredentialDeviceBindingDefinition::new(credential(1), device(1), auth_scheme),
            CredentialDeviceBindingDefinition::new(credential(1), device(1), auth_scheme),
        ];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &grants,
            ),
            Err(StaticCredentialAuthorizationError::DuplicateCredentialBinding)
        ));

        let credential_bindings = [CredentialDeviceBindingDefinition::new(
            credential(1),
            device(1),
            auth_scheme,
        )];
        let unused = CredentialUseGrant::new(
            device(1),
            site(1),
            endpoint(2),
            EndpointOrigin::from_host_canonical("https://other.example.test")
                .expect("测试 Origin 非空"),
            origin_revision,
            account(1),
            credential(1),
            auth_scheme,
            adapter_revision,
            grant_revision(2),
            CredentialUseGrantState::Approved,
            true,
        );
        let grants = [grants[0], unused];
        assert!(matches!(
            compile_single(
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &grants,
            ),
            Err(StaticCredentialAuthorizationError::UnusedOrMismatchedGrant)
        ));
    }

    #[test]
    fn same_credential_requires_separate_exact_grants_for_each_origin() {
        let first_origin = origin("https://first.example.test");
        let second_origin = origin("https://second.example.test");
        let first_origin_revision = origin_revision(1);
        let second_origin_revision = origin_revision(2);
        let auth_scheme = auth_scheme(1);
        let first_adapter_revision = adapter_revision(1);
        let second_adapter_revision = adapter_revision(2);
        let groups = [group(1)];
        let units = [QuotaSelectionUnit::new(
            unit(1),
            NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            account(1),
            credential(1),
            unit(1),
            0,
        )];
        let selectors = [AccountSelectorDefinition::new(
            selector(1),
            SelectorRevision::new(1).expect("测试 Selector 修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )
        .expect("测试 Selector 有效")];
        let candidates = [
            RouteCandidate::ready(
                stage(1),
                RouteTarget::new(target(1), site(1), deployment(1), endpoint(1), selector(1)),
                0,
            ),
            RouteCandidate::ready(
                stage(2),
                RouteTarget::new(target(2), site(1), deployment(2), endpoint(2), selector(1)),
                0,
            ),
        ];
        let deployments = [
            ModelDeploymentDefinition::new(deployment(1), site(1), endpoint(1)),
            ModelDeploymentDefinition::new(deployment(2), site(1), endpoint(2)),
        ];
        let accounts = [AccountDefinition::new(account(1), site(1))];
        let credentials = [CredentialDefinition::new(credential(1), account(1))];
        let endpoint_origins = [
            EndpointOriginDefinition::new(
                site(1),
                endpoint(1),
                first_origin,
                first_origin_revision,
            ),
            EndpointOriginDefinition::new(
                site(1),
                endpoint(2),
                second_origin,
                second_origin_revision,
            ),
        ];
        let adapter_contracts = [
            DeploymentAdapterContractDefinition::new(deployment(1), first_adapter_revision),
            DeploymentAdapterContractDefinition::new(deployment(2), second_adapter_revision),
        ];
        let credential_bindings = [CredentialDeviceBindingDefinition::new(
            credential(1),
            device(1),
            auth_scheme,
        )];
        let grants = [
            exact_grant(
                first_origin,
                first_origin_revision,
                account(1),
                credential(1),
                auth_scheme,
                first_adapter_revision,
                CredentialUseGrantState::Approved,
                true,
            ),
            CredentialUseGrant::new(
                device(1),
                site(1),
                endpoint(2),
                second_origin,
                second_origin_revision,
                account(1),
                credential(1),
                auth_scheme,
                second_adapter_revision,
                grant_revision(2),
                CredentialUseGrantState::Approved,
                true,
            ),
        ];
        let deployment_catalog =
            ModelDeploymentCatalog::new(&deployments).expect("测试部署目录有效");
        let selector_catalog =
            AccountSelectorCatalog::new(&selectors).expect("测试 Selector 目录有效");
        let account_catalog = AccountCatalog::new(&accounts).expect("测试账户目录有效");
        let credential_catalog =
            CredentialCatalog::new(&credentials, &account_catalog).expect("测试凭据目录有效");

        let authorizations = StaticCredentialAuthorizations::compile(
            super::super::SnapshotVersion::new(1).expect("测试快照版本非零"),
            &candidates,
            &deployment_catalog,
            &selector_catalog,
            &account_catalog,
            &credential_catalog,
            StaticCredentialAuthorizationDefinitions::new(
                device(1),
                &endpoint_origins,
                &adapter_contracts,
                &credential_bindings,
                &grants,
            ),
        )
        .expect("同一 Credential 的两个精确 Origin Grant 都必须编译");
        assert!(authorizations.entry_for(0, 0).is_some());
        assert!(authorizations.entry_for(1, 0).is_some());
    }

    #[test]
    fn authorization_factory_requires_same_snapshot_and_real_member_slot() {
        let origin = origin("https://api.example.test");
        let origin_revision = origin_revision(1);
        let auth_scheme = auth_scheme(1);
        let adapter_revision = adapter_revision(1);
        let groups = [group(1)];
        let units = [QuotaSelectionUnit::new(
            unit(1),
            NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [
            AccountSelectorMember::new(account(1), credential(1), unit(1), 0),
            AccountSelectorMember::new(account(2), credential(2), unit(1), 0),
        ];
        let selectors = [AccountSelectorDefinition::new(
            selector(1),
            SelectorRevision::new(1).expect("测试 Selector 修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )
        .expect("测试 Selector 有效")];
        let candidates = [RouteCandidate::ready(
            stage(1),
            RouteTarget::new(target(1), site(1), deployment(1), endpoint(1), selector(1)),
            0,
        )];
        let deployments = [ModelDeploymentDefinition::new(
            deployment(1),
            site(1),
            endpoint(1),
        )];
        let accounts = [
            AccountDefinition::new(account(1), site(1)),
            AccountDefinition::new(account(2), site(1)),
        ];
        let credentials = [
            CredentialDefinition::new(credential(1), account(1)),
            CredentialDefinition::new(credential(2), account(2)),
        ];
        let endpoint_origins = [EndpointOriginDefinition::new(
            site(1),
            endpoint(1),
            origin,
            origin_revision,
        )];
        let adapter_contracts = [DeploymentAdapterContractDefinition::new(
            deployment(1),
            adapter_revision,
        )];
        let credential_bindings = [
            CredentialDeviceBindingDefinition::new(credential(1), device(1), auth_scheme),
            CredentialDeviceBindingDefinition::new(credential(2), device(1), auth_scheme),
        ];
        let grants = [
            exact_grant(
                origin,
                origin_revision,
                account(1),
                credential(1),
                auth_scheme,
                adapter_revision,
                CredentialUseGrantState::Approved,
                true,
            ),
            exact_grant(
                origin,
                origin_revision,
                account(2),
                credential(2),
                auth_scheme,
                adapter_revision,
                CredentialUseGrantState::Approved,
                true,
            ),
        ];
        let definitions = StaticCredentialAuthorizationDefinitions::new(
            device(1),
            &endpoint_origins,
            &adapter_contracts,
            &credential_bindings,
            &grants,
        );
        let version = super::super::SnapshotVersion::new(1).expect("测试快照版本非零");
        let compiled = CompiledRoutingSnapshot::compile_with_static_credential_authorizations(
            version,
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
            definitions,
        )
        .expect("精确授权快照应编译成功");
        let legacy = CompiledRoutingSnapshot::compile(
            version,
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("旧快照仍应保持兼容");

        let request = IngressClassifier::new()
            .classify(IngressRequest::routed(OperationId::CONVERSATION, version))
            .expect("测试路由请求可分类");
        let VerifiedIngressDisposition::Routed(request) = request else {
            panic!("会话请求必须进入 Routed");
        };
        let mut health = HealthRegistry::new();
        let route_eligibility = health.eligibility_for(compiled.routing(), HealthTick::new(0));
        let plan = RoutePlanner::plan(&request, compiled.routing(), &route_eligibility, 0)
            .expect("测试计划有效");
        let resolved = compiled
            .resolve_plan_target(&plan, 0)
            .expect("计划与授权快照一致")
            .expect("首个 Target 存在");
        let account_runtime = [
            AccountRuntimeDefinition::new(
                account(1),
                NonZeroU16::new(1).expect("测试账户上限非零"),
            ),
            AccountRuntimeDefinition::new(
                account(2),
                NonZeroU16::new(1).expect("测试账户上限非零"),
            ),
        ];
        let credential_runtime = [
            CredentialRuntimeDefinition::new(
                credential(1),
                NonZeroU16::new(1).expect("测试凭据上限非零"),
            ),
            CredentialRuntimeDefinition::new(
                credential(2),
                NonZeroU16::new(1).expect("测试凭据上限非零"),
            ),
        ];
        let quota_runtime = [QuotaGroupRuntimeDefinition::new(
            group(1),
            NonZeroU16::new(1).expect("测试额度组上限非零"),
        )];
        let runtime_definitions =
            SelectionRuntimeDefinitions::new(&quota_runtime, &account_runtime, &credential_runtime)
                .expect("测试运行时定义有效");
        let layout = compiled
            .selection_runtime_layout(&runtime_definitions)
            .expect("授权快照可生成选择布局");
        let registry =
            SelectionLeaseRegistry::new(layout, &compiled).expect("选择 Registry 可激活");
        let selection_request = compiled
            .selection_request(resolved, SelectionSession::Absent)
            .expect("同代选择请求有效");
        let selection_eligibility = AccountSelectionEligibility::new(selection_request, 1, 0b10)
            .expect("仅第二成员的动态资格有效");
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试重试策略有效"))
            .expect("测试 Coordinator 有效");
        let selected = match registry.start_priority(
            coordinator
                .start(&route_eligibility)
                .expect("首个 Permit 可签发"),
            selection_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("第二成员必须成功取得 Lease"),
        };

        let authorization = selected
            .credential_use_authorization(&compiled)
            .expect("实际选择第二成员时必须取得第二成员的授权");
        assert_eq!(authorization.credential_for_test(), credential(2));

        let foreign = CompiledRoutingSnapshot::compile_with_static_credential_authorizations(
            version,
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
            definitions,
        )
        .expect("同版本的另一快照也可独立编译");
        assert!(matches!(
            selected.credential_use_authorization(&foreign),
            Err(CredentialAuthorizationLookupError::StaleSnapshot)
        ));

        let handoff = selected.into_transport_handoff();
        {
            let handoff_authorization = handoff
                .credential_use_authorization(&compiled)
                .expect("交接后不得丢失第二成员的授权来源");
            assert_eq!(handoff_authorization.credential_for_test(), credential(2));
        }
        let incomplete_handoff = match handoff.into_success_completion() {
            TransportHandoffSuccess::Incomplete(handoff) => handoff,
            TransportHandoffSuccess::Completed(_) => panic!("无响应证据的 handoff 不能提前完成"),
        };
        let incomplete_authorization = incomplete_handoff
            .credential_use_authorization(&compiled)
            .expect("不完整成功回退必须保留第二成员的授权来源");
        assert_eq!(
            incomplete_authorization.credential_for_test(),
            credential(2)
        );

        let mut legacy_health = HealthRegistry::new();
        let legacy_route_eligibility =
            legacy_health.eligibility_for(legacy.routing(), HealthTick::new(0));
        let legacy_plan =
            RoutePlanner::plan(&request, legacy.routing(), &legacy_route_eligibility, 0)
                .expect("旧快照计划仍有效");
        let legacy_resolved = legacy
            .resolve_plan_target(&legacy_plan, 0)
            .expect("旧计划与快照一致")
            .expect("旧 Target 存在");
        let legacy_layout = legacy
            .selection_runtime_layout(&runtime_definitions)
            .expect("旧快照可生成选择布局");
        let legacy_registry =
            SelectionLeaseRegistry::new(legacy_layout, &legacy).expect("旧 Registry 可激活");
        let legacy_selection_request = legacy
            .selection_request(legacy_resolved, SelectionSession::Absent)
            .expect("旧快照同代选择请求有效");
        let legacy_selection_eligibility =
            AccountSelectionEligibility::new(legacy_selection_request, 1, 1)
                .expect("旧快照成员动态资格有效");
        let mut legacy_coordinator = legacy_plan
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试重试策略有效"))
            .expect("旧测试 Coordinator 有效");
        let legacy_selected = match legacy_registry.start_priority(
            legacy_coordinator
                .start(&legacy_route_eligibility)
                .expect("旧首个 Permit 可签发"),
            legacy_selection_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("旧快照唯一有效成员必须成功取得 Lease"),
        };
        assert!(matches!(
            legacy_selected.credential_use_authorization(&legacy),
            Err(CredentialAuthorizationLookupError::AuthorizationUnavailable)
        ));
    }
}
