//! 单次发送、流式语义与安全重放的最小合同。
//!
//! 本模块只记录固定大小的状态，不执行网络 I/O，也不解析 HTTP、SSE 或错误文本。
//! 结果只能由 crate 内受控的 [`AttemptTracker`] 与受信执行前拒绝证明共同生成；Tracker
//! 必须消费 Coordinator 许可，防止一次 Attempt 被重复发送。

use super::{
    coordinator::{AttemptId, AttemptPermit, CoordinatorId},
    health::{HealthTick, RateLimitScope},
    FailureClass, ResolvedRouteTarget, RouteTarget,
};

/// 尝试绑定的限流上报器创建失败原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RateLimitReporterError {
    /// Attempt 许可与同代已解析目标不属于同一个 RoutingSnapshot 或 RouteTarget。
    TargetMismatch,
    /// 当前 Attempt 已经签发过限流上报器。
    AlreadyReported,
}

/// 只能由当前 Attempt 受控执行层签发的一次限流上报器。
///
/// 构造器仅在本模块内可见，且只能由 [`AttemptTracker`] 调用。上报器既不保存 HTTP
/// 状态、响应头或正文，也不携带交付状态或重放证明；调用 [`Self::report`] 时它会被
/// 消费，因此单个上报器不能重复生成观测。
pub(crate) struct AdapterRateLimitReporter {
    target: RouteTarget,
}

impl AdapterRateLimitReporter {
    fn for_attempt(
        permit: &AttemptPermit<'_, '_>,
        resolved: ResolvedRouteTarget<'_, '_>,
    ) -> Result<Self, RateLimitReporterError> {
        let target = resolved.target();
        if permit.target() != target.id() || !resolved.matches_snapshot(permit.snapshot()) {
            return Err(RateLimitReporterError::TargetMismatch);
        }
        Ok(Self { target })
    }

    /// 消费上报器并生成一次只能被 HealthRegistry 消费的受信限流观测。
    pub(crate) fn report(
        self,
        scope: RateLimitScope,
        deadline: HealthTick,
    ) -> TrustedRateLimitObservation {
        TrustedRateLimitObservation {
            target: self.target,
            scope,
            deadline,
        }
    }

    #[cfg(test)]
    fn test_only(target: RouteTarget) -> Self {
        Self { target }
    }
}

/// 仅能由 [`AdapterRateLimitReporter`] 消费式签发的限流冷却观测。
///
/// 它没有公开构造器、不能 Clone 或 Copy，也不表达 HTTP/Delivery/Replay/Account/Quota
/// 信息。HealthRegistry 消费它后只更新当前 Target 对应的 Site 或 Deployment 冷却。
pub(crate) struct TrustedRateLimitObservation {
    target: RouteTarget,
    scope: RateLimitScope,
    deadline: HealthTick,
}

impl TrustedRateLimitObservation {
    pub(crate) fn into_parts(self) -> (RouteTarget, RateLimitScope, HealthTick) {
        (self.target, self.scope, self.deadline)
    }
}

#[cfg(test)]
pub(crate) fn test_rate_limit_observation(
    target: RouteTarget,
    scope: RateLimitScope,
    deadline: HealthTick,
) -> TrustedRateLimitObservation {
    AdapterRateLimitReporter::test_only(target).report(scope, deadline)
}

/// 发送尝试的单调阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendPhase {
    /// 尚未写出请求字节。
    Pending,
    /// 已开始写入请求；上游可能已经收到数据。
    RequestWriteStarted,
    /// 已观察到上游响应头。
    UpstreamResponseObserved,
    /// 已观察到首个经协议确认的 SSE 或流式语义事件。
    FirstSemanticEventObserved,
    /// 已向下游提交任意响应语义。
    DownstreamCommitted,
}

/// 上游请求写入的保守状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamWriteState {
    /// 受控计数 writer 证明尚未写出任何请求字节。
    NoBytesProven,
    /// 已写出一个或多个请求字节。
    BytesWritten,
    /// 无法证明写入状态，必须按可能已发送处理。
    Unknown,
}

/// 下游响应对调用客户端的提交状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DownstreamCommitState {
    /// 尚未提交响应语义。
    NotCommitted,
    /// 已提交响应头、正文或流式语义事件。
    Committed,
}

