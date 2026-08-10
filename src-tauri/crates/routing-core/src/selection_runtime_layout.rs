//! Account、Credential 与全局额度组的统一静态运行时布局合同。
//!
//! 本模块只在快照激活前核对所有可达静态资源的唯一并发上限，并为同代目标创建受控、
//! 零分配的三资源 binding。它不选择账户、不发放 Lease、不维护在途数、健康、游标或
//! 任何其他运行时状态。

use core::{marker::PhantomData, num::NonZeroU16, slice::Iter};

use super::{
    AccountId, AccountSelectorMember, CredentialId, QuotaGroupId, QuotaSelectionUnitId,
    ResolvedRouteTarget, RoutingSnapshot, MAX_ACCOUNTS, MAX_CREDENTIALS,
};

/// 单个编译快照允许追踪的最多全局额度组数量。
///
/// 该上限是本地路由核心的明确产品边界。超过上限的配置必须在激活前拒绝，不能拆分
/// Registry、静默降级，或把本应共享的额度组误当作多个独立状态。
pub const MAX_TRACKED_QUOTA_GROUPS: usize = 256;

/// 单个编译快照允许追踪的最多 Account 数量。
///
/// 运行时布局沿用静态目录的固定边界，避免在本地热路径扩大内存或查找规模。
pub const MAX_TRACKED_ACCOUNTS: usize = MAX_ACCOUNTS;

/// 单个编译快照允许追踪的最多 Credential 数量。
///
/// 运行时布局沿用静态目录的固定边界，避免在本地热路径扩大内存或查找规模。
pub const MAX_TRACKED_CREDENTIALS: usize = MAX_CREDENTIALS;

/// 一个 Account 的静态运行时定义。
///
/// 本定义不携带授权、API Key、Bearer、Secret 或任何可用于发起上游请求的材料；
/// `max_inflight` 只为后续同一布局上的原子 Lease 合同预留固定上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountRuntimeDefinition {
    id: AccountId,
    max_inflight: NonZeroU16,
}

impl AccountRuntimeDefinition {
    /// 创建一个由快照布局工厂后续校验可达性的 Account 定义。
    pub const fn new(id: AccountId, max_inflight: NonZeroU16) -> Self {
        Self { id, max_inflight }
    }

    /// 返回稳定 Account 标识。
    pub const fn id(self) -> AccountId {
        self.id
    }

    /// 返回该 Account 允许的正整数在途上限。
    pub const fn max_inflight(self) -> NonZeroU16 {
        self.max_inflight
    }
}

/// 一个 Credential 的静态运行时定义。
///
/// 本定义不携带 API Key、Bearer、Secret 或任何可用于发起上游请求的材料；
/// `max_inflight` 只为后续同一布局上的原子 Lease 合同预留固定上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialRuntimeDefinition {
    id: CredentialId,
    max_inflight: NonZeroU16,
}

impl CredentialRuntimeDefinition {
    /// 创建一个由快照布局工厂后续校验可达性的 Credential 定义。
    pub const fn new(id: CredentialId, max_inflight: NonZeroU16) -> Self {
        Self { id, max_inflight }
    }

    /// 返回稳定 Credential 标识。
    pub const fn id(self) -> CredentialId {
        self.id
    }

    /// 返回该 Credential 允许的正整数在途上限。
    pub const fn max_inflight(self) -> NonZeroU16 {
        self.max_inflight
    }
}

/// 一个全局额度组的静态运行时定义。
///
/// 本定义不携带账户、凭据、API Key、Bearer、Secret 或任何可用于发起上游请求的材料；
/// `max_inflight` 只为后续同一布局上的原子 Lease 合同预留固定上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaGroupRuntimeDefinition {
    id: QuotaGroupId,
    max_inflight: NonZeroU16,
}

impl QuotaGroupRuntimeDefinition {
    /// 创建一个由快照布局工厂后续校验可达性的额度组定义。
    pub const fn new(id: QuotaGroupId, max_inflight: NonZeroU16) -> Self {
        Self { id, max_inflight }
    }

    /// 返回稳定的额度组标识。
    pub const fn id(self) -> QuotaGroupId {
        self.id
    }

    /// 返回该组允许的正整数在途上限。
    pub const fn max_inflight(self) -> NonZeroU16 {
        self.max_inflight
    }
}

