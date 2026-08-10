//! PriorityFailover 的固定容量选择与原子三资源 Lease。
//!
//! 本模块独占已验证的静态布局，只支持无等待的 `PriorityFailover`。它用固定数组和原子
//! 计数器取得 Account、Credential 与当前 Unit 全部 QuotaGroup 的容量；任何一步失败都会
//! 回滚，绝不把容量不足伪装成上游错误、429 或健康事件。

use core::sync::atomic::{AtomicU16, Ordering};

use super::{
    attempt::{
        AttemptCompletion, AttemptSuccessCompletion, AttemptTracker, TrustedPreExecutionRejection,
    },
    coordinator::AttemptPermit,
    selection_input::AccountSelectionEligibility,
    selection_runtime_layout::{SelectionRuntimeBinding, SelectionRuntimeLayout},
    CompiledRoutingSnapshot, CredentialSelectionPolicy, FailureClass, MAX_QUOTA_GROUPS_PER_UNIT,
    MAX_TRACKED_ACCOUNTS, MAX_TRACKED_CREDENTIALS, MAX_TRACKED_QUOTA_GROUPS,
};

/// Registry 激活被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionLeaseRegistryError {
    /// 传入编译快照并非布局绑定的同一 RoutingSnapshot 实例。
    LayoutSnapshotMismatch,
    /// 布局中的 Target 无法解析其选择合同；理论不变量失效时拒绝激活。
    UnknownLayoutSelector,
    /// 当前 C2 切片尚未实现该选择策略，禁止静默降级为优先故障转移。
    UnsupportedPolicy(CredentialSelectionPolicy),
}

/// 一次原子获取不能产生 Lease 的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LeaseAcquireError {
    /// 任一资源达到并发上限；已取得的资源已经完整回滚。
    CapacityUnavailable,
    /// binding 与 Registry 静态槽不一致；理论不变量失效时拒绝继续发送。
    InvalidBinding,
}

/// 一个固定容量的运行时资源槽。
struct CapacitySlot {
    id: u64,
    limit: u16,
    active: AtomicU16,
}

impl CapacitySlot {
    /// 单次无等待获取允许的最多 CAS 尝试次数。
    ///
    /// 达到上限时保守返回容量不可用，避免高竞争下无界自旋拉高本地路由 CPU。
    const MAX_CAS_ATTEMPTS: usize = 8;

    const fn empty() -> Self {
        Self {
            id: 0,
            limit: 0,
            active: AtomicU16::new(0),
        }
    }

    fn new(id: u64, limit: u16) -> Self {
        Self {
            id,
            limit,
            active: AtomicU16::new(0),
        }
    }

    fn try_acquire(&self) -> bool {
        for _ in 0..Self::MAX_CAS_ATTEMPTS {
            let active = self.active.load(Ordering::Acquire);
            if active >= self.limit {
                return false;
            }
            let next = active + 1;
            if self
                .active
                .compare_exchange_weak(active, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
        false
    }

    fn release(&self) {
        let previous = self.active.fetch_sub(1, Ordering::Release);
        debug_assert!(previous > 0, "Lease 释放前必须持有容量");
    }

    #[cfg(test)]
    fn inflight(&self) -> u16 {
        self.active.load(Ordering::Acquire)
    }
}

/// 用静态定义按声明顺序初始化固定容量槽。
fn capacity_slots<const N: usize>(
    definitions: impl Iterator<Item = (u64, u16)>,
) -> [CapacitySlot; N] {
    let mut slots = core::array::from_fn(|_| CapacitySlot::empty());
    for (slot, (id, limit)) in slots.iter_mut().zip(definitions) {
        *slot = CapacitySlot::new(id, limit);
    }
    slots
}

fn find_slot(slots: &[CapacitySlot], id: u64) -> Option<&CapacitySlot> {
    slots.iter().find(|slot| slot.id == id)
}

/// 固定容量、共享借用式的三资源 Lease。
///
/// Lease 不提供手动释放接口；它只可由 `SelectedAttempt` 线性持有，离开作用域时以反向
/// 取得顺序释放。它不会也不能替代 Coordinator 的显式 Attempt 完成。
#[must_use = "获取到的 Lease 必须由 SelectedAttempt 持有直至显式完成或离开作用域"]
struct SelectedLease<'registry> {
    account: &'registry CapacitySlot,
    credential: &'registry CapacitySlot,
    quota_groups: [Option<&'registry CapacitySlot>; MAX_QUOTA_GROUPS_PER_UNIT],
    quota_group_len: u8,
}

impl Drop for SelectedLease<'_> {
    fn drop(&mut self) {
        for slot in self.quota_groups[..usize::from(self.quota_group_len)]
            .iter()
            .rev()
            .flatten()
        {
            slot.release();
        }
        self.credential.release();
        self.account.release();
    }
}

/// 由 Registry 成功发放的、唯一可进入发送阶段的选择结果。
///
/// 它同时线性持有 Lease 和 Tracker。失败、取消与成功出口都会释放 Lease；直接 Drop 只
/// 回收未来容量，绝不伪造 Coordinator 完成或推进下一个 RouteTarget。
#[must_use = "SelectedAttempt 必须显式完成，或在放弃时仅由 Drop 回收容量"]
pub(crate) struct SelectedAttempt<'registry, 'snapshot, 'config> {
    lease: SelectedLease<'registry>,
    tracker: AttemptTracker<'snapshot, 'config>,
}

