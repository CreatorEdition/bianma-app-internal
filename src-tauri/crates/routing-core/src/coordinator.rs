//! RoutePlan 的线性、有界 Attempt 协调器。
//!
//! 协调链使用所有权而非全局 ID 或锁：Plan 创建首个不可复制 Permit，Permit 被
//! AttemptTracker 消费，Completion 再消费 Coordinator 状态。任何时刻只能存在一个
//! 活跃 Attempt；本模块不执行发送、等待、健康更新、换凭据或同 Stage 重选。

use super::{attempt::AttemptTracker, AttemptOutcome, PlanError, ReplayDecision, ReplayStopReason};
use super::{RoutePlan, RouteStageId, RouteTarget};

/// Coordinator 停止自动推进的稳定原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CoordinatorStopReason {
    /// ReplayGate 拒绝再次发送。
    ReplayDenied(ReplayStopReason),
    /// 路由计划已经访问完毕。
    PlanExhausted,
    /// 路由计划无法解析下一位置，必须 fail closed。
    InvalidPlan(PlanError),
}

/// 完成一个 Attempt 后唯一允许的下一步。
pub(crate) enum CoordinatorStep<'s, 'c> {
    /// 已签发下一计划位置的不可复制 Permit。
    Next(AttemptPermit<'s, 'c>),
    /// 当前请求停止自动推进。
    Stop(CoordinatorStopReason),
}

/// 一个计划位置的只读、不可复制描述。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AttemptPosition {
    index: u8,
    stage: RouteStageId,
    target: RouteTarget,
}

impl AttemptPosition {
    /// 返回从零开始的计划位置。
    pub(crate) const fn index(self) -> u8 {
        self.index
    }
    /// 返回本次绑定的 RouteStage。
    pub(crate) const fn stage(self) -> RouteStageId {
        self.stage
    }
    /// 返回本次绑定的具体 RouteTarget。
    pub(crate) const fn target(self) -> RouteTarget {
        self.target
    }
}

/// Coordinator 签发、只能消费一次的 Attempt 许可。
pub(crate) struct AttemptPermit<'s, 'c> {
    state: CoordinatorState<'s, 'c>,
    position: AttemptPosition,
}

impl<'s, 'c> AttemptPermit<'s, 'c> {
    pub(crate) const fn position(&self) -> AttemptPosition {
        self.position
    }

    pub(crate) fn begin(self) -> AttemptTracker<'s, 'c> {
        AttemptTracker::from_permit(self)
    }

    #[cfg(test)]
    pub(crate) fn test_only() -> AttemptPermit<'static, 'static> {
        static CANDIDATES: [super::RouteCandidate; 1] = [super::RouteCandidate::ready(
            super::RouteStageId::new(1).unwrap(),
            super::RouteTarget::new(
                super::RouteTargetId::new(1).unwrap(),
                super::SiteId::new(1).unwrap(),
                super::ModelDeploymentId::new(1).unwrap(),
                super::EndpointId::new(1).unwrap(),
                super::AccountSelectorId::new(1).unwrap(),
            ),
            0,
        )];
        static SNAPSHOT: super::RoutingSnapshot<'static> = super::RoutingSnapshot {
            version: super::SnapshotVersion::new(1).unwrap(),
            candidates: &CANDIDATES,
            strategy: super::RoutingStrategy::Priority,
            max_attempts: 1,
        };
        let mut target_indices = [u8::MAX; super::MAX_ROUTE_TARGETS];
        target_indices[0] = 0;
        let plan = RoutePlan {
            snapshot: &SNAPSHOT,
            target_indices,
            len: 1,
        };
        CoordinatorState::new(plan).start().unwrap()
    }
}

/// 绑定 Permit 与不可伪造 Outcome 的完成对象。
pub(crate) struct AttemptCompletion<'s, 'c> {
    permit: AttemptPermit<'s, 'c>,
    outcome: AttemptOutcome,
}

impl<'s, 'c> AttemptCompletion<'s, 'c> {
    pub(crate) const fn new(permit: AttemptPermit<'s, 'c>, outcome: AttemptOutcome) -> Self {
        Self { permit, outcome }
    }

