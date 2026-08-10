//! 单次 Attempt 的线性协调器。
//!
//! Coordinator 消费不可变 [`RoutePlan`]，只签发一个活跃许可；一次
//! 完成要么原子地签发下一个计划位置，要么永久终止。它不执行网络 I/O、等待或健康更新。

use core::fmt;
use core::sync::atomic::{AtomicU64, Ordering};

use super::{
    attempt::{AttemptCompletion, AttemptSuccessCompletion},
    selection_lease::{SelectionLocalRejection, SelectionLocalStop},
    RetryDecision, RetryGate, RetryPolicy, RetryStopReason, RouteEligibility, RoutePlan,
    RouteTargetId, RoutingSnapshot,
};

/// 进程内只增不复用的 Coordinator 标识分配器。
///
/// 唯一性只用于拒绝将另一请求的 Completion 交给当前 Coordinator；达到理论上限时新
/// Coordinator 会保持停止状态，而不会复用旧标识。
static NEXT_COORDINATOR_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CoordinatorId(u64);

fn allocate_coordinator_id() -> Option<CoordinatorId> {
    NEXT_COORDINATOR_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .ok()
        .map(CoordinatorId)
}

/// 单个 Coordinator 请求内的稳定 Attempt 标识。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttemptId(u8);

impl AttemptId {
    const fn new(value: u8) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }
}

/// Coordinator 签发的一次 Attempt 许可。
///
/// 许可没有公开构造器，也刻意不实现 `Clone` 或 `Copy`。它必须先被
/// [`AttemptTracker`](super::attempt::AttemptTracker) 消费，再由私有 Completion 交给
/// [`AttemptCoordinator::complete`]；调用方不能保留许可并制造第二次发送。
pub(crate) struct AttemptPermit<'snapshot, 'candidates> {
    coordinator: CoordinatorId,
    id: AttemptId,
    index: u8,
    snapshot: &'snapshot RoutingSnapshot<'candidates>,
    candidate_index: u8,
}

impl fmt::Debug for AttemptPermit<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AttemptPermit")
            .field("coordinator", &self.coordinator)
            .field("id", &self.id)
            .field("index", &self.index)
            .field("target", &self.target())
            .finish()
    }
}

impl<'snapshot, 'candidates> AttemptPermit<'snapshot, 'candidates> {
    /// 返回计划内的单调尝试位置。
    pub(crate) const fn index(&self) -> u8 {
        self.index
    }

    /// 返回本次唯一绑定的路由目标。
    pub(crate) fn target(&self) -> RouteTargetId {
        self.snapshot.candidates[usize::from(self.candidate_index)]
            .target()
            .id()
    }

    /// 返回许可绑定的唯一快照实例。
    pub(crate) const fn snapshot(&self) -> &'snapshot RoutingSnapshot<'candidates> {
        self.snapshot
    }

    pub(crate) const fn attempt_id(&self) -> AttemptId {
        self.id
    }

    /// 返回本次 Attempt 绑定的内部 Coordinator 标识。
    ///
    /// 此方法仅供 Attempt 收据校验使用，不对 crate 外暴露构造或伪造路径。
    pub(crate) const fn coordinator_id(&self) -> CoordinatorId {
        self.coordinator
    }

    pub(crate) fn belongs_to(&self, coordinator: CoordinatorId) -> bool {
        self.coordinator == coordinator
    }

    #[cfg(test)]
    pub(crate) fn test_only(id: u8) -> AttemptPermit<'static, 'static> {
        let target = RouteTargetId::new(u64::from(id)).expect("测试目标标识非零");
        let candidates = Box::leak(Box::new([super::RouteCandidate::ready(
            super::RouteStageId::new(1).expect("测试阶段标识非零"),
            super::RouteTarget::new(
                target,
                super::SiteId::new(1).expect("测试站点标识非零"),
                super::ModelDeploymentId::new(1).expect("测试部署标识非零"),
                super::EndpointId::new(1).expect("测试端点标识非零"),
                super::AccountSelectorId::new(1).expect("测试选择合同标识非零"),
            ),
            0,
        )]));
        let snapshot = Box::leak(Box::new(
            RoutingSnapshot::new(
                super::SnapshotVersion::new(1).expect("测试快照版本非零"),
                candidates,
                super::RoutingStrategy::Priority,
                1,
            )
            .expect("测试快照有效"),
        ));
        AttemptPermit {
            coordinator: CoordinatorId(1),
            id: AttemptId::new(id).expect("测试 Attempt 标识非零"),
            index: id.saturating_sub(1),
            snapshot,
            candidate_index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveAttempt {
    id: AttemptId,
    index: u8,
}

/// 尝试开始被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttemptStartError {
    /// 上一次 Attempt 尚未完成。
    ActiveAttempt,
    /// 已停止、耗尽或检测到内部不一致。
    Stopped,
    /// 传入的 eligibility 未与当前计划同代匹配。
    EligibilityMismatch,
    /// 当前 eligibility 下没有可签发的计划内目标。
    NoEligibleTargets,
}

/// 创建协调器被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttemptCoordinatorBuildError {
    /// 重试预算不足以消费已编译计划中的每个 Stage。
    InsufficientAttemptBudget,
}

/// 尝试完成被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttemptCompleteError {
    /// 当前没有可完成的活跃 Attempt。
    NoActive,
    /// Completion 不属于当前活跃 Attempt；协调器随即 fail closed。
    Mismatched,
}

