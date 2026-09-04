//! 单次发送结束后的保守重放门禁。
//!
//! 本模块不解析 HTTP、`Retry-After` 或错误文本，也不执行等待和下一次发送。未来的
//! Transport 只能通过 crate 内状态机记录事实；普通 429/503 本身不能签发重放许可。

use super::coordinator::{AttemptCompletion, AttemptPermit, AttemptPosition};

/// 一次发送失败的无敏感分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptFailure {
    /// 上游额度或频率限制。
    RateLimited,
    /// 上游过载或暂时不可用。
    Overloaded,
    /// 建连、写入或读取响应失败。
    Transport,
    /// 超过连接、首字节或流空闲预算。
    Timeout,
    /// 上游拒绝当前凭据。
    Authentication,
    /// 上游拒绝请求或模型。
    Rejected,
    /// 响应无法按已登记协议验证。
    Protocol,
    /// 本地客户端取消。
    Cancelled,
    /// 无法安全分类。
    Unknown,
}

/// 单次发送只前进的阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SendPhase {
    /// 受控 writer 尚未写出请求字节。
    Pending,
    /// 已开始写出请求，上游可能已经收到数据。
    RequestWriteStarted,
    /// 已观察到上游响应头。
    UpstreamResponseObserved,
    /// 已观察到首个经协议确认的语义事件。
    FirstSemanticEventObserved,
    /// 已向下游提交响应语义。
    DownstreamCommitted,
    /// 客户端已取消。
    Cancelled,
}

/// 上游写入的保守状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpstreamWriteState {
    /// 计数 writer 证明零字节写出。
    NoBytesProven,
    /// 已开始写出一个或多个请求字节。
    BytesMayHaveBeenWritten,
    /// Transport 无法证明写入量。
    Unknown,
}

/// 下游响应是否已经对客户端可见。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DownstreamCommitState {
    /// 尚未提交响应语义。
    NotCommitted,
    /// 已提交响应头、正文或流事件。
    Committed,
}

/// 当前请求是否有可能已经执行。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryState {
    /// 受控 writer 证明没有写出请求。
    NotSent,
    /// 已登记 adapter 合同证明上游在执行前拒绝。
    PreExecutionRejected,
    /// 无法证明请求未发送或未执行。
    DeliveryUnknown,
}

/// 保守计费状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeState {
    /// 已证明当前 Attempt 未执行。
    NotCharged,
    /// 无法证明是否产生计费。
    Unknown,
}

/// ReplayGate 允许新建独立 Attempt 的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayPermitReason {
    /// 当前 Attempt 被证明未发送。
    NotSent,
    /// 受信 adapter 合同证明在执行前拒绝。
    PreExecutionRejected,
}

/// ReplayGate 停止自动重放的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayStopReason {
    /// 本地客户端取消。
    Cancelled,
    /// 已向下游提交响应语义。
    DownstreamCommitted,
    /// 已观察到上游语义，不能再声称执行前拒绝。
    SemanticEventObserved,
    /// 写出或执行状态无法证明安全。
    DeliveryUnknown,
}