/// 上游计费的保守状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeState {
    /// 已证明请求未执行，或由受信证据证明在执行前拒绝。
    NotCharged,
    /// 无法证明未计费。
    Unknown,
}

/// 请求是否可安全自动重放。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    /// 传输层证明没有写出请求字节。
    NotSent,
    /// 受信适配器证明上游在执行前明确拒绝。
    PreExecutionRejected,
    /// 已写出请求，或已观察到上游语义。
    Sent,
    /// 无法证明是否写出请求。
    Unknown,
}

/// 违反单调 Attempt 状态机的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttemptTransitionError {
    /// 当前阶段不允许该状态转换。
    InvalidPhase,
}

/// 只能由已登记的 crate 内适配器签发的执行前拒绝证明。
///
/// 未来接入生产适配器时，签发点必须绑定到静态 adapter 合同和经验证的上游
/// 拒绝语义；不得依据 HTTP 429、Retry-After 或错误文本直接签发。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrustedPreExecutionRejection(());

impl TrustedPreExecutionRejection {
    /// 为 crate 内已经验证的适配器合同创建证明。
    pub(crate) const fn registered() -> Self {
        Self(())
    }
}

/// 固定大小、只前进的单次发送记录器。
///
/// 本类型及其变更方法不对外导出，避免调用方把任意 HTTP 状态伪装为“未发送”或
/// “执行前拒绝”。它刻意不实现 `Clone` 或 `Copy`，状态只能向风险更高的方向前进，
/// 最终结果通过消费本记录器生成。
#[derive(Debug)]
pub(crate) struct AttemptTracker<'snapshot, 'candidates> {
    permit: AttemptPermit<'snapshot, 'candidates>,
    phase: SendPhase,
    write: UpstreamWriteState,
    downstream: DownstreamCommitState,
    rate_limit_reporter_issued: bool,
}

impl<'snapshot, 'candidates> AttemptTracker<'snapshot, 'candidates> {
    /// 消费 Coordinator 签发的单次许可并创建状态机。
    ///
    /// 许可被移动进 Tracker，因而同一个 Attempt 不可能被构造为两个独立的发送器。
    pub(crate) const fn from_permit(permit: AttemptPermit<'snapshot, 'candidates>) -> Self {
        Self {
            permit,
            phase: SendPhase::Pending,
            write: UpstreamWriteState::NoBytesProven,
            downstream: DownstreamCommitState::NotCommitted,
            rate_limit_reporter_issued: false,
        }
    }

    /// 返回当前发送阶段。
    pub(crate) const fn phase(&self) -> SendPhase {
        self.phase
    }

    /// 返回当前上游写入状态。
    pub(crate) const fn write_state(&self) -> UpstreamWriteState {
        self.write
    }

    /// 返回当前下游提交状态。
    pub(crate) const fn downstream_state(&self) -> DownstreamCommitState {
        self.downstream
    }

    /// 为当前 Attempt 与已解析 Target 签发一次受信限流上报器。
    ///
    /// 该方法不会修改发送、交付或重放状态；它只在适配器已经按自身受控合同确认限流
    /// 时被调用。一个 Attempt 最多签发一次上报器，且 Target 必须与 Permit 完全一致。
    pub(crate) fn rate_limit_reporter(
        &mut self,
        resolved: ResolvedRouteTarget<'_, '_>,
    ) -> Result<AdapterRateLimitReporter, RateLimitReporterError> {
        if self.rate_limit_reporter_issued {
            return Err(RateLimitReporterError::AlreadyReported);
        }
        let reporter = AdapterRateLimitReporter::for_attempt(&self.permit, resolved)?;
        self.rate_limit_reporter_issued = true;
        Ok(reporter)
    }