/// 消费 SelectedAttempt 后的成功转换结果。
///
/// 未完成分支必须返还完整 Lease+Tracker，因而其固定大小显著大于成功 token。这里刻意不用
/// `Result<_, SelectedAttempt>`：Clippy 要求为大错误值分配 Box，但这会破坏核心零堆分配
/// 合同；枚举使这一成本显式且始终在调用方栈上。
#[allow(clippy::large_enum_variant)]
pub(crate) enum SelectedAttemptSuccess<'registry, 'snapshot, 'config> {
    /// 已释放 Lease，并生成可由 Coordinator 消费的密封成功 token。
    Completed(AttemptSuccessCompletion),
    /// 响应前置条件不足，完整 Attempt 仍可继续记录或如实失败/取消。
    Incomplete(SelectedAttempt<'registry, 'snapshot, 'config>),
}

impl<'registry, 'snapshot, 'config> SelectedAttempt<'registry, 'snapshot, 'config> {
    /// 返回唯一 Attempt Tracker 的可变借用，供未来受控 Transport 记录发送事实。
    pub(crate) fn tracker_mut(&mut self) -> &mut AttemptTracker<'snapshot, 'config> {
        &mut self.tracker
    }

    /// 先释放 Lease，再生成原有失败完成对象。
    pub(crate) fn into_completion(
        self,
        failure: FailureClass,
        retry_after_ms: Option<u64>,
        trusted_rejection: Option<TrustedPreExecutionRejection<'snapshot, 'config>>,
    ) -> AttemptCompletion<'snapshot, 'config> {
        let Self { lease, tracker } = self;
        drop(lease);
        tracker.into_completion(failure, retry_after_ms, trusted_rejection)
    }

    /// 先释放 Lease，再生成取消完成对象。
    pub(crate) fn into_cancelled_completion(self) -> AttemptCompletion<'snapshot, 'config> {
        self.into_completion(FailureClass::Cancelled, None, None)
    }

    /// 将完整响应转换为密封成功对象。
    ///
    /// 仅当 Tracker 满足 C2-S0 的完整响应前置条件时才释放 Lease 并返回成功 token；否则
    /// 原样返还完整 `SelectedAttempt`，避免丢失 Tracker 导致 Coordinator 永久保持 active。
    pub(crate) fn into_success_completion(
        self,
    ) -> SelectedAttemptSuccess<'registry, 'snapshot, 'config> {
        let Self { lease, tracker } = self;
        match tracker.into_response_completed() {
            Ok(completion) => {
                drop(lease);
                SelectedAttemptSuccess::Completed(completion)
            }
            Err(tracker) => SelectedAttemptSuccess::Incomplete(Self { lease, tracker }),
        }
    }
}

/// 容量不足或无可选成员时的密封本地未发送拒绝。
///
/// 此 token 只由 Registry 在未创建 Tracker、未获得 Lease 的路径发放。Coordinator 消费它
/// 时直接推进 A → B，不读取 RetryGate、不写 Health、也不构造 `AttemptOutcome`。
pub(crate) struct SelectionLocalRejection<'snapshot, 'config> {
    permit: AttemptPermit<'snapshot, 'config>,
}

impl SelectionLocalRejection<'_, '_> {
    pub(crate) fn matches(
        &self,
        coordinator: super::coordinator::CoordinatorId,
        attempt: super::coordinator::AttemptId,
    ) -> bool {
        self.permit.belongs_to(coordinator) && self.permit.attempt_id() == attempt
    }
}

/// 静态来源或策略不变量失效时的密封本地停止 token。
///
/// 它不表示容量不足。Coordinator 消费后 fail closed，禁止把不完整 provenance 误推进到
/// 下一个 Target。
pub(crate) struct SelectionLocalStop<'snapshot, 'config> {
    permit: AttemptPermit<'snapshot, 'config>,
}

impl SelectionLocalStop<'_, '_> {
    pub(crate) fn matches(
        &self,
        coordinator: super::coordinator::CoordinatorId,
        attempt: super::coordinator::AttemptId,
    ) -> bool {
        self.permit.belongs_to(coordinator) && self.permit.attempt_id() == attempt
    }
}

/// PriorityFailover 选择的唯一下一步。
pub(crate) enum PrioritySelectionStart<'registry, 'snapshot, 'config> {
    /// 成功取得全部三类资源，可以开始受控发送。
    Selected(SelectedAttempt<'registry, 'snapshot, 'config>),
    /// 未发送的容量或 eligibility 拒绝，应由 Coordinator 直接推进后续 Target。
    Rejected(SelectionLocalRejection<'snapshot, 'config>),
    /// provenance、静态 binding 或策略不变量失效，应由 Coordinator fail closed。
    Stopped(SelectionLocalStop<'snapshot, 'config>),
}

/// 独占一个 `SelectionRuntimeLayout` 的固定容量 Registry。
///
/// 它不在 Planner 热路径运行，不分配、不等待、不启动任务、不读写网络或数据库。Lease 只
/// 共享借用本 Registry 的原子槽，因此多个网络 Attempt 不会因为 Rust 可变借用而被串行化。
pub(crate) struct SelectionLeaseRegistry<'snapshot, 'config> {
    layout: SelectionRuntimeLayout<'snapshot, 'config>,
    accounts: [CapacitySlot; MAX_TRACKED_ACCOUNTS],
    credentials: [CapacitySlot; MAX_TRACKED_CREDENTIALS],
    quota_groups: [CapacitySlot; MAX_TRACKED_QUOTA_GROUPS],
}

