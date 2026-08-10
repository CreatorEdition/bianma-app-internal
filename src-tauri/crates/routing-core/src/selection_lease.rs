//! PriorityFailover 的固定容量选择与原子三资源 Lease。
//!
//! 本模块独占已验证的静态布局，只支持无等待的 `PriorityFailover`。它用固定数组和原子
//! 计数器取得 Account、Credential 与当前 Unit 全部 QuotaGroup 的容量；任何一步失败都会
//! 回滚，绝不把容量不足伪装成上游错误、429 或健康事件。

use core::sync::atomic::{AtomicU16, Ordering};

use super::{
    attempt::{
        AdapterRateLimitReporter, AdapterReplayReporter, AttemptCompletion,
        AttemptSuccessCompletion, AttemptTracker, AttemptTransitionError, RateLimitReporterError,
        ReplayReporterError, TrustedPreExecutionRejection,
    },
    coordinator::AttemptPermit,
    selection_input::AccountSelectionEligibility,
    selection_runtime_layout::{SelectionRuntimeBinding, SelectionRuntimeLayout},
    CompiledRoutingSnapshot, CredentialSelectionPolicy, FailureClass, ResolvedRouteTarget,
    MAX_QUOTA_GROUPS_PER_UNIT, MAX_TRACKED_ACCOUNTS, MAX_TRACKED_CREDENTIALS,
    MAX_TRACKED_QUOTA_GROUPS,
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

    fn release(&self) -> bool {
        self.active
            .fetch_update(Ordering::Release, Ordering::Acquire, |active| {
                active.checked_sub(1)
            })
            .is_ok()
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
            let _released = slot.release();
            debug_assert!(_released, "Lease 释放前必须持有额度组容量");
        }
        let _credential_released = self.credential.release();
        debug_assert!(_credential_released, "Lease 释放前必须持有凭据容量");
        let _account_released = self.account.release();
        debug_assert!(_account_released, "Lease 释放前必须持有账户容量");
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

/// Transport 交接后本地维护的终结安全见证。
///
/// 此状态只回答“交接终结时能否保留 `NotSent`”，不等同于上游实际字节写出状态。下游提交
/// 是独立的不可重放证据：它可以在未记录首字节时发生，但仍必须跳过 `Unknown` 封口并由
/// Tracker/RetryGate 终结为 `Sent` / `DownstreamCommitted`。
#[derive(Clone, Copy, Eq, PartialEq)]
enum TransportTerminalSafety {
    NoBytesProven,
    BytesWritten,
    Unknown,
    DownstreamCommitted,
}

/// 由 [`SelectedAttempt`] 唯一交接给未来受控 Transport 的线性尝试。
///
/// 它按值持有 Lease 与 Tracker，但不暴露裸 Tracker、Lease、Secret 或 Session。Transport
/// 只能通过此包装器单调记录写出、响应和下游提交；失败、取消或不完整成功后的终结默认会
/// 保守封口未证明的写出状态，避免交接后的任何错误被错误重放。唯一例外是既有的密封受信
/// 执行前拒绝收据：它仍必须由 Tracker 逐项校验 Attempt、Target 与快照绑定后，才可保留
/// `PreExecutionRejected` 的重试语义。
#[must_use = "Transport 交接后的 Attempt 必须显式终结，或在放弃时仅由 Drop 回收 Lease"]
pub(crate) struct TransportHandoffAttempt<'registry, 'snapshot, 'config> {
    lease: SelectedLease<'registry>,
    tracker: AttemptTracker<'snapshot, 'config>,
    terminal_safety: TransportTerminalSafety,
}