    /// 记录写出首个请求字节。
    pub(crate) fn request_write_started(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::Pending || self.write != UpstreamWriteState::NoBytesProven {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::RequestWriteStarted;
        self.write = UpstreamWriteState::BytesWritten;
        Ok(())
    }

    /// 记录 writer 不再能够证明写入量；不会恢复成可重放状态。
    pub(crate) fn write_state_unknown(&mut self) -> Result<(), AttemptTransitionError> {
        if self.downstream == DownstreamCommitState::Committed
            || self.write == UpstreamWriteState::Unknown
        {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.write = UpstreamWriteState::Unknown;
        Ok(())
    }

    /// 记录收到上游响应头。
    pub(crate) fn upstream_response_observed(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::RequestWriteStarted {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::UpstreamResponseObserved;
        Ok(())
    }

    /// 记录首个经协议确认的 SSE 或流式语义事件。
    pub(crate) fn first_semantic_event_observed(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::UpstreamResponseObserved {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::FirstSemanticEventObserved;
        Ok(())
    }

    /// 记录任意响应语义已经对下游可见。
    pub(crate) fn downstream_committed(&mut self) -> Result<(), AttemptTransitionError> {
        if self.downstream != DownstreamCommitState::NotCommitted {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::DownstreamCommitted;
        self.downstream = DownstreamCommitState::Committed;
        Ok(())
    }

    /// 消费记录器并生成只能被 Coordinator 消费的完成对象。
    ///
    /// `trusted_rejection` 只接受 crate 内受信适配器合同签发的证明；普通 HTTP 429
    /// 或 Retry-After 必须传入 `None`，并在发生请求写入后得到不可重放结果。
    pub(crate) fn into_completion(
        self,
        failure: FailureClass,
        retry_after_ms: Option<u64>,
        trusted_rejection: Option<TrustedPreExecutionRejection>,
    ) -> AttemptCompletion<'snapshot, 'candidates> {
        let outcome = self.build_outcome(failure, retry_after_ms, trusted_rejection);
        AttemptCompletion {
            permit: self.permit,
            outcome,
        }
    }

    fn build_outcome(
        &self,
        failure: FailureClass,
        retry_after_ms: Option<u64>,
        trusted_rejection: Option<TrustedPreExecutionRejection>,
    ) -> AttemptOutcome {
        let semantic_event_seen = matches!(
            self.phase,
            SendPhase::FirstSemanticEventObserved | SendPhase::DownstreamCommitted
        );
        let (delivery, charge) = if semantic_event_seen
            || self.downstream == DownstreamCommitState::Committed
            || self.write == UpstreamWriteState::BytesWritten
        {
            (DeliveryState::Sent, ChargeState::Unknown)
        } else if self.write == UpstreamWriteState::Unknown {
            (DeliveryState::Unknown, ChargeState::Unknown)
        } else if trusted_rejection.is_some() {
            (DeliveryState::PreExecutionRejected, ChargeState::NotCharged)
        } else {
            (DeliveryState::NotSent, ChargeState::NotCharged)
        };
        AttemptOutcome {
            failure,
            delivery,
            retry_after_ms,
            phase: self.phase,
            write: self.write,
            downstream: self.downstream,
            charge,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_only(id: u8) -> AttemptTracker<'static, 'static> {
        AttemptTracker::from_permit(AttemptPermit::test_only(id))
    }

    #[cfg(test)]
    pub(crate) fn into_outcome_for_test(
        self,
        failure: FailureClass,
        retry_after_ms: Option<u64>,
        trusted_rejection: Option<TrustedPreExecutionRejection>,
    ) -> AttemptOutcome {
        self.build_outcome(failure, retry_after_ms, trusted_rejection)
    }
}

/// 绑定不可复制许可的 Attempt 完成对象。
///
/// 字段、构造器与解构路径均保持 crate-private。Coordinator 必须消费该对象，不能把
/// Permit 与 Outcome 拆开并重新启动一次 Attempt。
#[derive(Debug)]
pub(crate) struct AttemptCompletion<'snapshot, 'candidates> {
    permit: AttemptPermit<'snapshot, 'candidates>,
    outcome: AttemptOutcome,
}

impl AttemptCompletion<'_, '_> {
    pub(crate) fn matches(&self, coordinator: CoordinatorId, id: AttemptId) -> bool {
        self.permit.belongs_to(coordinator) && self.permit.attempt_id() == id
    }

    pub(crate) const fn outcome(&self) -> &AttemptOutcome {
        &self.outcome
    }
}

/// 由 crate 内受控观察生成的一次失败结果。
///
/// 此类型的字段与构造路径均不对外开放。`RetryGate` 可以读取它并决定下一条路径，
/// 但调用方不能自行制造 `NotSent` 或 `PreExecutionRejected`。
#[derive(Debug)]
pub struct AttemptOutcome {
    failure: FailureClass,
    delivery: DeliveryState,
    retry_after_ms: Option<u64>,
    phase: SendPhase,
    write: UpstreamWriteState,
    downstream: DownstreamCommitState,
    charge: ChargeState,
}

impl AttemptOutcome {
    /// 返回失败类别。
    pub const fn failure(&self) -> FailureClass {
        self.failure
    }

    /// 返回交付状态。
    pub const fn delivery(&self) -> DeliveryState {
        self.delivery
    }

    /// 返回最终发送阶段。
    pub const fn phase(&self) -> SendPhase {
        self.phase
    }

    /// 返回最终上游写入状态。
    pub const fn write_state(&self) -> UpstreamWriteState {
        self.write
    }

    /// 返回最终下游提交状态。
    pub const fn downstream_state(&self) -> DownstreamCommitState {
        self.downstream
    }

    /// 返回保守的上游计费状态。
    pub const fn charge_state(&self) -> ChargeState {
        self.charge
    }

    pub(crate) fn downstream_committed(&self) -> bool {
        self.downstream == DownstreamCommitState::Committed
    }

    pub(crate) const fn retry_after_ms(&self) -> Option<u64> {
        self.retry_after_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_source_stays_small_and_fixed_size() {
        let runtime = include_str!("attempt.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("存在运行时代码");
        let code_lines = runtime
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count();
        assert!(
            code_lines <= 200,
            "Attempt 合同运行时代码过大: {code_lines} 行"
        );
        assert!(core::mem::size_of::<AttemptOutcome>() <= 32);
        assert!(core::mem::size_of::<AttemptTracker<'_, '_>>() <= 32);
        assert!(core::mem::size_of::<AttemptCompletion<'_, '_>>() <= 64);
    }

    #[test]
    fn transitions_are_monotonic_and_bounded() {
        let mut tracker = AttemptTracker::test_only(1);
        assert_eq!(
            tracker.first_semantic_event_observed(),
            Err(AttemptTransitionError::InvalidPhase)
        );
        tracker.request_write_started().expect("允许开始写入");
        tracker
            .upstream_response_observed()
            .expect("允许观察响应头");
        tracker
            .first_semantic_event_observed()
            .expect("允许观察语义事件");
        tracker.downstream_committed().expect("允许下游提交");
        assert_eq!(tracker.phase(), SendPhase::DownstreamCommitted);
        assert_eq!(tracker.downstream_state(), DownstreamCommitState::Committed);
        assert_eq!(
            tracker.request_write_started(),
            Err(AttemptTransitionError::InvalidPhase)
        );
    }

    #[test]
    fn downstream_commit_can_close_any_in_flight_attempt() {
        let mut tracker = AttemptTracker::test_only(1);
        tracker.downstream_committed().expect("允许先提交下游响应");
        assert_eq!(tracker.phase(), SendPhase::DownstreamCommitted);
        assert_eq!(tracker.downstream_state(), DownstreamCommitState::Committed);
        assert_eq!(
            tracker.downstream_committed(),
            Err(AttemptTransitionError::InvalidPhase)
        );
    }

    #[test]
    fn write_state_never_returns_to_a_safe_value() {
        let mut tracker = AttemptTracker::test_only(1);
        tracker.write_state_unknown().expect("允许标记未知");
        assert_eq!(tracker.write_state(), UpstreamWriteState::Unknown);
        assert_eq!(
            tracker.request_write_started(),
            Err(AttemptTransitionError::InvalidPhase)
        );
        let outcome = tracker.into_outcome_for_test(FailureClass::Timeout, None, None);
        assert_eq!(outcome.delivery(), DeliveryState::Unknown);
        assert_eq!(outcome.charge_state(), ChargeState::Unknown);
    }

    #[test]
    fn written_bytes_and_semantic_event_override_trusted_rejection() {
        let mut tracker = AttemptTracker::test_only(1);
        tracker.request_write_started().expect("允许开始写入");
        tracker
            .upstream_response_observed()
            .expect("允许观察响应头");
        tracker
            .first_semantic_event_observed()
            .expect("允许观察语义事件");
        let outcome = tracker.into_outcome_for_test(
            FailureClass::RateLimited,
            Some(1_000),
            Some(TrustedPreExecutionRejection::registered()),
        );
        assert_eq!(outcome.delivery(), DeliveryState::Sent);
        assert_eq!(outcome.charge_state(), ChargeState::Unknown);
    }
}