/// 构造统一静态运行时定义输入时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionRuntimeDefinitionsError {
    /// 没有任何额度组定义。
    EmptyQuotaGroupDefinitions,
    /// 额度组定义数量超过固定上限。
    TooManyQuotaGroupDefinitions,
    /// 同一额度组标识出现了多个定义。
    DuplicateQuotaGroupDefinition,
    /// 没有任何 Account 定义。
    EmptyAccountDefinitions,
    /// Account 定义数量超过固定上限。
    TooManyAccountDefinitions,
    /// 同一 Account 标识出现了多个定义。
    DuplicateAccountDefinition,
    /// 没有任何 Credential 定义。
    EmptyCredentialDefinitions,
    /// Credential 定义数量超过固定上限。
    TooManyCredentialDefinitions,
    /// 同一 Credential 标识出现了多个定义。
    DuplicateCredentialDefinition,
}

/// 借用式、固定上限的统一静态运行时定义输入。
///
/// 此类型只验证三类输入切片自身的形状；它们是否刚好覆盖一个编译快照的全部可达资源，
/// 必须由同代 `CompiledRoutingSnapshot` 的 crate 内布局工厂验证。三类定义必须作为同一
/// 不可变值传入，不能再由彼此独立的 factory 错配组合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionRuntimeDefinitions<'a> {
    quota_groups: &'a [QuotaGroupRuntimeDefinition],
    accounts: &'a [AccountRuntimeDefinition],
    credentials: &'a [CredentialRuntimeDefinition],
}

impl<'a> SelectionRuntimeDefinitions<'a> {
    /// 验证并借用固定上限内的 Account、Credential 与额度组静态定义。
    pub fn new(
        quota_groups: &'a [QuotaGroupRuntimeDefinition],
        accounts: &'a [AccountRuntimeDefinition],
        credentials: &'a [CredentialRuntimeDefinition],
    ) -> Result<Self, SelectionRuntimeDefinitionsError> {
        validate_quota_groups(quota_groups)?;
        validate_accounts(accounts)?;
        validate_credentials(credentials)?;
        Ok(Self {
            quota_groups,
            accounts,
            credentials,
        })
    }

    /// 返回已验证额度组定义的数量。
    pub const fn quota_group_len(&self) -> usize {
        self.quota_groups.len()
    }

    /// 返回已验证 Account 定义的数量。
    pub const fn account_len(&self) -> usize {
        self.accounts.len()
    }

    /// 返回已验证 Credential 定义的数量。
    pub const fn credential_len(&self) -> usize {
        self.credentials.len()
    }

    pub(crate) const fn quota_groups(&self) -> &'a [QuotaGroupRuntimeDefinition] {
        self.quota_groups
    }

    pub(crate) const fn accounts(&self) -> &'a [AccountRuntimeDefinition] {
        self.accounts
    }

    pub(crate) const fn credentials(&self) -> &'a [CredentialRuntimeDefinition] {
        self.credentials
    }

    pub(crate) fn quota_group(&self, id: QuotaGroupId) -> Option<QuotaGroupRuntimeDefinition> {
        self.quota_groups
            .iter()
            .copied()
            .find(|definition| definition.id() == id)
    }

    pub(crate) fn account(&self, id: AccountId) -> Option<AccountRuntimeDefinition> {
        self.accounts
            .iter()
            .copied()
            .find(|definition| definition.id() == id)
    }

    pub(crate) fn credential(&self, id: CredentialId) -> Option<CredentialRuntimeDefinition> {
        self.credentials
            .iter()
            .copied()
            .find(|definition| definition.id() == id)
    }
}

fn validate_quota_groups(
    definitions: &[QuotaGroupRuntimeDefinition],
) -> Result<(), SelectionRuntimeDefinitionsError> {
    if definitions.is_empty() {
        return Err(SelectionRuntimeDefinitionsError::EmptyQuotaGroupDefinitions);
    }
    if definitions.len() > MAX_TRACKED_QUOTA_GROUPS {
        return Err(SelectionRuntimeDefinitionsError::TooManyQuotaGroupDefinitions);
    }
    for (index, definition) in definitions.iter().enumerate() {
        if definitions[..index]
            .iter()
            .any(|previous| previous.id() == definition.id())
        {
            return Err(SelectionRuntimeDefinitionsError::DuplicateQuotaGroupDefinition);
        }
    }
    Ok(())
}