/// 消费 Transport 交接尝试后的成功转换结果。
///
/// 未完成分支必须返还完整 Lease+Tracker，因而其固定大小显著大于成功 token。这里刻意不用
/// `Result<_, TransportHandoffAttempt>`：Clippy 要求为大错误值分配 Box，但这会破坏核心零堆分配
/// 合同；枚举使这一成本显式且始终在调用方栈上。
#[allow(clippy::large_enum_variant)]
pub(crate) enum TransportHandoffSuccess<'registry, 'snapshot, 'config> {
    /// 已释放 Lease，并生成可由 Coordinator 消费的密封成功 token。
    Completed(AttemptSuccessCompletion),
    /// 响应前置条件不足，完整交接 Attempt 仍可继续记录或如实失败/取消。
    Incomplete(TransportHandoffAttempt<'registry, 'snapshot, 'config>),
}

impl<'registry, 'snapshot, 'config> SelectedAttempt<'registry, 'snapshot, 'config> {
    /// 消费选择结果并创建唯一的 Transport 交接包装器。
    ///
    /// 交接后不再存在 crate-wide 的裸 Tracker 或直接完成入口。若 Transport 未能通过受控
    /// writer 证明首字节写出，包装器的失败/取消出口会先记录 `Unknown`，从而禁止自动重放。
    pub(crate) fn into_transport_handoff(
        self,
    ) -> TransportHandoffAttempt<'registry, 'snapshot, 'config> {
        let Self { lease, tracker } = self;
        TransportHandoffAttempt {
            lease,
            tracker,
            terminal_safety: TransportTerminalSafety::NoBytesProven,
        }
    }
}

impl<'registry, 'snapshot, 'config> TransportHandoffAttempt<'registry, 'snapshot, 'config> {
    /// 记录受控 writer 已写出首个请求字节。
    pub(crate) fn request_write_started(&mut self) -> Result<(), AttemptTransitionError> {
        self.tracker.request_write_started()?;
        self.terminal_safety = TransportTerminalSafety::BytesWritten;
        Ok(())
    }

    /// 记录 writer 无法继续证明请求写出状态。
    pub(crate) fn write_state_unknown(&mut self) -> Result<(), AttemptTransitionError> {
        self.tracker.write_state_unknown()?;
        self.terminal_safety = TransportTerminalSafety::Unknown;
        Ok(())
    }

    /// 记录已经观察到上游响应头。
    pub(crate) fn upstream_response_observed(&mut self) -> Result<(), AttemptTransitionError> {
        self.tracker.upstream_response_observed()
    }

    /// 记录首个经协议确认的流式语义事件。
    pub(crate) fn first_semantic_event_observed(&mut self) -> Result<(), AttemptTransitionError> {
        self.tracker.first_semantic_event_observed()
    }

    /// 记录响应语义已经向下游提交。
    pub(crate) fn downstream_committed(&mut self) -> Result<(), AttemptTransitionError> {
        self.tracker.downstream_committed()?;
        self.terminal_safety = TransportTerminalSafety::DownstreamCommitted;
        Ok(())
    }

    /// 为当前交接 Attempt 签发一次受信限流上报器。
    pub(crate) fn rate_limit_reporter(
        &mut self,
        resolved: ResolvedRouteTarget<'_, '_>,
    ) -> Result<AdapterRateLimitReporter, RateLimitReporterError> {
        self.tracker.rate_limit_reporter(resolved)
    }

    /// 为当前交接 Attempt 签发一次受信执行前拒绝上报器。
    pub(crate) fn replay_reporter(
        &mut self,
        resolved: ResolvedRouteTarget<'_, 'config>,
    ) -> Result<AdapterReplayReporter<'snapshot, 'config>, ReplayReporterError> {
        self.tracker.replay_reporter(resolved)
    }