/// 是否允许 Coordinator 创建一个新的独立 Attempt。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDecision {
    /// 仅允许新建 Attempt；不决定等待、换 Key、同 Stage 或推进 A→B。
    Permit(ReplayPermitReason),
    /// 必须停止当前请求的自动重放。
    Stop(ReplayStopReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AttemptTransitionError {
    InvalidPhase,
}

/// 只能由 crate 内已登记 adapter 合同签发的执行前拒绝证明。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrustedPreExecutionRejection(());

impl TrustedPreExecutionRejection {
    #[cfg(test)]
    pub(crate) const fn test_only() -> Self {
        Self(())
    }
}

/// 固定大小、不可复制的单次发送事实记录器。
pub(crate) struct AttemptTracker<'s, 'c> {
    permit: AttemptPermit<'s, 'c>,
    phase: SendPhase,
    write: UpstreamWriteState,
    downstream: DownstreamCommitState,
    cancelled: bool,
}

impl<'s, 'c> AttemptTracker<'s, 'c> {
    pub(crate) const fn from_permit(permit: AttemptPermit<'s, 'c>) -> Self {
        Self {
            permit,
            phase: SendPhase::Pending,
            // 默认不假设 writer 已经证明零字节；必须由 writer 显式签发后才可安全重放。
            write: UpstreamWriteState::Unknown,
            downstream: DownstreamCommitState::NotCommitted,
            cancelled: false,
        }
    }

    pub(crate) const fn position(&self) -> AttemptPosition {
        self.permit.position()
    }

    /// 计数 writer 报告零字节写出后调用；只允许在 `Pending` 阶段签发。
    pub(crate) fn zero_bytes_proven(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::Pending {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.write = UpstreamWriteState::NoBytesProven;
        Ok(())
    }

    pub(crate) fn request_write_started(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::Pending
            || self.write == UpstreamWriteState::BytesMayHaveBeenWritten
        {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::RequestWriteStarted;
        self.write = UpstreamWriteState::BytesMayHaveBeenWritten;
        Ok(())
    }

    pub(crate) fn write_state_unknown(&mut self) -> Result<(), AttemptTransitionError> {
        if matches!(
            self.phase,
            SendPhase::DownstreamCommitted | SendPhase::Cancelled
        ) {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.write = UpstreamWriteState::Unknown;
        Ok(())
    }

    pub(crate) fn upstream_response_observed(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::RequestWriteStarted {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::UpstreamResponseObserved;
        Ok(())
    }

    pub(crate) fn first_semantic_event_observed(&mut self) -> Result<(), AttemptTransitionError> {
        if self.phase != SendPhase::UpstreamResponseObserved {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::FirstSemanticEventObserved;
        Ok(())
    }

    pub(crate) fn downstream_committed(&mut self) -> Result<(), AttemptTransitionError> {
        if !matches!(
            self.phase,
            SendPhase::UpstreamResponseObserved | SendPhase::FirstSemanticEventObserved
        ) {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.phase = SendPhase::DownstreamCommitted;
        self.downstream = DownstreamCommitState::Committed;
        Ok(())
    }

    pub(crate) fn cancel(&mut self) -> Result<(), AttemptTransitionError> {
        if self.cancelled || matches!(self.phase, SendPhase::DownstreamCommitted) {
            return Err(AttemptTransitionError::InvalidPhase);
        }
        self.cancelled = true;
        if !matches!(self.phase, SendPhase::FirstSemanticEventObserved) {
            self.phase = SendPhase::Cancelled;
        }
        Ok(())
    }

    pub(crate) fn finish(
        self,
        failure: AttemptFailure,
        trusted_rejection: Option<TrustedPreExecutionRejection>,
    ) -> AttemptCompletion<'s, 'c> {
        let delivery = if self.write == UpstreamWriteState::Unknown
            || matches!(
                self.phase,
                SendPhase::FirstSemanticEventObserved | SendPhase::DownstreamCommitted
            ) {
            DeliveryState::DeliveryUnknown
        } else if self.write == UpstreamWriteState::NoBytesProven {
            DeliveryState::NotSent
        } else if trusted_rejection.is_some() {
            DeliveryState::PreExecutionRejected
        } else {
            DeliveryState::DeliveryUnknown
        };
        let charge = match delivery {
            DeliveryState::NotSent | DeliveryState::PreExecutionRejected => ChargeState::NotCharged,
            DeliveryState::DeliveryUnknown => ChargeState::Unknown,
        };
        AttemptCompletion::new(
            self.permit,
            AttemptOutcome {
                failure,
                phase: self.phase,
                write: self.write,
                downstream: self.downstream,
                delivery,
                charge,
                cancelled: self.cancelled,
            },
        )
    }
}

#[cfg(test)]
impl AttemptTracker<'static, 'static> {
    fn test_only() -> Self {
        Self::from_permit(AttemptPermit::test_only())
    }
}

/// 一次 Attempt 的固定大小、不可伪造结果。
#[derive(Debug)]
pub struct AttemptOutcome {
    failure: AttemptFailure,
    phase: SendPhase,
    write: UpstreamWriteState,
    downstream: DownstreamCommitState,
    delivery: DeliveryState,
    charge: ChargeState,
    cancelled: bool,
}

impl AttemptOutcome {
    /// 返回失败类别；该值不能单独决定重放。
    pub const fn failure(&self) -> AttemptFailure {
        self.failure
    }
    /// 返回最终发送阶段。
    pub const fn phase(&self) -> SendPhase {
        self.phase
    }
    /// 返回最终写入状态。
    pub const fn write_state(&self) -> UpstreamWriteState {
        self.write
    }
    /// 返回下游提交状态。
    pub const fn downstream_state(&self) -> DownstreamCommitState {
        self.downstream
    }
    /// 返回保守交付状态。
    pub const fn delivery(&self) -> DeliveryState {
        self.delivery
    }
    /// 返回保守计费状态。
    pub const fn charge_state(&self) -> ChargeState {
        self.charge
    }

    /// 生成唯一重放结论，不决定具体路由动作。
    pub const fn replay_decision(&self) -> ReplayDecision {
        if matches!(self.downstream, DownstreamCommitState::Committed) {
            return ReplayDecision::Stop(ReplayStopReason::DownstreamCommitted);
        }
        if self.cancelled
            || matches!(self.failure, AttemptFailure::Cancelled)
            || matches!(self.phase, SendPhase::Cancelled)
        {
            return ReplayDecision::Stop(ReplayStopReason::Cancelled);
        }
        if matches!(self.phase, SendPhase::FirstSemanticEventObserved) {
            return ReplayDecision::Stop(ReplayStopReason::SemanticEventObserved);
        }
        match self.delivery {
            DeliveryState::NotSent => ReplayDecision::Permit(ReplayPermitReason::NotSent),
            DeliveryState::PreExecutionRejected => {
                ReplayDecision::Permit(ReplayPermitReason::PreExecutionRejected)
            }
            DeliveryState::DeliveryUnknown => {
                ReplayDecision::Stop(ReplayStopReason::DeliveryUnknown)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_429_or_503_never_self_authorizes_replay_after_write() {
        for failure in [AttemptFailure::RateLimited, AttemptFailure::Overloaded] {
            let mut tracker = AttemptTracker::test_only();
            tracker.request_write_started().unwrap();
            tracker.upstream_response_observed().unwrap();
            let outcome = tracker.finish(failure, None).into_outcome_for_test();
            assert_eq!(outcome.delivery(), DeliveryState::DeliveryUnknown);
            assert_eq!(
                outcome.replay_decision(),
                ReplayDecision::Stop(ReplayStopReason::DeliveryUnknown)
            );
            assert_eq!(outcome.charge_state(), ChargeState::Unknown);
        }
    }

    #[test]
    fn unproven_initial_write_state_fails_closed() {
        let outcome = AttemptTracker::test_only()
            .finish(AttemptFailure::Transport, None)
            .into_outcome_for_test();
        assert_eq!(outcome.write_state(), UpstreamWriteState::Unknown);
        assert_eq!(outcome.delivery(), DeliveryState::DeliveryUnknown);
        assert_eq!(outcome.charge_state(), ChargeState::Unknown);
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::DeliveryUnknown)
        );
    }

    #[test]
    fn zero_write_proof_permits_a_new_independent_attempt() {
        let mut tracker = AttemptTracker::test_only();
        tracker.zero_bytes_proven().unwrap();
        let outcome = tracker
            .finish(AttemptFailure::Transport, None)
            .into_outcome_for_test();
        assert_eq!(outcome.delivery(), DeliveryState::NotSent);
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Permit(ReplayPermitReason::NotSent)
        );
        assert_eq!(outcome.charge_state(), ChargeState::NotCharged);
    }

    #[test]
    fn trusted_pre_execution_rejection_is_the_only_written_request_exception() {
        let mut tracker = AttemptTracker::test_only();
        tracker.request_write_started().unwrap();
        tracker.upstream_response_observed().unwrap();
        let outcome = tracker
            .finish(
                AttemptFailure::RateLimited,
                Some(TrustedPreExecutionRejection::test_only()),
            )
            .into_outcome_for_test();
        assert_eq!(outcome.delivery(), DeliveryState::PreExecutionRejected);
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Permit(ReplayPermitReason::PreExecutionRejected)
        );
    }

    #[test]
    fn semantic_event_or_downstream_commit_always_stops() {
        let mut semantic = AttemptTracker::test_only();
        semantic.request_write_started().unwrap();
        semantic.upstream_response_observed().unwrap();
        semantic.first_semantic_event_observed().unwrap();
        let outcome = semantic
            .finish(
                AttemptFailure::Protocol,
                Some(TrustedPreExecutionRejection::test_only()),
            )
            .into_outcome_for_test();
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::SemanticEventObserved)
        );

        let mut committed_after_cancel = AttemptTracker::test_only();
        committed_after_cancel.request_write_started().unwrap();
        committed_after_cancel.upstream_response_observed().unwrap();
        committed_after_cancel
            .first_semantic_event_observed()
            .unwrap();
        committed_after_cancel.cancel().unwrap();
        committed_after_cancel.downstream_committed().unwrap();
        let outcome = committed_after_cancel
            .finish(AttemptFailure::Cancelled, None)
            .into_outcome_for_test();
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::DownstreamCommitted)
        );

        let mut committed = AttemptTracker::test_only();
        committed.request_write_started().unwrap();
        committed.upstream_response_observed().unwrap();
        committed.downstream_committed().unwrap();
        let outcome = committed
            .finish(AttemptFailure::Unknown, None)
            .into_outcome_for_test();
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::DownstreamCommitted)
        );
    }

    #[test]
    fn zero_write_proof_takes_precedence_over_trusted_rejection() {
        let mut tracker = AttemptTracker::test_only();
        tracker.zero_bytes_proven().unwrap();
        let outcome = tracker
            .finish(
                AttemptFailure::RateLimited,
                Some(TrustedPreExecutionRejection::test_only()),
            )
            .into_outcome_for_test();
        assert_eq!(outcome.delivery(), DeliveryState::NotSent);
        assert_eq!(outcome.charge_state(), ChargeState::NotCharged);
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Permit(ReplayPermitReason::NotSent)
        );
    }

    #[test]
    fn unknown_write_state_and_cancel_fail_closed() {
        let mut unknown = AttemptTracker::test_only();
        unknown.write_state_unknown().unwrap();
        let outcome = unknown
            .finish(
                AttemptFailure::Timeout,
                Some(TrustedPreExecutionRejection::test_only()),
            )
            .into_outcome_for_test();
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::DeliveryUnknown)
        );

        let mut cancelled = AttemptTracker::test_only();
        cancelled.cancel().unwrap();
        let outcome = cancelled
            .finish(AttemptFailure::Cancelled, None)
            .into_outcome_for_test();
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::Cancelled)
        );
    }

    #[test]
    fn cancellation_never_erases_observed_semantics() {
        let mut tracker = AttemptTracker::test_only();
        tracker.request_write_started().unwrap();
        tracker.upstream_response_observed().unwrap();
        tracker.first_semantic_event_observed().unwrap();
        tracker.cancel().unwrap();
        let outcome = tracker
            .finish(AttemptFailure::Cancelled, None)
            .into_outcome_for_test();
        assert_eq!(outcome.phase(), SendPhase::FirstSemanticEventObserved);
        assert_eq!(outcome.delivery(), DeliveryState::DeliveryUnknown);
        assert_eq!(outcome.charge_state(), ChargeState::Unknown);
        assert_eq!(
            outcome.replay_decision(),
            ReplayDecision::Stop(ReplayStopReason::Cancelled)
        );
    }

    #[test]
    fn transitions_and_runtime_state_stay_small() {
        let mut tracker = AttemptTracker::test_only();
        assert_eq!(
            tracker.first_semantic_event_observed(),
            Err(AttemptTransitionError::InvalidPhase)
        );
        tracker.request_write_started().unwrap();
        assert_eq!(
            tracker.request_write_started(),
            Err(AttemptTransitionError::InvalidPhase)
        );
        assert!(core::mem::size_of::<AttemptTracker<'_, '_>>() <= 96);
        assert!(core::mem::size_of::<AttemptOutcome>() <= 8);
    }
}