fn validate_accounts(
    definitions: &[AccountRuntimeDefinition],
) -> Result<(), SelectionRuntimeDefinitionsError> {
    if definitions.is_empty() {
        return Err(SelectionRuntimeDefinitionsError::EmptyAccountDefinitions);
    }
    if definitions.len() > MAX_TRACKED_ACCOUNTS {
        return Err(SelectionRuntimeDefinitionsError::TooManyAccountDefinitions);
    }
    for (index, definition) in definitions.iter().enumerate() {
        if definitions[..index]
            .iter()
            .any(|previous| previous.id() == definition.id())
        {
            return Err(SelectionRuntimeDefinitionsError::DuplicateAccountDefinition);
        }
    }
    Ok(())
}

fn validate_credentials(
    definitions: &[CredentialRuntimeDefinition],
) -> Result<(), SelectionRuntimeDefinitionsError> {
    if definitions.is_empty() {
        return Err(SelectionRuntimeDefinitionsError::EmptyCredentialDefinitions);
    }
    if definitions.len() > MAX_TRACKED_CREDENTIALS {
        return Err(SelectionRuntimeDefinitionsError::TooManyCredentialDefinitions);
    }
    for (index, definition) in definitions.iter().enumerate() {
        if definitions[..index]
            .iter()
            .any(|previous| previous.id() == definition.id())
        {
            return Err(SelectionRuntimeDefinitionsError::DuplicateCredentialDefinition);
        }
    }
    Ok(())
}

/// 将统一静态定义绑定到编译快照时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionRuntimeLayoutError {
    /// 解析目标并非由布局绑定的同一 RoutingSnapshot 实例产生。
    StaleSnapshot,
    /// 同一可达路由候选引用的 Selector 在已编译目录中缺失；理论不变量失效时保守拒绝。
    UnknownReachableSelector,
    /// 可达成员引用的 Account 在已编译目录中缺失；理论不变量失效时保守拒绝。
    UnknownReachableAccount,
    /// 可达成员引用的 Credential 在已编译目录中缺失；理论不变量失效时保守拒绝。
    UnknownReachableCredential,
    /// 所有可达 Selector 去重后引用的额度组超过固定上限。
    TooManyReachableGroups,
    /// 所有可达 Selector 去重后引用的 Account 超过固定上限。
    TooManyReachableAccounts,
    /// 所有可达 Selector 去重后引用的 Credential 超过固定上限。
    TooManyReachableCredentials,
    /// 一个可达额度组没有对应的静态运行时定义。
    MissingReachableQuotaGroupDefinition,
    /// 一个可达 Account 没有对应的静态运行时定义。
    MissingReachableAccountDefinition,
    /// 一个可达 Credential 没有对应的静态运行时定义。
    MissingReachableCredentialDefinition,
    /// 一个输入额度组定义未被任何可达 Selector 引用。
    UnusedQuotaGroupDefinition,
    /// 一个输入 Account 定义未被任何可达 Selector 引用。
    UnusedAccountDefinition,
    /// 一个输入 Credential 定义未被任何可达 Selector 引用。
    UnusedCredentialDefinition,
    /// 解析目标的 Selector 不包含调用方声明的额度单元。
    UnknownTargetUnit,
    /// 调用方声明的成员不属于解析目标的 Selector。
    UnknownTargetMember,
    /// 调用方声明的成员不属于声明的目标额度单元。
    MemberNotInTargetUnit,
    /// 已验证布局内缺少目标成员的 Account 定义；理论不变量失效时保守拒绝。
    MissingBoundAccountDefinition,
    /// 已验证布局内缺少目标成员的 Credential 定义；理论不变量失效时保守拒绝。
    MissingBoundCredentialDefinition,
    /// 已验证布局内缺少目标单元的额度组定义；理论不变量失效时保守拒绝。
    MissingBoundQuotaGroupDefinition,
}