/// 完成一次 Attempt 后的唯一下一步。
#[derive(Debug)]
pub(crate) enum CoordinatorStep<'snapshot, 'candidates> {
    /// 已原子签发下一个计划位置，调用方只能消费返回的唯一许可继续。
    Next {
        /// 下一个单次 Attempt 许可。
        permit: AttemptPermit<'snapshot, 'candidates>,
        /// 在开始该 Attempt 前的有界等待时间。
        delay_ms: u64,
    },
    /// 当前请求永久停止自动推进。
    Stop(RetryStopReason),
}

/// 当前请求已由明确的成功完成对象终结。
///
/// 该零大小私有标记只由 [`AttemptCoordinator::complete_success`] 返回；它不包含下一次
/// Permit、重试信息或发送结果，防止成功路径被误接到故障转移。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CompletedAttempt;

/// 消费一个 RoutePlan、限制 Attempt 线性推进的固定大小状态机。
pub(crate) struct AttemptCoordinator<'snapshot, 'candidates> {
    plan: RoutePlan<'snapshot, 'candidates>,
    gate: RetryGate,
    coordinator: Option<CoordinatorId>,
    next_index: u8,
    next_id: u8,
    active: Option<ActiveAttempt>,
    stopped: bool,
}

impl<'snapshot, 'candidates> AttemptCoordinator<'snapshot, 'candidates> {
    pub(crate) fn new(
        plan: RoutePlan<'snapshot, 'candidates>,
        policy: RetryPolicy,
    ) -> Result<Self, AttemptCoordinatorBuildError> {
        if policy.max_attempts() < plan.len() {
            return Err(AttemptCoordinatorBuildError::InsufficientAttemptBudget);
        }
        let coordinator = allocate_coordinator_id();
        Ok(Self {
            plan,
            gate: RetryGate::new(policy),
            coordinator,
            next_index: 0,
            next_id: 1,
            active: None,
            stopped: coordinator.is_none(),
        })
    }

    /// 返回是否已有永久停止结论。
    pub(crate) const fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// 返回是否已有尚未完成的 Attempt。
    pub(crate) const fn has_active_attempt(&self) -> bool {
        self.active.is_some()
    }

    /// 使用当前 eligibility 签发初始 Attempt；后续 Attempt 只能由 [`Self::complete`] 原子签发。
    pub(crate) fn start(
        &mut self,
        eligibility: &RouteEligibility<'snapshot, 'candidates>,
    ) -> Result<AttemptPermit<'snapshot, 'candidates>, AttemptStartError> {
        if self.stopped {
            return Err(AttemptStartError::Stopped);
        }
        if self.active.is_some() {
            return Err(AttemptStartError::ActiveAttempt);
        }
        match self.issue_next(eligibility) {
            Err(()) => {
                self.stopped = true;
                Err(AttemptStartError::EligibilityMismatch)
            }
            Ok(Some(permit)) => Ok(permit),
            Ok(None) => {
                self.stopped = true;
                Err(AttemptStartError::NoEligibleTargets)
            }
        }
    }

    /// 消费匹配的 Completion，返回唯一的后续许可或永久停止结论。
    pub(crate) fn complete(
        &mut self,
        completion: AttemptCompletion<'snapshot, 'candidates>,
        eligibility: &RouteEligibility<'snapshot, 'candidates>,
    ) -> Result<CoordinatorStep<'snapshot, 'candidates>, AttemptCompleteError> {
        let Some(active) = self.active else {
            return Err(AttemptCompleteError::NoActive);
        };
        let Some(coordinator) = self.coordinator else {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        };
        if !completion.matches(coordinator, active.id) {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        }

        let decision = self
            .gate
            .decide(&self.plan, active.index, completion.outcome());
        self.active = None;
        match decision {
            RetryDecision::Advance { delay_ms } => {
                self.next_index = active.index.saturating_add(1);
                match self.issue_next(eligibility) {
                    Err(()) => {
                        self.stopped = true;
                        Ok(CoordinatorStep::Stop(RetryStopReason::EligibilityMismatch))
                    }
                    Ok(Some(permit)) => Ok(CoordinatorStep::Next { permit, delay_ms }),
                    Ok(None) => {
                        self.stopped = true;
                        Ok(CoordinatorStep::Stop(RetryStopReason::NoEligibleTargets))
                    }
                }
            }
            RetryDecision::Stop(reason) => {
                self.stopped = true;
                Ok(CoordinatorStep::Stop(reason))
            }
        }
    }