    pub(crate) fn advance(self) -> CoordinatorStep<'s, 'c> {
        match self.outcome.replay_decision() {
            ReplayDecision::Stop(reason) => {
                CoordinatorStep::Stop(CoordinatorStopReason::ReplayDenied(reason))
            }
            ReplayDecision::Permit(_) => match self.permit.position.index.checked_add(1) {
                Some(next_index) => self.permit.state.issue(next_index),
                None => CoordinatorStep::Stop(CoordinatorStopReason::InvalidPlan(
                    PlanError::UnknownTarget,
                )),
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn into_outcome_for_test(self) -> AttemptOutcome {
        self.outcome
    }
}

/// 消费 RoutePlan 并线性推进位置的私有状态。
struct CoordinatorState<'s, 'c> {
    plan: RoutePlan<'s, 'c>,
}

impl<'s, 'c> CoordinatorState<'s, 'c> {
    fn new(plan: RoutePlan<'s, 'c>) -> Self {
        Self { plan }
    }

    fn start(self) -> Result<AttemptPermit<'s, 'c>, CoordinatorStopReason> {
        match self.issue(0) {
            CoordinatorStep::Next(permit) => Ok(permit),
            CoordinatorStep::Stop(reason) => Err(reason),
        }
    }

    fn issue(self, index: u8) -> CoordinatorStep<'s, 'c> {
        let stage = match self.plan.stage_id(index) {
            Ok(Some(stage)) => stage,
            Ok(None) => return CoordinatorStep::Stop(CoordinatorStopReason::PlanExhausted),
            Err(error) => return CoordinatorStep::Stop(CoordinatorStopReason::InvalidPlan(error)),
        };
        let target = match self.plan.resolve(index) {
            Ok(Some(target)) => target,
            Ok(None) => return CoordinatorStep::Stop(CoordinatorStopReason::PlanExhausted),
            Err(error) => return CoordinatorStep::Stop(CoordinatorStopReason::InvalidPlan(error)),
        };
        CoordinatorStep::Next(AttemptPermit {
            state: self,
            position: AttemptPosition {
                index,
                stage,
                target,
            },
        })
    }
}

impl<'s, 'c> RoutePlan<'s, 'c> {
    /// 消费计划并签发第一个线性 Attempt Permit。
    pub(crate) fn start_attempts(self) -> Result<AttemptPermit<'s, 'c>, CoordinatorStopReason> {
        CoordinatorState::new(self).start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccountSelectorId, AttemptFailure, EndpointId, ModelDeploymentId, RouteCandidate,
        RoutePlanner, RouteStageId, RouteTarget, RouteTargetId, RoutingSnapshot, RoutingStrategy,
        SiteId, SnapshotVersion,
    };

    fn id<T>(value: u64, make: fn(u64) -> Option<T>) -> T {
        make(value).unwrap()
    }

    fn target(value: u64) -> RouteTarget {
        RouteTarget::new(
            id(value, RouteTargetId::new),
            id(value, SiteId::new),
            id(value, ModelDeploymentId::new),
            id(value, EndpointId::new),
            id(value, AccountSelectorId::new),
        )
    }

    fn plan<'s, 'c>(snapshot: &'s RoutingSnapshot<'c>) -> RoutePlan<'s, 'c> {
        RoutePlanner::plan(snapshot, 0).unwrap()
    }

    #[test]
    fn safe_completions_advance_a_to_b_to_c_then_stop() {
        let candidates = [
            RouteCandidate::ready(id(1, RouteStageId::new), target(1), 0),
            RouteCandidate::ready(id(2, RouteStageId::new), target(2), 0),
            RouteCandidate::ready(id(3, RouteStageId::new), target(3), 0),
        ];
        let snapshot = RoutingSnapshot::new(
            id(1, SnapshotVersion::new),
            &candidates,
            RoutingStrategy::Priority,
            3,
        )
        .unwrap();
        let mut tracker = plan(&snapshot).start_attempts().unwrap().begin();
        assert_eq!(tracker.position().index(), 0);
        let step = tracker.finish(AttemptFailure::Transport, None).advance();
        tracker = match step {
            CoordinatorStep::Next(p) => p.begin(),
            _ => panic!(),
        };
        assert_eq!(tracker.position().index(), 1);
        let step = tracker.finish(AttemptFailure::Transport, None).advance();
        tracker = match step {
            CoordinatorStep::Next(p) => p.begin(),
            _ => panic!(),
        };
        assert_eq!(tracker.position().index(), 2);
        match tracker.finish(AttemptFailure::Transport, None).advance() {
            CoordinatorStep::Stop(CoordinatorStopReason::PlanExhausted) => {}
            _ => panic!(),
        }
    }

    #[test]
    fn delivery_unknown_stops_without_exposing_next_permit() {
        let candidates = [
            RouteCandidate::ready(id(1, RouteStageId::new), target(1), 0),
            RouteCandidate::ready(id(2, RouteStageId::new), target(2), 0),
        ];
        let snapshot = RoutingSnapshot::new(
            id(1, SnapshotVersion::new),
            &candidates,
            RoutingStrategy::Priority,
            2,
        )
        .unwrap();
        let mut tracker = plan(&snapshot).start_attempts().unwrap().begin();
        tracker.request_write_started().unwrap();
        match tracker.finish(AttemptFailure::RateLimited, None).advance() {
            CoordinatorStep::Stop(CoordinatorStopReason::ReplayDenied(
                ReplayStopReason::DeliveryUnknown,
            )) => {}
            _ => panic!(),
        }
    }

    #[test]
    fn ownership_chain_state_stays_small() {
        assert!(core::mem::size_of::<CoordinatorState<'_, '_>>() <= 32);
        assert!(core::mem::size_of::<AttemptPermit<'_, '_>>() <= 88);
        assert!(core::mem::size_of::<AttemptCompletion<'_, '_>>() <= 96);
    }

    #[test]
    fn plan_identity_is_preserved_in_each_position() {
        let candidates = [RouteCandidate::ready(
            id(7, RouteStageId::new),
            target(9),
            0,
        )];
        let snapshot = RoutingSnapshot::new(
            id(5, SnapshotVersion::new),
            &candidates,
            RoutingStrategy::Priority,
            1,
        )
        .unwrap();
        let tracker = plan(&snapshot).start_attempts().unwrap().begin();
        assert_eq!(tracker.position().stage(), id(7, RouteStageId::new));
        assert_eq!(tracker.position().target().id(), id(9, RouteTargetId::new));
    }

    #[test]
    fn trusted_pre_execution_rejection_advances_exactly_once() {
        let candidates = [
            RouteCandidate::ready(id(1, RouteStageId::new), target(1), 0),
            RouteCandidate::ready(id(2, RouteStageId::new), target(2), 0),
        ];
        let snapshot = RoutingSnapshot::new(
            id(1, SnapshotVersion::new),
            &candidates,
            RoutingStrategy::Priority,
            2,
        )
        .unwrap();
        let mut tracker = plan(&snapshot).start_attempts().unwrap().begin();
        tracker.request_write_started().unwrap();
        tracker.upstream_response_observed().unwrap();
        let step = tracker
            .finish(
                AttemptFailure::RateLimited,
                Some(crate::attempt::TrustedPreExecutionRejection::test_only()),
            )
            .advance();
        let tracker = match step {
            CoordinatorStep::Next(permit) => permit.begin(),
            CoordinatorStep::Stop(_) => panic!(),
        };
        assert_eq!(tracker.position().index(), 1);
        assert_eq!(tracker.position().stage(), id(2, RouteStageId::new));
    }

    #[test]
    fn semantic_event_then_cancel_stops_without_next_permit() {
        let candidates = [
            RouteCandidate::ready(id(1, RouteStageId::new), target(1), 0),
            RouteCandidate::ready(id(2, RouteStageId::new), target(2), 0),
        ];
        let snapshot = RoutingSnapshot::new(
            id(1, SnapshotVersion::new),
            &candidates,
            RoutingStrategy::Priority,
            2,
        )
        .unwrap();
        let mut tracker = plan(&snapshot).start_attempts().unwrap().begin();
        tracker.request_write_started().unwrap();
        tracker.upstream_response_observed().unwrap();
        tracker.first_semantic_event_observed().unwrap();
        tracker.cancel().unwrap();
        match tracker.finish(AttemptFailure::Cancelled, None).advance() {
            CoordinatorStep::Stop(CoordinatorStopReason::ReplayDenied(
                ReplayStopReason::Cancelled,
            )) => {}
            _ => panic!(),
        }
    }

    #[test]
    fn corrupt_next_position_fails_closed() {
        let candidates = [RouteCandidate::ready(
            id(1, RouteStageId::new),
            target(1),
            0,
        )];
        let snapshot = RoutingSnapshot::new(
            id(1, SnapshotVersion::new),
            &candidates,
            RoutingStrategy::Priority,
            2,
        )
        .unwrap();
        let mut target_indices = [u8::MAX; crate::MAX_ROUTE_TARGETS];
        target_indices[0] = 0;
        let plan = RoutePlan {
            snapshot: &snapshot,
            target_indices,
            len: 2,
        };
        let tracker = plan.start_attempts().unwrap().begin();
        match tracker.finish(AttemptFailure::Transport, None).advance() {
            CoordinatorStep::Stop(CoordinatorStopReason::InvalidPlan(PlanError::UnknownTarget)) => {
            }
            _ => panic!(),
        }
    }
}