/// 与唯一 `RoutingSnapshot` 实例绑定的只读统一静态布局。
///
/// 本类型只能由同代 [`CompiledRoutingSnapshot`](super::CompiledRoutingSnapshot) 创建，且只
/// 保存已经验证的借用定义。它不保存容量计数器、不创建 Lease，也不执行账户选择。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SelectionRuntimeLayout<'snapshot, 'config> {
    routing: &'snapshot RoutingSnapshot<'config>,
    definitions: SelectionRuntimeDefinitions<'config>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'snapshot, 'config> SelectionRuntimeLayout<'snapshot, 'config> {
    pub(crate) const fn new(
        routing: &'snapshot RoutingSnapshot<'config>,
        definitions: SelectionRuntimeDefinitions<'config>,
    ) -> Self {
        Self {
            routing,
            definitions,
        }
    }

    /// 为同一快照、同一目标 Selector 内的 Unit 与 Member 创建三资源静态 binding。
    ///
    /// 返回值只暴露 Account/Credential 标识及其上限，和该 Unit 全部额度组的受控迭代；
    /// 它不包含 Secret、选中结果、Lease、健康、游标或任何可写运行时状态。
    pub(crate) fn binding_for(
        &self,
        resolved: ResolvedRouteTarget<'snapshot, 'config>,
        unit: QuotaSelectionUnitId,
        member: AccountSelectorMember,
    ) -> Result<SelectionRuntimeBinding<'snapshot, 'config>, SelectionRuntimeLayoutError> {
        if !resolved.matches_snapshot(self.routing) {
            return Err(SelectionRuntimeLayoutError::StaleSnapshot);
        }
        let target_unit = resolved
            .selector()
            .unit(unit)
            .ok_or(SelectionRuntimeLayoutError::UnknownTargetUnit)?;
        if !resolved.selector().members().contains(&member) {
            return Err(SelectionRuntimeLayoutError::UnknownTargetMember);
        }
        if member.unit() != unit {
            return Err(SelectionRuntimeLayoutError::MemberNotInTargetUnit);
        }

        let account = self
            .definitions
            .account(member.account())
            .ok_or(SelectionRuntimeLayoutError::MissingBoundAccountDefinition)?;
        let credential = self
            .definitions
            .credential(member.credential())
            .ok_or(SelectionRuntimeLayoutError::MissingBoundCredentialDefinition)?;
        if target_unit
            .quota_groups()
            .iter()
            .any(|id| self.definitions.quota_group(*id).is_none())
        {
            return Err(SelectionRuntimeLayoutError::MissingBoundQuotaGroupDefinition);
        }

        Ok(SelectionRuntimeBinding {
            snapshot: PhantomData,
            account,
            credential,
            quota_groups: target_unit.quota_groups(),
            quota_group_definitions: self.definitions.quota_groups(),
        })
    }
}

/// 一个同代目标、额度单元与成员的只读三资源静态 binding。
///
/// 该值只能由 [`SelectionRuntimeLayout::binding_for`] 创建；它不选择账户、不会发放 Lease，
/// 也不包含凭据内容或任何可写运行时状态。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SelectionRuntimeBinding<'snapshot, 'config> {
    snapshot: PhantomData<&'snapshot RoutingSnapshot<'config>>,
    account: AccountRuntimeDefinition,
    credential: CredentialRuntimeDefinition,
    quota_groups: &'config [QuotaGroupId],
    quota_group_definitions: &'config [QuotaGroupRuntimeDefinition],
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'snapshot, 'config> SelectionRuntimeBinding<'snapshot, 'config> {
    /// 返回绑定 Account 的稳定标识。
    pub(crate) const fn account_id(&self) -> AccountId {
        self.account.id()
    }

    /// 返回绑定 Account 的正整数在途上限。
    pub(crate) const fn account_max_inflight(&self) -> NonZeroU16 {
        self.account.max_inflight()
    }

    /// 返回绑定 Credential 的稳定标识。
    pub(crate) const fn credential_id(&self) -> CredentialId {
        self.credential.id()
    }

    /// 返回绑定 Credential 的正整数在途上限。
    pub(crate) const fn credential_max_inflight(&self) -> NonZeroU16 {
        self.credential.max_inflight()
    }

    /// 零分配地按 Unit 声明顺序遍历全部额度组及其在途上限。
    pub(crate) fn quota_group_limits(&self) -> QuotaGroupRuntimeLimits<'config> {
        QuotaGroupRuntimeLimits {
            ids: self.quota_groups.iter(),
            definitions: self.quota_group_definitions,
        }
    }
}

/// 一个 binding 中全部额度组上限的零分配迭代器。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct QuotaGroupRuntimeLimits<'config> {
    ids: Iter<'config, QuotaGroupId>,
    definitions: &'config [QuotaGroupRuntimeDefinition],
}

#[cfg_attr(not(test), allow(dead_code))]
impl Iterator for QuotaGroupRuntimeLimits<'_> {
    type Item = (QuotaGroupId, NonZeroU16);

    fn next(&mut self) -> Option<Self::Item> {
        let id = *self.ids.next()?;
        self.definitions
            .iter()
            .copied()
            .find(|definition| definition.id() == id)
            .map(|definition| (id, definition.max_inflight()))
    }
}
