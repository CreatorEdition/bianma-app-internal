//! 单次 Attempt 的线性协调器。
//!
//! Coordinator 消费不可变 [`RoutePlan`](super::RoutePlan)，只签发一个活跃许可；一次
//! 完成要么原子地签发下一个计划位置，要么永久终止。它不执行网络 I/O、等待或健康更新。

use core::sync::atomic::{AtomicU64, Ordering};

use super::{
    attempt::AttemptCompletion, RetryDecision, RetryGate, RetryPolicy, RetryStopReason, RoutePlan,
    RouteTargetId,
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
#[derive(Debug)]
pub(crate) struct AttemptPermit {
    coordinator: CoordinatorId,
    id: AttemptId,
    index: u8,
    target: RouteTargetId,
}

impl AttemptPermit {
    /// 返回计划内的单调尝试位置。
    pub(crate) const fn index(&self) -> u8 {
        self.index
    }

    /// 返回本次唯一绑定的路由目标。
    pub(crate) const fn target(&self) -> RouteTargetId {
        self.target
    }

    pub(crate) const fn attempt_id(&self) -> AttemptId {
        self.id
    }

    pub(crate) fn belongs_to(&self, coordinator: CoordinatorId) -> bool {
        self.coordinator == coordinator
    }

    #[cfg(test)]
    pub(crate) fn test_only(id: u8) -> Self {
        Self {
            coordinator: CoordinatorId(1),
            id: AttemptId::new(id).expect("测试 Attempt 标识非零"),
            index: id.saturating_sub(1),
            target: RouteTargetId::new(u64::from(id)).expect("测试目标标识非零"),
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
}

/// 尝试完成被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttemptCompleteError {
    /// 当前没有可完成的活跃 Attempt。
    NoActive,
    /// Completion 不属于当前活跃 Attempt；协调器随即 fail closed。
    Mismatched,
    /// ReplayGate 允许推进但计划内部不再存在下一位置；协调器随即 fail closed。
    MissingNext,
}

/// 完成一次 Attempt 后的唯一下一步。
#[derive(Debug)]
pub(crate) enum CoordinatorStep {
    /// 已原子签发下一个计划位置，调用方只能消费返回的唯一许可继续。
    Next {
        /// 下一个单次 Attempt 许可。
        permit: AttemptPermit,
        /// 在开始该 Attempt 前的有界等待时间。
        delay_ms: u64,
    },
    /// 当前请求永久停止自动推进。
    Stop(RetryStopReason),
}

/// 消费一个 RoutePlan、限制 Attempt 线性推进的固定大小状态机。
pub(crate) struct AttemptCoordinator {
    plan: RoutePlan,
    gate: RetryGate,
    coordinator: Option<CoordinatorId>,
    next_index: u8,
    next_id: u8,
    active: Option<ActiveAttempt>,
    stopped: bool,
}

impl AttemptCoordinator {
    pub(crate) fn new(plan: RoutePlan, policy: RetryPolicy) -> Self {
        let coordinator = allocate_coordinator_id();
        Self {
            plan,
            gate: RetryGate::new(policy),
            coordinator,
            next_index: 0,
            next_id: 1,
            active: None,
            stopped: coordinator.is_none(),
        }
    }

    /// 返回是否已有永久停止结论。
    pub(crate) const fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// 返回是否已有尚未完成的 Attempt。
    pub(crate) const fn has_active_attempt(&self) -> bool {
        self.active.is_some()
    }

    /// 签发初始 Attempt；后续 Attempt 只能由 [`Self::complete`] 原子签发。
    pub(crate) fn start(&mut self) -> Result<AttemptPermit, AttemptStartError> {
        if self.stopped {
            return Err(AttemptStartError::Stopped);
        }
        if self.active.is_some() {
            return Err(AttemptStartError::ActiveAttempt);
        }
        match self.issue_next() {
            Some(permit) => Ok(permit),
            None => {
                self.stopped = true;
                Err(AttemptStartError::Stopped)
            }
        }
    }

    /// 消费匹配的 Completion，返回唯一的后续许可或永久停止结论。
    pub(crate) fn complete(
        &mut self,
        completion: AttemptCompletion,
    ) -> Result<CoordinatorStep, AttemptCompleteError> {
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
                let Some(permit) = self.issue_next() else {
                    self.stopped = true;
                    return Err(AttemptCompleteError::MissingNext);
                };
                Ok(CoordinatorStep::Next { permit, delay_ms })
            }
            RetryDecision::Stop(reason) => {
                self.stopped = true;
                Ok(CoordinatorStep::Stop(reason))
            }
        }
    }

    fn issue_next(&mut self) -> Option<AttemptPermit> {
        let coordinator = self.coordinator?;
        let target = self.plan.target_id(self.next_index)?;
        let id = AttemptId::new(self.next_id)?;
        self.next_id = self.next_id.checked_add(1)?;
        self.active = Some(ActiveAttempt {
            id,
            index: self.next_index,
        });
        Some(AttemptPermit {
            coordinator,
            id,
            index: self.next_index,
            target,
        })
    }
}