impl<'snapshot, 'config> SelectionLeaseRegistry<'snapshot, 'config> {
    /// 激活一个只支持 PriorityFailover 的固定容量 Registry。
    ///
    /// `WeightedLeastInflight` 与 `RoundRobinCompat` 在此明确拒绝，不能等到请求路径再降级。
    pub(crate) fn new(
        layout: SelectionRuntimeLayout<'snapshot, 'config>,
        compiled: &'snapshot CompiledRoutingSnapshot<'config>,
    ) -> Result<Self, SelectionLeaseRegistryError> {
        if !layout.matches_snapshot(compiled.routing()) {
            return Err(SelectionLeaseRegistryError::LayoutSnapshotMismatch);
        }
        for candidate in compiled.routing().candidates() {
            let policy = compiled
                .selection_policy_for(candidate.target().id())
                .ok_or(SelectionLeaseRegistryError::UnknownLayoutSelector)?;
            if policy != CredentialSelectionPolicy::PriorityFailover {
                return Err(SelectionLeaseRegistryError::UnsupportedPolicy(policy));
            }
        }

        let definitions = layout.definitions();
        Ok(Self {
            accounts: capacity_slots(
                definitions
                    .accounts()
                    .iter()
                    .map(|definition| (definition.id().get(), definition.max_inflight().get())),
            ),
            credentials: capacity_slots(
                definitions
                    .credentials()
                    .iter()
                    .map(|definition| (definition.id().get(), definition.max_inflight().get())),
            ),
            quota_groups: capacity_slots(
                definitions
                    .quota_groups()
                    .iter()
                    .map(|definition| (definition.id().get(), definition.max_inflight().get())),
            ),
            layout,
        })
    }

    /// 消费 Permit 与同代动态 eligibility，执行无等待的 PriorityFailover 选择。
    ///
    /// 优先层按数值升序，层内保留成员声明顺序。每个候选必须原子取得 Account、Credential
    /// 与当前 Unit 全部 QuotaGroup；容量不足仅尝试下一个成员，所有候选耗尽才产生本地拒绝。
    pub(crate) fn start_priority<'registry>(
        &'registry self,
        permit: AttemptPermit<'snapshot, 'config>,
        eligibility: AccountSelectionEligibility<'snapshot, 'config>,
    ) -> PrioritySelectionStart<'registry, 'snapshot, 'config> {
        let resolved = eligibility.request().resolved();
        if !self.layout.matches_snapshot(permit.snapshot())
            || !resolved.matches_snapshot(permit.snapshot())
            || resolved.target().id() != permit.target()
            || resolved.selector().policy() != CredentialSelectionPolicy::PriorityFailover
        {
            return PrioritySelectionStart::Stopped(SelectionLocalStop { permit });
        }

        let selector = resolved.selector();
        let mut previous_tier = None;
        loop {
            let mut next_tier = None;
            for (index, member) in selector.members().iter().copied().enumerate() {
                let Ok(is_eligible) = member_is_eligible(&eligibility, index, member) else {
                    return PrioritySelectionStart::Stopped(SelectionLocalStop { permit });
                };
                if is_eligible && previous_tier.is_none_or(|tier| member.priority_tier() > tier) {
                    next_tier = Some(next_tier.map_or(member.priority_tier(), |tier: u8| {
                        tier.min(member.priority_tier())
                    }));
                }
            }
            let Some(current_tier) = next_tier else {
                break;
            };

            for (index, member) in selector.members().iter().copied().enumerate() {
                let Ok(is_eligible) = member_is_eligible(&eligibility, index, member) else {
                    return PrioritySelectionStart::Stopped(SelectionLocalStop { permit });
                };
                if !is_eligible || member.priority_tier() != current_tier {
                    continue;
                }
                let binding = match self.layout.binding_for(resolved, member.unit(), member) {
                    Ok(binding) => binding,
                    Err(_) => {
                        return PrioritySelectionStart::Stopped(SelectionLocalStop { permit })
                    }
                };
                if !binding.matches_provenance(resolved, member.unit(), member)
                    || !binding.matches_attempt_target(permit.snapshot(), permit.target())
                {
                    return PrioritySelectionStart::Stopped(SelectionLocalStop { permit });
                }
                match self.acquire(binding) {
                    Ok(lease) => {
                        return PrioritySelectionStart::Selected(SelectedAttempt {
                            lease,
                            tracker: AttemptTracker::from_permit(permit),
                        });
                    }
                    Err(LeaseAcquireError::CapacityUnavailable) => continue,
                    Err(LeaseAcquireError::InvalidBinding) => {
                        return PrioritySelectionStart::Stopped(SelectionLocalStop { permit });
                    }
                }
            }
            previous_tier = Some(current_tier);
        }

        PrioritySelectionStart::Rejected(SelectionLocalRejection { permit })
    }

    fn acquire<'registry>(
        &'registry self,
        binding: SelectionRuntimeBinding<'snapshot, 'config>,
    ) -> Result<SelectedLease<'registry>, LeaseAcquireError> {
        let account = find_slot(&self.accounts, binding.account_id().get())
            .ok_or(LeaseAcquireError::InvalidBinding)?;
        let credential = find_slot(&self.credentials, binding.credential_id().get())
            .ok_or(LeaseAcquireError::InvalidBinding)?;
        if !account.try_acquire() {
            return Err(LeaseAcquireError::CapacityUnavailable);
        }
        if !credential.try_acquire() {
            account.release();
            return Err(LeaseAcquireError::CapacityUnavailable);
        }

        let mut quota_groups = [None; MAX_QUOTA_GROUPS_PER_UNIT];
        let mut quota_group_len = 0usize;
        let mut after_id = 0u64;
        loop {
            let next = binding
                .quota_group_limits()
                .filter_map(|(id, _)| (id.get() > after_id).then_some(id.get()))
                .min();
            let Some(id) = next else {
                break;
            };
            let Some(slot) = find_slot(&self.quota_groups, id) else {
                rollback(account, credential, &quota_groups[..quota_group_len]);
                return Err(LeaseAcquireError::InvalidBinding);
            };
            if !slot.try_acquire() {
                rollback(account, credential, &quota_groups[..quota_group_len]);
                return Err(LeaseAcquireError::CapacityUnavailable);
            }
            quota_groups[quota_group_len] = Some(slot);
            quota_group_len += 1;
            after_id = id;
        }
        if quota_group_len == 0 {
            rollback(account, credential, &quota_groups[..quota_group_len]);
            return Err(LeaseAcquireError::InvalidBinding);
        }

        Ok(SelectedLease {
            account,
            credential,
            quota_groups,
            quota_group_len: quota_group_len as u8,
        })
    }

    #[cfg(test)]
    pub(crate) fn account_inflight(&self, id: u64) -> Option<u16> {
        find_slot(&self.accounts, id).map(CapacitySlot::inflight)
    }

    #[cfg(test)]
    pub(crate) fn credential_inflight(&self, id: u64) -> Option<u16> {
        find_slot(&self.credentials, id).map(CapacitySlot::inflight)
    }

    #[cfg(test)]
    pub(crate) fn quota_group_inflight(&self, id: u64) -> Option<u16> {
        find_slot(&self.quota_groups, id).map(CapacitySlot::inflight)
    }
}