    /// 消费已完整提交的私有成功完成对象，并永久终结当前请求。
    ///
    /// 成功路径不读取 `FailureClass`、不调用 `RetryGate`，也绝不签发下一个 Permit。完成
    /// 对象与当前活跃 Attempt 不匹配时，Coordinator 清除 active 后 fail closed，保持与
    /// 失败 Completion 相同的跨请求混配防护。
    pub(crate) fn complete_success(
        &mut self,
        completion: AttemptSuccessCompletion,
    ) -> Result<CompletedAttempt, AttemptCompleteError> {
        let Some(active) = self.active else {
            return Err(AttemptCompleteError::NoActive);
        };
        let Some(coordinator) = self.coordinator else {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        };
        if !completion.matches(coordinator, active.id) {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        }

        self.active = None;
        self.stopped = true;
        Ok(CompletedAttempt)
    }

    /// 消费零发送的本地容量/选择拒绝，并直接推进下一个计划 Target。
    ///
    /// 此路径不构造 `AttemptOutcome`、不访问 `RetryGate`、不写 Health 或 429；它只在
    /// Registry 尚未取得 Lease、尚未创建 Tracker 的情况下可达。token 失配时仍按所有
    /// Completion 一样清除 active 并 fail closed。
    pub(crate) fn complete_local_rejection(
        &mut self,
        rejection: SelectionLocalRejection<'snapshot, 'candidates>,
        eligibility: &RouteEligibility<'snapshot, 'candidates>,
    ) -> Result<CoordinatorStep<'snapshot, 'candidates>, AttemptCompleteError> {
        let Some(active) = self.active else {
            return Err(AttemptCompleteError::NoActive);
        };
        let Some(coordinator) = self.coordinator else {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        };
        if !rejection.matches(coordinator, active.id) {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        }

        self.active = None;
        self.next_index = active.index.saturating_add(1);
        match self.issue_next(eligibility) {
            Err(()) => {
                self.stopped = true;
                Ok(CoordinatorStep::Stop(RetryStopReason::EligibilityMismatch))
            }
            Ok(Some(permit)) => Ok(CoordinatorStep::Next {
                permit,
                delay_ms: 0,
            }),
            Ok(None) => {
                self.stopped = true;
                Ok(CoordinatorStep::Stop(RetryStopReason::NoEligibleTargets))
            }
        }
    }

    /// 消费来源或策略不变量失效的本地停止 token。
    ///
    /// 与容量拒绝不同，此路径绝不推进 A → B；它只清除当前 active Attempt 并以
    /// `EligibilityMismatch` 停止，防止跨快照、跨 Target 或未实现策略被错误执行。
    pub(crate) fn complete_local_stop(
        &mut self,
        stop: SelectionLocalStop<'snapshot, 'candidates>,
    ) -> Result<CoordinatorStep<'snapshot, 'candidates>, AttemptCompleteError> {
        let Some(active) = self.active else {
            return Err(AttemptCompleteError::NoActive);
        };
        let Some(coordinator) = self.coordinator else {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        };
        if !stop.matches(coordinator, active.id) {
            self.active = None;
            self.stopped = true;
            return Err(AttemptCompleteError::Mismatched);
        }

        self.active = None;
        self.stopped = true;
        Ok(CoordinatorStep::Stop(RetryStopReason::EligibilityMismatch))
    }

    fn issue_next(
        &mut self,
        eligibility: &RouteEligibility<'snapshot, 'candidates>,
    ) -> Result<Option<AttemptPermit<'snapshot, 'candidates>>, ()> {
        if !eligibility.supports_plan(&self.plan) {
            return Err(());
        }
        let Some(coordinator) = self.coordinator else {
            return Ok(None);
        };
        while let Some(target) = self.plan.target_id(self.next_index) {
            let index = self.next_index;
            self.next_index = self.next_index.saturating_add(1);
            if !eligibility.allows_plan_target(target) {
                continue;
            }
            let Some(id) = AttemptId::new(self.next_id) else {
                return Ok(None);
            };
            let Some(next_id) = self.next_id.checked_add(1) else {
                return Ok(None);
            };
            let Some(candidate_index) = self
                .plan
                .snapshot
                .candidates
                .iter()
                .position(|candidate| candidate.target().id() == target)
                .and_then(|position| u8::try_from(position).ok())
            else {
                return Err(());
            };
            self.next_id = next_id;
            self.active = Some(ActiveAttempt { id, index });
            return Ok(Some(AttemptPermit {
                coordinator,
                id,
                index,
                snapshot: self.plan.snapshot,
                candidate_index,
            }));
        }
        Ok(None)
    }
}