    /// 释放 Lease 并生成失败完成对象。
    ///
    /// 默认先保守封口未证明的写出状态，令交接后的裸 429、超时、连接错误和服务端错误都
    /// 只能得到 `Unknown`。只有已提供的密封受信执行前拒绝收据才跳过这一默认封口，并仍由
    /// Tracker 校验其 Attempt、Target 与快照绑定；无效收据或任何已写出/下游提交证据都会
    /// 在 Tracker 中 fail closed，不能恢复可重放结论。
    pub(crate) fn into_completion(
        mut self,
        failure: FailureClass,
        retry_after_ms: Option<u64>,
        trusted_rejection: Option<TrustedPreExecutionRejection<'snapshot, 'config>>,
    ) -> AttemptCompletion<'snapshot, 'config> {
        if trusted_rejection.is_none() {
            self.seal_unproven_write_before_terminal();
        }
        let Self { lease, tracker, .. } = self;
        drop(lease);
        tracker.into_completion(failure, retry_after_ms, trusted_rejection)
    }

    /// 先保守封口未证明的写出状态，再生成取消完成对象。
    pub(crate) fn into_cancelled_completion(self) -> AttemptCompletion<'snapshot, 'config> {
        self.into_completion(FailureClass::Cancelled, None, None)
    }