fn member_is_eligible(
    eligibility: &AccountSelectionEligibility<'_, '_>,
    member_index: usize,
    member: super::AccountSelectorMember,
) -> Result<bool, ()> {
    let Some(member_allowed) = eligibility.member_allowed_at(member_index as u8) else {
        return Err(());
    };
    if !member_allowed {
        return Ok(false);
    }
    let unit_index = eligibility
        .request()
        .resolved()
        .selector()
        .units()
        .iter()
        .position(|unit| unit.id() == member.unit())
        .ok_or(())?;
    eligibility.unit_allowed_at(unit_index as u8).ok_or(())
}

fn rollback(
    account: &CapacitySlot,
    credential: &CapacitySlot,
    quota_groups: &[Option<&CapacitySlot>],
) {
    for slot in quota_groups.iter().rev().flatten() {
        slot.release();
    }
    credential.release();
    account.release();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccountCredentialDefinitions, AccountDefinition, AccountRuntimeDefinition,
        AccountSelectorDefinition, CompiledRoutingSnapshot, CredentialDefinition,
        CredentialRuntimeDefinition, EndpointId, HealthRegistry, HealthTick, IngressClassifier,
        IngressRequest, ModelDeploymentDefinition, ModelDeploymentId, OperationId, QuotaGroupId,
        QuotaGroupRuntimeDefinition, QuotaSelectionUnit, QuotaSelectionUnitId, QuotaTopologySource,
        RetryPolicy, RouteCandidate, RoutePlanner, RouteStageId, RouteTarget, RouteTargetId,
        RoutingStrategy, SelectionRuntimeDefinitions, SelectionSession, SelectorAffinitySalt,
        SelectorRevision, SiteId, SnapshotVersion, VerifiedIngressDisposition,
    };

    fn target_id(value: u64) -> RouteTargetId {
        RouteTargetId::new(value).expect("测试 Target ID 非零")
    }

    fn selector_id(value: u64) -> crate::AccountSelectorId {
        crate::AccountSelectorId::new(value).expect("测试 Selector ID 非零")
    }

    fn account_id(value: u64) -> crate::AccountId {
        crate::AccountId::new(value).expect("测试 Account ID 非零")
    }

    fn credential_id(value: u64) -> crate::CredentialId {
        crate::CredentialId::new(value).expect("测试 Credential ID 非零")
    }

    fn group_id(value: u64) -> QuotaGroupId {
        QuotaGroupId::new(value).expect("测试 QuotaGroup ID 非零")
    }

    fn unit_id(value: u64) -> QuotaSelectionUnitId {
        QuotaSelectionUnitId::new(value).expect("测试 Unit ID 非零")
    }

    fn target(value: u64, selector: u64) -> RouteTarget {
        RouteTarget::new(
            target_id(value),
            SiteId::new(value).expect("测试 Site ID 非零"),
            ModelDeploymentId::new(value).expect("测试 Deployment ID 非零"),
            EndpointId::new(value).expect("测试 Endpoint ID 非零"),
            selector_id(selector),
        )
    }

    fn candidate(stage: u64, value: u64, selector: u64) -> RouteCandidate {
        RouteCandidate::ready(
            RouteStageId::new(stage).expect("测试 Stage ID 非零"),
            target(value, selector),
            0,
        )
    }

    fn deployment(value: u64) -> ModelDeploymentDefinition {
        ModelDeploymentDefinition::new(
            ModelDeploymentId::new(value).expect("测试 Deployment ID 非零"),
            SiteId::new(value).expect("测试 Site ID 非零"),
            EndpointId::new(value).expect("测试 Endpoint ID 非零"),
        )
    }

    fn account(value: u64, site: u64) -> AccountDefinition {
        AccountDefinition::new(
            account_id(value),
            SiteId::new(site).expect("测试 Site ID 非零"),
        )
    }

    fn credential(value: u64, owner: u64) -> CredentialDefinition {
        CredentialDefinition::new(credential_id(value), account_id(owner))
    }

    fn group_runtime(value: u64, limit: u16) -> QuotaGroupRuntimeDefinition {
        QuotaGroupRuntimeDefinition::new(
            group_id(value),
            core::num::NonZeroU16::new(limit).expect("测试额度组上限非零"),
        )
    }

    fn account_runtime(value: u64, limit: u16) -> AccountRuntimeDefinition {
        AccountRuntimeDefinition::new(
            account_id(value),
            core::num::NonZeroU16::new(limit).expect("测试账户上限非零"),
        )
    }

    fn credential_runtime(value: u64, limit: u16) -> CredentialRuntimeDefinition {
        CredentialRuntimeDefinition::new(
            credential_id(value),
            core::num::NonZeroU16::new(limit).expect("测试凭据上限非零"),
        )
    }

    fn selector<'a>(
        id: u64,
        policy: CredentialSelectionPolicy,
        topology: QuotaTopologySource,
        units: &'a [QuotaSelectionUnit<'a>],
        members: &'a [crate::AccountSelectorMember],
    ) -> AccountSelectorDefinition<'a> {
        AccountSelectorDefinition::new(
            selector_id(id),
            SelectorRevision::new(1).expect("测试 Selector revision 非零"),
            SelectorAffinitySalt::new([1; 16]),
            policy,
            topology,
            units,
            members,
        )
        .expect("测试 Selector 定义有效")
    }

    fn compiled<'a>(
        candidates: &'a [RouteCandidate],
        deployments: &'a [ModelDeploymentDefinition],
        accounts: &'a [AccountDefinition],
        credentials: &'a [CredentialDefinition],
        selectors: &'a [AccountSelectorDefinition<'a>],
    ) -> CompiledRoutingSnapshot<'a> {
        CompiledRoutingSnapshot::compile(
            SnapshotVersion::new(1).expect("测试快照版本非零"),
            candidates,
            RoutingStrategy::Priority,
            candidates.len() as u8,
            deployments,
            AccountCredentialDefinitions::new(accounts, credentials),
            selectors,
        )
        .expect("测试快照应编译成功")
    }

    fn route_eligibility<'snapshot, 'config>(
        snapshot: &'snapshot crate::RoutingSnapshot<'config>,
    ) -> crate::RouteEligibility<'snapshot, 'config> {
        HealthRegistry::new().eligibility_for(snapshot, HealthTick::new(0))
    }

    fn route_plan<'snapshot, 'config>(
        snapshot: &'snapshot crate::RoutingSnapshot<'config>,
        eligibility: &crate::RouteEligibility<'snapshot, 'config>,
    ) -> crate::RoutePlan<'snapshot, 'config> {
        let disposition = IngressClassifier::new()
            .classify(IngressRequest::routed(
                OperationId::CONVERSATION,
                snapshot.version(),
            ))
            .expect("测试路由请求可分类");
        let VerifiedIngressDisposition::Routed(dispatch) = disposition else {
            panic!("会话请求必须进入 Routed");
        };
        RoutePlanner::plan(&dispatch, snapshot, eligibility, 0).expect("测试计划有效")
    }

    fn selection_eligibility<'snapshot, 'config>(
        compiled: &'snapshot CompiledRoutingSnapshot<'config>,
        plan: &crate::RoutePlan<'snapshot, 'config>,
        unit_mask: u16,
        member_mask: u16,
    ) -> AccountSelectionEligibility<'snapshot, 'config> {
        let resolved = compiled
            .resolve_plan_target(plan, 0)
            .expect("计划与快照一致")
            .expect("首个 Target 存在");
        let request = compiled
            .selection_request(resolved, SelectionSession::Absent)
            .expect("同代选择请求有效");
        AccountSelectionEligibility::new(request, unit_mask, member_mask).expect("测试动态资格有效")
    }

    fn registry<'snapshot, 'config>(
        compiled: &'snapshot CompiledRoutingSnapshot<'config>,
        definitions: &SelectionRuntimeDefinitions<'config>,
    ) -> SelectionLeaseRegistry<'snapshot, 'config> {
        let layout = compiled
            .selection_runtime_layout(definitions)
            .expect("测试静态布局有效");
        SelectionLeaseRegistry::new(layout, compiled).expect("Priority Registry 应可激活")
    }

    #[test]
    fn registry_rejects_unimplemented_policies_at_activation() {
        let groups = [group_id(1)];
        let units = [QuotaSelectionUnit::new(
            unit_id(1),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [crate::AccountSelectorMember::new(
            account_id(1),
            credential_id(1),
            unit_id(1),
            0,
        )];
        let selectors = [selector(
            1,
            CredentialSelectionPolicy::WeightedLeastInflight,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )];
        let candidates = [candidate(1, 1, 1)];
        let deployments = [deployment(1)];
        let accounts = [account(1, 1)];
        let credentials = [credential(1, 1)];
        let compiled = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1)];
        let account_runtime = [account_runtime(1, 1)];
        let credential_runtime = [credential_runtime(1, 1)];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let layout = compiled
            .selection_runtime_layout(&definitions)
            .expect("测试布局有效");

        assert!(matches!(
            SelectionLeaseRegistry::new(layout, &compiled),
            Err(SelectionLeaseRegistryError::UnsupportedPolicy(
                CredentialSelectionPolicy::WeightedLeastInflight
            ))
        ));
    }

    #[test]
    fn priority_uses_lowest_tier_then_declaration_order_and_drop_releases() {
        let groups_one = [group_id(1)];
        let groups_two = [group_id(2)];
        let units = [
            QuotaSelectionUnit::new(
                unit_id(1),
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &groups_one,
            ),
            QuotaSelectionUnit::new(
                unit_id(2),
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &groups_two,
            ),
        ];
        let members = [
            crate::AccountSelectorMember::new(account_id(2), credential_id(2), unit_id(2), 1),
            crate::AccountSelectorMember::new(account_id(3), credential_id(3), unit_id(1), 0),
            crate::AccountSelectorMember::new(account_id(1), credential_id(1), unit_id(1), 0),
        ];
        let selectors = [selector(
            1,
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )];
        let candidates = [candidate(1, 1, 1)];
        let deployments = [deployment(1)];
        let accounts = [account(1, 1), account(2, 1), account(3, 1)];
        let credentials = [credential(1, 1), credential(2, 2), credential(3, 3)];
        let compiled = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1), group_runtime(2, 1)];
        let account_runtime = [
            account_runtime(1, 1),
            account_runtime(2, 1),
            account_runtime(3, 1),
        ];
        let credential_runtime = [
            credential_runtime(1, 1),
            credential_runtime(2, 1),
            credential_runtime(3, 1),
        ];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let registry = registry(&compiled, &definitions);
        let route_eligibility = route_eligibility(compiled.routing());
        let plan = route_plan(compiled.routing(), &route_eligibility);
        let account_eligibility = selection_eligibility(&compiled, &plan, 0b11, 0b111);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试重试策略有效"))
            .expect("测试 Coordinator 有效");
        let permit = coordinator
            .start(&route_eligibility)
            .expect("首个 Permit 可签发");
        let selected = match registry.start_priority(permit, account_eligibility) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("最低优先层的首个声明成员应被选中"),
        };

        assert_eq!(registry.account_inflight(3), Some(1));
        assert_eq!(registry.account_inflight(1), Some(0));
        assert_eq!(registry.account_inflight(2), Some(0));
        drop(selected);
        assert_eq!(registry.account_inflight(3), Some(0));
        assert!(coordinator.has_active_attempt());
    }

    #[test]
    fn priority_tries_lower_tier_after_higher_tier_capacity_exhaustion() {
        let groups_one = [group_id(1)];
        let groups_two = [group_id(2)];
        let units = [
            QuotaSelectionUnit::new(
                unit_id(1),
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &groups_one,
            ),
            QuotaSelectionUnit::new(
                unit_id(2),
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &groups_two,
            ),
        ];
        let members = [
            crate::AccountSelectorMember::new(account_id(1), credential_id(1), unit_id(1), 0),
            crate::AccountSelectorMember::new(account_id(2), credential_id(2), unit_id(2), 1),
        ];
        let selectors = [selector(
            1,
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )];
        let candidates = [candidate(1, 1, 1)];
        let deployments = [deployment(1)];
        let accounts = [account(1, 1), account(2, 1)];
        let credentials = [credential(1, 1), credential(2, 2)];
        let compiled = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1), group_runtime(2, 1)];
        let account_runtime = [account_runtime(1, 1), account_runtime(2, 1)];
        let credential_runtime = [credential_runtime(1, 1), credential_runtime(2, 1)];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let registry = registry(&compiled, &definitions);
        let route_eligibility = route_eligibility(compiled.routing());
        let plan = route_plan(compiled.routing(), &route_eligibility);
        let account_eligibility = selection_eligibility(&compiled, &plan, 0b11, 0b11);

        let mut high_holder = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("高优先层持有 Coordinator 有效");
        let held = match registry.start_priority(
            high_holder
                .start(&route_eligibility)
                .expect("持有 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("高优先层初始应取得 Lease"),
        };

        let mut fallback = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("次优先层 Coordinator 有效");
        let selected = match registry.start_priority(
            fallback
                .start(&route_eligibility)
                .expect("次优先层 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("高优先层容量满时应尝试较低优先层"),
        };
        assert_eq!(registry.account_inflight(1), Some(1));
        assert_eq!(registry.account_inflight(2), Some(1));
        drop(selected);
        drop(held);
    }

    #[test]
    fn failed_multi_group_acquisition_rolls_back_account_and_credential() {
        let groups = [group_id(2), group_id(1)];
        let units = [QuotaSelectionUnit::new(
            unit_id(1),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [crate::AccountSelectorMember::new(
            account_id(1),
            credential_id(1),
            unit_id(1),
            0,
        )];
        let selectors = [selector(
            1,
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )];
        let candidates = [candidate(1, 1, 1)];
        let deployments = [deployment(1)];
        let accounts = [account(1, 1)];
        let credentials = [credential(1, 1)];
        let compiled = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1), group_runtime(2, 2)];
        let account_runtime = [account_runtime(1, 2)];
        let credential_runtime = [credential_runtime(1, 2)];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let registry = registry(&compiled, &definitions);
        let route_eligibility = route_eligibility(compiled.routing());
        let plan = route_plan(compiled.routing(), &route_eligibility);
        let account_eligibility = selection_eligibility(&compiled, &plan, 1, 1);
        let mut first = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("第一个 Coordinator 有效");
        let first_selected = match registry.start_priority(
            first.start(&route_eligibility).expect("首个 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("首次应取得全部资源"),
        };
        let mut second = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("第二个 Coordinator 有效");
        assert!(matches!(
            registry.start_priority(
                second
                    .start(&route_eligibility)
                    .expect("第二个 Permit 有效"),
                account_eligibility,
            ),
            PrioritySelectionStart::Rejected(_)
        ));
        assert_eq!(registry.account_inflight(1), Some(1));
        assert_eq!(registry.credential_inflight(1), Some(1));
        assert_eq!(registry.quota_group_inflight(1), Some(1));
        assert_eq!(registry.quota_group_inflight(2), Some(1));
        drop(first_selected);
        assert_eq!(registry.account_inflight(1), Some(0));
        assert_eq!(registry.credential_inflight(1), Some(0));
        assert_eq!(registry.quota_group_inflight(1), Some(0));
        assert_eq!(registry.quota_group_inflight(2), Some(0));
    }

    #[test]
    fn local_capacity_rejection_advances_to_next_target_without_retry_gate() {
        let groups_one = [group_id(1)];
        let groups_two = [group_id(2)];
        let units_one = [QuotaSelectionUnit::new(
            unit_id(1),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups_one,
        )];
        let units_two = [QuotaSelectionUnit::new(
            unit_id(2),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups_two,
        )];
        let members_one = [crate::AccountSelectorMember::new(
            account_id(1),
            credential_id(1),
            unit_id(1),
            0,
        )];
        let members_two = [crate::AccountSelectorMember::new(
            account_id(2),
            credential_id(2),
            unit_id(2),
            0,
        )];
        let selectors = [
            selector(
                1,
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &units_one,
                &members_one,
            ),
            selector(
                2,
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &units_two,
                &members_two,
            ),
        ];
        let candidates = [candidate(1, 1, 1), candidate(2, 2, 2)];
        let deployments = [deployment(1), deployment(2)];
        let accounts = [account(1, 1), account(2, 2)];
        let credentials = [credential(1, 1), credential(2, 2)];
        let compiled = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1), group_runtime(2, 1)];
        let account_runtime = [account_runtime(1, 1), account_runtime(2, 1)];
        let credential_runtime = [credential_runtime(1, 1), credential_runtime(2, 1)];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let registry = registry(&compiled, &definitions);
        let route_eligibility = route_eligibility(compiled.routing());
        let plan = route_plan(compiled.routing(), &route_eligibility);
        let account_eligibility = selection_eligibility(&compiled, &plan, 1, 1);
        let mut holder = plan
            .into_attempt_coordinator(RetryPolicy::new(2, 0).expect("测试策略有效"))
            .expect("持有 Coordinator 有效");
        let held = match registry.start_priority(
            holder.start(&route_eligibility).expect("持有 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("首次应取得 A 的 Lease"),
        };
        let mut coordinator = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(2, 0).expect("测试策略有效"))
            .expect("故障转移 Coordinator 有效");
        let rejected = match registry.start_priority(
            coordinator
                .start(&route_eligibility)
                .expect("第二个 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Rejected(rejected) => rejected,
            _ => panic!("A 容量满必须产生本地拒绝"),
        };
        let crate::coordinator::CoordinatorStep::Next { permit, delay_ms } = coordinator
            .complete_local_rejection(rejected, &route_eligibility)
            .expect("本地拒绝可推进")
        else {
            panic!("容量拒绝必须推进 B");
        };
        assert_eq!(permit.target(), target_id(2));
        assert_eq!(delay_ms, 0);
        drop(held);
    }

    #[test]
    fn local_stop_rejects_foreign_eligibility_without_fallback() {
        let groups = [group_id(1)];
        let units = [QuotaSelectionUnit::new(
            unit_id(1),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [crate::AccountSelectorMember::new(
            account_id(1),
            credential_id(1),
            unit_id(1),
            0,
        )];
        let selectors = [selector(
            1,
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )];
        let candidates = [candidate(1, 1, 1)];
        let deployments = [deployment(1)];
        let accounts = [account(1, 1)];
        let credentials = [credential(1, 1)];
        let compiled_a = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let compiled_b = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1)];
        let account_runtime = [account_runtime(1, 1)];
        let credential_runtime = [credential_runtime(1, 1)];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let registry = registry(&compiled_a, &definitions);
        let route_eligibility_a = route_eligibility(compiled_a.routing());
        let plan_a = route_plan(compiled_a.routing(), &route_eligibility_a);
        let route_eligibility_b = route_eligibility(compiled_b.routing());
        let plan_b = route_plan(compiled_b.routing(), &route_eligibility_b);
        let foreign_eligibility = selection_eligibility(&compiled_b, &plan_b, 1, 1);
        let mut coordinator = plan_a
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("测试 Coordinator 有效");
        let stopped = match registry.start_priority(
            coordinator
                .start(&route_eligibility_a)
                .expect("测试 Permit 有效"),
            foreign_eligibility,
        ) {
            PrioritySelectionStart::Stopped(stop) => stop,
            _ => panic!("跨快照 eligibility 必须得到本地停止"),
        };

        assert!(matches!(
            coordinator
                .complete_local_stop(stopped)
                .expect("本地停止可消费"),
            crate::coordinator::CoordinatorStep::Stop(crate::RetryStopReason::EligibilityMismatch)
        ));
        assert!(coordinator.is_stopped());
    }

    #[test]
    fn atomic_slot_allows_at_most_one_winner_at_limit_one() {
        let slot = CapacitySlot::new(1, 1);
        let winner_count = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..16 {
                handles.push(scope.spawn(|| slot.try_acquire()));
            }
            handles
                .into_iter()
                .filter_map(|handle| handle.join().ok())
                .filter(|acquired| *acquired)
                .count()
        });

        assert_eq!(winner_count, 1);
        assert_eq!(slot.inflight(), 1);
        slot.release();
        assert_eq!(slot.inflight(), 0);
    }

    #[test]
    fn selected_attempt_releases_on_cancel_success_and_failed_success_conversion() {
        let groups = [group_id(1)];
        let units = [QuotaSelectionUnit::new(
            unit_id(1),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [crate::AccountSelectorMember::new(
            account_id(1),
            credential_id(1),
            unit_id(1),
            0,
        )];
        let selectors = [selector(
            1,
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )];
        let candidates = [candidate(1, 1, 1)];
        let deployments = [deployment(1)];
        let accounts = [account(1, 1)];
        let credentials = [credential(1, 1)];
        let compiled = compiled(
            &candidates,
            &deployments,
            &accounts,
            &credentials,
            &selectors,
        );
        let group_runtime = [group_runtime(1, 1)];
        let account_runtime = [account_runtime(1, 1)];
        let credential_runtime = [credential_runtime(1, 1)];
        let definitions =
            SelectionRuntimeDefinitions::new(&group_runtime, &account_runtime, &credential_runtime)
                .expect("测试定义有效");
        let registry = registry(&compiled, &definitions);
        let route_eligibility = route_eligibility(compiled.routing());
        let plan = route_plan(compiled.routing(), &route_eligibility);
        let account_eligibility = selection_eligibility(&compiled, &plan, 1, 1);

        let mut invalid_coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("测试 Coordinator 有效");
        let invalid_selected = match registry.start_priority(
            invalid_coordinator
                .start(&route_eligibility)
                .expect("Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("应取得 Lease"),
        };
        let SelectedAttemptSuccess::Incomplete(invalid_selected) =
            invalid_selected.into_success_completion()
        else {
            panic!("不完整响应不能生成成功 token");
        };
        assert_eq!(registry.account_inflight(1), Some(1));
        drop(invalid_selected);
        assert_eq!(registry.account_inflight(1), Some(0));
        assert!(invalid_coordinator.has_active_attempt());

        let mut success_coordinator = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("成功 Coordinator 有效");
        let mut success_selected = match registry.start_priority(
            success_coordinator
                .start(&route_eligibility)
                .expect("成功 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("成功路径应取得 Lease"),
        };
        success_selected
            .tracker_mut()
            .request_write_started()
            .expect("可记录写入");
        success_selected
            .tracker_mut()
            .upstream_response_observed()
            .expect("可记录响应");
        success_selected
            .tracker_mut()
            .downstream_committed()
            .expect("可记录下游提交");
        let success = match success_selected.into_success_completion() {
            SelectedAttemptSuccess::Completed(success) => success,
            SelectedAttemptSuccess::Incomplete(_) => panic!("完整响应可完成"),
        };
        assert_eq!(registry.account_inflight(1), Some(0));
        success_coordinator
            .complete_success(success)
            .expect("Coordinator 可成功终结");

        let mut cancelled_coordinator = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("取消 Coordinator 有效");
        let cancelled = match registry.start_priority(
            cancelled_coordinator
                .start(&route_eligibility)
                .expect("取消 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("取消路径应取得 Lease"),
        };
        let completion = cancelled.into_cancelled_completion();
        assert_eq!(registry.account_inflight(1), Some(0));
        assert!(matches!(
            cancelled_coordinator
                .complete(completion, &route_eligibility)
                .expect("取消完成可消费"),
            crate::coordinator::CoordinatorStep::Stop(crate::RetryStopReason::Cancelled)
        ));
    }

    #[test]
    fn fixed_state_remains_bounded() {
        assert!(core::mem::size_of::<SelectedAttempt<'_, '_, '_>>() <= 256);
        assert!(core::mem::size_of::<SelectionLeaseRegistry<'_, '_>>() <= 6 * 1024);
    }
}