    /// 将完整响应转换为密封成功对象。
    ///
    /// 仅当 Tracker 满足 C2-S0 的完整响应前置条件时才释放 Lease 并返回成功 token；否则
    /// 原样返还完整交接包装器，避免绕回 `SelectedAttempt` 后丢失 Transport 封口。
    pub(crate) fn into_success_completion(
        self,
    ) -> TransportHandoffSuccess<'registry, 'snapshot, 'config> {
        let Self {
            lease,
            tracker,
            terminal_safety,
        } = self;
        match tracker.into_response_completed() {
            Ok(completion) => {
                drop(lease);
                TransportHandoffSuccess::Completed(completion)
            }
            Err(tracker) => TransportHandoffSuccess::Incomplete(Self {
                lease,
                tracker,
                terminal_safety,
            }),
        }
    }

    /// 在交接后的非成功终结前，阻断任何未记录首字节写出的可重放结论。
    fn seal_unproven_write_before_terminal(&mut self) {
        if self.terminal_safety != TransportTerminalSafety::NoBytesProven {
            return;
        }
        self.tracker
            .write_state_unknown()
            .expect("交接包装器的未证明写出状态必须可保守封口");
        self.terminal_safety = TransportTerminalSafety::Unknown;
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
            let _account_released = account.release();
            debug_assert!(_account_released, "已取得账户容量必须能够回滚");
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
        let _released = slot.release();
        debug_assert!(_released, "已取得额度组容量必须能够回滚");
    }
    let _credential_released = credential.release();
    debug_assert!(_credential_released, "已取得凭据容量必须能够回滚");
    let _account_released = account.release();
    debug_assert!(_account_released, "已取得账户容量必须能够回滚");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccountCredentialDefinitions, AccountDefinition, AccountRuntimeDefinition,
        AccountSelectorDefinition, CompiledRoutingSnapshot, CredentialDefinition,
        CredentialRuntimeDefinition, DeliveryState, EndpointId, FailureClass, HealthRegistry,
        HealthTick, IngressClassifier, IngressRequest, ModelDeploymentDefinition,
        ModelDeploymentId, OperationId, QuotaGroupId, QuotaGroupRuntimeDefinition,
        QuotaSelectionUnit, QuotaSelectionUnitId, QuotaTopologySource, RetryPolicy, RouteCandidate,
        RoutePlanner, RouteStageId, RouteTarget, RouteTargetId, RoutingStrategy,
        SelectionRuntimeDefinitions, SelectionSession, SelectorAffinitySalt, SelectorRevision,
        SiteId, SnapshotVersion, VerifiedIngressDisposition,
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
        assert!(slot.release());
        assert_eq!(slot.inflight(), 0);
    }

    #[test]
    fn zero_capacity_release_never_wraps_counter() {
        let slot = CapacitySlot::new(1, 1);

        assert!(!slot.release());
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
        let invalid_handoff = invalid_selected.into_transport_handoff();
        let TransportHandoffSuccess::Incomplete(invalid_handoff) =
            invalid_handoff.into_success_completion()
        else {
            panic!("不完整响应不能生成成功 token");
        };
        assert_eq!(registry.account_inflight(1), Some(1));
        let invalid_completion = invalid_handoff.into_cancelled_completion();
        assert_eq!(registry.account_inflight(1), Some(0));
        assert_eq!(
            invalid_completion.outcome().delivery(),
            DeliveryState::Unknown
        );
        assert!(matches!(
            invalid_coordinator
                .complete(invalid_completion, &route_eligibility)
                .expect("交接后取消完成可消费"),
            crate::coordinator::CoordinatorStep::Stop(crate::RetryStopReason::Cancelled)
        ));

        let mut success_coordinator = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("成功 Coordinator 有效");
        let success_resolved = compiled
            .resolve_plan_target(&route_plan(compiled.routing(), &route_eligibility), 0)
            .expect("成功计划与快照一致")
            .expect("成功 Target 存在");
        let success_selected = match registry.start_priority(
            success_coordinator
                .start(&route_eligibility)
                .expect("成功 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("成功路径应取得 Lease"),
        };
        let mut success_handoff = success_selected.into_transport_handoff();
        let _rate_limit_reporter = success_handoff
            .rate_limit_reporter(success_resolved)
            .expect("同代 Target 可签发限流上报器");
        let _replay_reporter = success_handoff
            .replay_reporter(success_resolved)
            .expect("同代 Target 可签发回放上报器");
        success_handoff.request_write_started().expect("可记录写入");
        success_handoff
            .upstream_response_observed()
            .expect("可记录响应");
        success_handoff
            .first_semantic_event_observed()
            .expect("可记录首个语义事件");
        success_handoff
            .downstream_committed()
            .expect("可记录下游提交");
        let success = match success_handoff.into_success_completion() {
            TransportHandoffSuccess::Completed(success) => success,
            TransportHandoffSuccess::Incomplete(_) => panic!("完整响应可完成"),
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
        let completion = cancelled
            .into_transport_handoff()
            .into_cancelled_completion();
        assert_eq!(registry.account_inflight(1), Some(0));
        assert_eq!(completion.outcome().delivery(), DeliveryState::Unknown);
        assert!(matches!(
            cancelled_coordinator
                .complete(completion, &route_eligibility)
                .expect("取消完成可消费"),
            crate::coordinator::CoordinatorStep::Stop(crate::RetryStopReason::Cancelled)
        ));

        let mut downstream_coordinator = route_plan(compiled.routing(), &route_eligibility)
            .into_attempt_coordinator(RetryPolicy::new(1, 0).expect("测试策略有效"))
            .expect("下游提交 Coordinator 有效");
        let downstream_selected = match registry.start_priority(
            downstream_coordinator
                .start(&route_eligibility)
                .expect("下游提交 Permit 有效"),
            account_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("下游提交路径应取得 Lease"),
        };
        let mut downstream_handoff = downstream_selected.into_transport_handoff();
        downstream_handoff
            .downstream_committed()
            .expect("允许记录不带首字节见证的下游提交");
        let downstream_completion =
            downstream_handoff.into_completion(FailureClass::Server, None, None);
        assert_eq!(
            downstream_completion.outcome().delivery(),
            DeliveryState::Sent
        );
        assert_eq!(registry.account_inflight(1), Some(0));
        assert!(matches!(
            downstream_coordinator
                .complete(downstream_completion, &route_eligibility)
                .expect("下游提交完成可消费"),
            crate::coordinator::CoordinatorStep::Stop(crate::RetryStopReason::DownstreamCommitted)
        ));
    }

    #[test]
    fn transport_handoff_unproven_failures_never_issue_next_permit() {
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

        for (failure, mark_write_started, mark_write_unknown) in [
            (FailureClass::RateLimited, false, false),
            (FailureClass::Server, false, false),
            (FailureClass::Connect, false, false),
            (FailureClass::Server, false, true),
            (FailureClass::RateLimited, true, false),
        ] {
            let plan = route_plan(compiled.routing(), &route_eligibility);
            let account_eligibility = selection_eligibility(&compiled, &plan, 1, 1);
            let mut coordinator = plan
                .into_attempt_coordinator(RetryPolicy::new(2, 100).expect("测试策略有效"))
                .expect("测试 Coordinator 有效");
            let selected = match registry.start_priority(
                coordinator
                    .start(&route_eligibility)
                    .expect("测试 Permit 有效"),
                account_eligibility,
            ) {
                PrioritySelectionStart::Selected(selected) => selected,
                _ => panic!("首 Target 应取得 Lease"),
            };
            let mut handoff = selected.into_transport_handoff();
            if mark_write_started {
                handoff.request_write_started().expect("可记录写入首字节");
            }
            if mark_write_unknown {
                handoff.write_state_unknown().expect("可记录未知写出状态");
            }
            let completion = handoff.into_completion(failure, Some(500), None);
            assert_eq!(
                completion.outcome().delivery(),
                if mark_write_started {
                    DeliveryState::Sent
                } else {
                    DeliveryState::Unknown
                }
            );
            assert!(matches!(
                coordinator
                    .complete(completion, &route_eligibility)
                    .expect("交接后完成可消费"),
                crate::coordinator::CoordinatorStep::Stop(crate::RetryStopReason::ReplayNotProven)
            ));
        }

        let receipt_plan = route_plan(compiled.routing(), &route_eligibility);
        let receipt_resolved = compiled
            .resolve_plan_target(&receipt_plan, 0)
            .expect("受信拒绝计划与快照一致")
            .expect("受信拒绝首个 Target 存在");
        let receipt_eligibility = selection_eligibility(&compiled, &receipt_plan, 1, 1);
        let mut receipt_coordinator = receipt_plan
            .into_attempt_coordinator(RetryPolicy::new(2, 100).expect("测试策略有效"))
            .expect("测试 Coordinator 有效");
        let receipt_selected = match registry.start_priority(
            receipt_coordinator
                .start(&route_eligibility)
                .expect("受信拒绝 Permit 有效"),
            receipt_eligibility,
        ) {
            PrioritySelectionStart::Selected(selected) => selected,
            _ => panic!("受信拒绝路径应取得首个 Target 的 Lease"),
        };
        let mut receipt_handoff = receipt_selected.into_transport_handoff();
        let receipt = receipt_handoff
            .replay_reporter(receipt_resolved)
            .expect("同代 Target 可签发回放上报器")
            .pre_execution_rejected(
                crate::attempt::VerifiedPreExecutionContract::test_only_registered(
                    receipt_resolved.target().site(),
                    0x1001,
                )
                .expect("测试合同已登记"),
            )
            .expect("受信合同可签发执行前拒绝收据");
        let receipt_completion =
            receipt_handoff.into_completion(FailureClass::RateLimited, Some(500), Some(receipt));
        assert_eq!(
            receipt_completion.outcome().delivery(),
            DeliveryState::PreExecutionRejected
        );
        let crate::coordinator::CoordinatorStep::Next { permit, .. } = receipt_coordinator
            .complete(receipt_completion, &route_eligibility)
            .expect("受信执行前拒绝完成可消费")
        else {
            panic!("受信执行前拒绝必须签发计划内下一 Permit");
        };
        assert_eq!(permit.target(), target_id(2));
    }

    #[test]
    fn fixed_state_remains_bounded() {
        assert!(core::mem::size_of::<SelectedAttempt<'_, '_, '_>>() <= 256);
        assert!(core::mem::size_of::<TransportHandoffAttempt<'_, '_, '_>>() <= 256);
        assert!(core::mem::size_of::<SelectionLeaseRegistry<'_, '_>>() <= 6 * 1024);
    }
}
