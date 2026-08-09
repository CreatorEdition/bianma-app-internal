//! Bianma 的低开销纯 Rust 路由决策核心。
//!
//! 本 crate 只负责基于内存快照生成有界路由计划，并在一次尝试结束后给出保守的
//! 下一步决策。它不依赖 Tauri、数据库、网络客户端、异步运行时或 ContextPipeline。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// 单个路由计划允许的最大目标数。
pub const MAX_ROUTE_TARGETS: usize = 16;

// 当前切片只定义合同，生产 Transport 适配器尚未接线；不能为消除未使用告警而暴露
// 可伪造的 Attempt 构造路径。单测会覆盖该内部状态机。
#[cfg_attr(not(test), allow(dead_code))]
mod attempt;
#[cfg_attr(not(test), allow(dead_code))]
mod coordinator;
mod ingress;

pub use attempt::{
    AttemptOutcome, ChargeState, DeliveryState, DownstreamCommitState, SendPhase,
    UpstreamWriteState,
};
use coordinator::AttemptCoordinator;
pub use ingress::*;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            /// 从非零整数构造标识。
            pub const fn new(value: u64) -> Option<Self> {
                if value == 0 { None } else { Some(Self(value)) }
            }

            /// 返回底层稳定标识。
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

id_type!(
    /// 路由快照版本。
    SnapshotVersion
);
id_type!(
    /// 路由目标标识。
    RouteTargetId
);
id_type!(
    /// 站点标识。
    SiteId
);
id_type!(
    /// 模型部署标识。
    ModelDeploymentId
);
id_type!(
    /// 上游端点标识。
    EndpointId
);
id_type!(
    /// 账户标识。
    AccountId
);
id_type!(
    /// 凭据标识。
    CredentialId
);

impl RouteTargetId {
    const INVALID: Self = Self(0);
}

/// 一个已经由快照编译器绑定完整身份的路由目标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteTarget {
    id: RouteTargetId,
    site: SiteId,
    deployment: ModelDeploymentId,
    endpoint: EndpointId,
    account: AccountId,
    credential: CredentialId,
    priority: u16,
}

impl RouteTarget {
    /// 构造一个完整绑定的路由目标。
    pub const fn new(
        id: RouteTargetId,
        site: SiteId,
        deployment: ModelDeploymentId,
        endpoint: EndpointId,
        account: AccountId,
        credential: CredentialId,
        priority: u16,
    ) -> Self {
        Self {
            id,
            site,
            deployment,
            endpoint,
            account,
            credential,
            priority,
        }
    }

    /// 返回路由目标标识。
    pub const fn id(self) -> RouteTargetId {
        self.id
    }

    /// 返回站点标识。
    pub const fn site(self) -> SiteId {
        self.site
    }

    /// 返回模型部署标识。
    pub const fn deployment(self) -> ModelDeploymentId {
        self.deployment
    }

    /// 返回上游端点标识。
    pub const fn endpoint(self) -> EndpointId {
        self.endpoint
    }

    /// 返回账户标识。
    pub const fn account(self) -> AccountId {
        self.account
    }

    /// 返回凭据标识。
    pub const fn credential(self) -> CredentialId {
        self.credential
    }

    /// 返回较小数值优先的静态优先级。
    pub const fn priority(self) -> u16 {
        self.priority
    }
}

/// 路由目标在当前不可变快照中的可用状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetState {
    /// 目标可选，`penalty` 越小越适合均衡策略。
    Ready {
        /// 由快照编译器计算的有界惩罚值。
        penalty: u16,
    },
    /// 目标正在冷却，本次计划不选择。
    CoolingDown,
    /// 目标被用户或配置禁用。
    Disabled,
}

/// 快照中的一个目标候选。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteCandidate {
    target: RouteTarget,
    state: TargetState,
}

impl RouteCandidate {
    /// 创建可用候选。
    pub const fn ready(target: RouteTarget, penalty: u16) -> Self {
        Self {
            target,
            state: TargetState::Ready { penalty },
        }
    }

    /// 创建冷却中的候选。
    pub const fn cooling_down(target: RouteTarget) -> Self {
        Self {
            target,
            state: TargetState::CoolingDown,
        }
    }

    /// 创建禁用候选。
    pub const fn disabled(target: RouteTarget) -> Self {
        Self {
            target,
            state: TargetState::Disabled,
        }
    }

    /// 返回绑定目标。
    pub const fn target(self) -> RouteTarget {
        self.target
    }

    /// 返回当前状态。
    pub const fn state(self) -> TargetState {
        self.state
    }
}

/// 路由目标排序策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingStrategy {
    /// 严格按静态优先级形成 `A -> B -> C`。
    Priority,
    /// 在静态优先级序列上按请求游标轮转起点。
    RoundRobin,
    /// 优先选择当前惩罚值更低的目标，再按静态优先级排序。
    LeastPenalty,
}

/// 路由计划编译失败原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    /// 快照没有任何候选。
    NoCandidates,
    /// 候选数超过固定上限。
    TooManyCandidates,
    /// 最大尝试数为零或超过固定上限。
    InvalidMaxAttempts,
    /// 同一快照重复出现相同目标。
    DuplicateTarget,
    /// 当前没有可用目标。
    NoEligibleTargets,
    /// 路由计划与传入快照版本不一致。
    StaleSnapshot,
    /// 计划中的目标无法在同版本快照中解析。
    UnknownTarget,
}

/// 路由选择使用的不可变内存快照。
pub struct RoutingSnapshot<'a> {
    version: SnapshotVersion,
    candidates: &'a [RouteCandidate],
    strategy: RoutingStrategy,
    max_attempts: u8,
}

impl<'a> RoutingSnapshot<'a> {
    /// 验证并创建快照视图。
    pub fn new(
        version: SnapshotVersion,
        candidates: &'a [RouteCandidate],
        strategy: RoutingStrategy,
        max_attempts: u8,
    ) -> Result<Self, PlanError> {
        if candidates.is_empty() {
            return Err(PlanError::NoCandidates);
        }
        if candidates.len() > MAX_ROUTE_TARGETS {
            return Err(PlanError::TooManyCandidates);
        }
        if max_attempts == 0 || usize::from(max_attempts) > MAX_ROUTE_TARGETS {
            return Err(PlanError::InvalidMaxAttempts);
        }
        for (index, candidate) in candidates.iter().enumerate() {
            if candidates[..index]
                .iter()
                .any(|previous| previous.target.id == candidate.target.id)
            {
                return Err(PlanError::DuplicateTarget);
            }
        }
        Ok(Self {
            version,
            candidates,
            strategy,
            max_attempts,
        })
    }

    /// 返回快照版本。
    pub const fn version(&self) -> SnapshotVersion {
        self.version
    }

    /// 返回路由策略。
    pub const fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }

    /// 返回最大尝试数。
    pub const fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    fn resolve(&self, target_id: RouteTargetId) -> Option<&RouteTarget> {
        self.candidates
            .iter()
            .find(|candidate| candidate.target.id == target_id)
            .map(|candidate| &candidate.target)
    }
}

#[derive(Clone, Copy)]
struct CandidateOrder {
    index: u8,
    penalty: u16,
    priority: u16,
}

impl CandidateOrder {
    const EMPTY: Self = Self {
        index: 0,
        penalty: 0,
        priority: 0,
    };
}

/// 无状态、无分配的路由计划器。
pub struct RoutePlanner;

impl RoutePlanner {
    /// 根据快照与请求游标生成有界计划。
    pub fn plan(
        snapshot: &RoutingSnapshot<'_>,
        request_cursor: u64,
    ) -> Result<RoutePlan, PlanError> {
        let mut ordered = [CandidateOrder::EMPTY; MAX_ROUTE_TARGETS];
        let mut eligible_count = 0usize;

        for (index, candidate) in snapshot.candidates.iter().enumerate() {
            let TargetState::Ready { penalty } = candidate.state else {
                continue;
            };
            ordered[eligible_count] = CandidateOrder {
                index: index as u8,
                penalty,
                priority: candidate.target.priority,
            };
            eligible_count += 1;
        }

        if eligible_count == 0 {
            return Err(PlanError::NoEligibleTargets);
        }

        let eligible = &mut ordered[..eligible_count];
        match snapshot.strategy {
            RoutingStrategy::Priority | RoutingStrategy::RoundRobin => {
                eligible.sort_unstable_by_key(|item| (item.priority, item.index));
            }
            RoutingStrategy::LeastPenalty => {
                eligible.sort_unstable_by_key(|item| (item.penalty, item.priority, item.index));
            }
        }

        let rotation = if snapshot.strategy == RoutingStrategy::RoundRobin {
            request_cursor as usize % eligible_count
        } else {
            0
        };
        let plan_len = eligible_count.min(usize::from(snapshot.max_attempts));
        let mut target_ids = [RouteTargetId::INVALID; MAX_ROUTE_TARGETS];
        for (plan_index, slot) in target_ids[..plan_len].iter_mut().enumerate() {
            let source = eligible[(plan_index + rotation) % eligible_count];
            *slot = snapshot.candidates[usize::from(source.index)].target.id;
        }

        Ok(RoutePlan {
            snapshot_version: snapshot.version,
            target_ids,
            len: plan_len as u8,
        })
    }
}

/// 固定容量、无堆分配的路由执行计划。
pub struct RoutePlan {
    snapshot_version: SnapshotVersion,
    target_ids: [RouteTargetId; MAX_ROUTE_TARGETS],
    len: u8,
}

impl RoutePlan {
    /// 消费计划并创建只允许线性推进的 Attempt 协调器。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn into_attempt_coordinator(self, policy: RetryPolicy) -> AttemptCoordinator {
        AttemptCoordinator::new(self, policy)
    }

    /// 返回快照版本。
    pub const fn snapshot_version(&self) -> SnapshotVersion {
        self.snapshot_version
    }

    /// 返回计划中的尝试数。
    pub const fn len(&self) -> u8 {
        self.len
    }

    /// 判断计划是否为空。
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 返回指定尝试对应的目标标识。
    pub fn target_id(&self, attempt_index: u8) -> Option<RouteTargetId> {
        if attempt_index >= self.len {
            return None;
        }
        Some(self.target_ids[usize::from(attempt_index)])
    }

    /// 在同版本快照中解析指定尝试的完整目标。
    pub fn resolve<'a>(
        &self,
        snapshot: &'a RoutingSnapshot<'_>,
        attempt_index: u8,
    ) -> Result<Option<&'a RouteTarget>, PlanError> {
        if self.snapshot_version != snapshot.version {
            return Err(PlanError::StaleSnapshot);
        }
        let Some(target_id) = self.target_id(attempt_index) else {
            return Ok(None);
        };
        snapshot
            .resolve(target_id)
            .map(Some)
            .ok_or(PlanError::UnknownTarget)
    }
}

/// 一次尝试的稳定失败类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureClass {
    /// 建立连接失败。
    Connect,
    /// 在可证明未写入请求时超时。
    Timeout,
    /// 上游限流。
    RateLimited,
    /// 上游服务错误。
    Server,
    /// 认证失败。
    Authentication,
    /// 请求无效。
    InvalidRequest,
    /// 协议响应无效。
    Protocol,
    /// 客户端取消。
    Cancelled,
    /// 无法安全归类。
    Unknown,
}

/// 重试策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    max_attempts: u8,
    retry_after_cap_ms: u64,
}

impl RetryPolicy {
    /// 创建有界重试策略。
    pub const fn new(max_attempts: u8, retry_after_cap_ms: u64) -> Option<Self> {
        if max_attempts == 0 || max_attempts as usize > MAX_ROUTE_TARGETS {
            return None;
        }
        Some(Self {
            max_attempts,
            retry_after_cap_ms,
        })
    }
}

/// 停止推进路由计划的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryStopReason {
    /// 没有下一个目标或预算已耗尽。
    Exhausted,
    /// 客户端已经取消。
    Cancelled,
    /// 已向客户端提交响应。
    DownstreamCommitted,
    /// 错误本身不可通过换目标解决。
    NonRetryable,
    /// 请求可能已发送，缺少安全重放证据。
    ReplayNotProven,
}

/// ReplayGate 对下一步的唯一决策。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum RetryDecision {
    /// 推进到计划中的下一个目标。
    Advance {
        /// 推进前的有界等待时间。
        delay_ms: u64,
    },
    /// 停止自动推进。
    Stop(RetryStopReason),
}

/// 保守的重试与路径推进门禁。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct RetryGate {
    policy: RetryPolicy,
}

impl RetryGate {
    /// 创建门禁。
    pub(crate) const fn new(policy: RetryPolicy) -> Self {
        Self { policy }
    }

    /// 根据计划位置与尝试结果决定是否推进。
    pub(crate) fn decide(
        &self,
        plan: &RoutePlan,
        attempt_index: u8,
        outcome: &AttemptOutcome,
    ) -> RetryDecision {
        if outcome.downstream_committed() {
            return RetryDecision::Stop(RetryStopReason::DownstreamCommitted);
        }
        if outcome.failure() == FailureClass::Cancelled {
            return RetryDecision::Stop(RetryStopReason::Cancelled);
        }
        if attempt_index.saturating_add(1) >= plan.len
            || attempt_index.saturating_add(1) >= self.policy.max_attempts
        {
            return RetryDecision::Stop(RetryStopReason::Exhausted);
        }
        if matches!(
            outcome.failure(),
            FailureClass::Authentication
                | FailureClass::InvalidRequest
                | FailureClass::Protocol
                | FailureClass::Unknown
        ) {
            return RetryDecision::Stop(RetryStopReason::NonRetryable);
        }
        if !matches!(
            outcome.delivery(),
            DeliveryState::NotSent | DeliveryState::PreExecutionRejected
        ) {
            return RetryDecision::Stop(RetryStopReason::ReplayNotProven);
        }

        let delay_ms = if outcome.failure() == FailureClass::RateLimited {
            outcome
                .retry_after_ms()
                .unwrap_or_default()
                .min(self.policy.retry_after_cap_ms)
        } else {
            0
        };
        RetryDecision::Advance { delay_ms }
    }
}

#[cfg(test)]
mod tests {
    use super::coordinator::{
        AttemptCompleteError, AttemptPermit, AttemptStartError, CoordinatorStep,
    };
    use super::*;

    fn id(value: u64) -> RouteTargetId {
        RouteTargetId::new(value).expect("测试 ID 非零")
    }

    fn target(value: u64, priority: u16) -> RouteTarget {
        RouteTarget::new(
            id(value),
            SiteId::new(value).expect("站点 ID 非零"),
            ModelDeploymentId::new(value).expect("部署 ID 非零"),
            EndpointId::new(value).expect("端点 ID 非零"),
            AccountId::new(value).expect("账户 ID 非零"),
            CredentialId::new(value).expect("凭据 ID 非零"),
            priority,
        )
    }

    fn version(value: u64) -> SnapshotVersion {
        SnapshotVersion::new(value).expect("快照版本非零")
    }

    fn snapshot<'a>(
        candidates: &'a [RouteCandidate],
        strategy: RoutingStrategy,
        max_attempts: u8,
    ) -> RoutingSnapshot<'a> {
        RoutingSnapshot::new(version(1), candidates, strategy, max_attempts).expect("测试快照有效")
    }

    fn not_sent(failure: FailureClass, retry_after_ms: Option<u64>) -> AttemptOutcome {
        attempt::AttemptTracker::test_only(1).into_outcome_for_test(failure, retry_after_ms, None)
    }

    fn sent(failure: FailureClass, retry_after_ms: Option<u64>) -> AttemptOutcome {
        let mut tracker = attempt::AttemptTracker::test_only(1);
        tracker.request_write_started().expect("允许记录写入");
        tracker.into_outcome_for_test(failure, retry_after_ms, None)
    }

    fn delivery_unknown(failure: FailureClass) -> AttemptOutcome {
        let mut tracker = attempt::AttemptTracker::test_only(1);
        tracker.write_state_unknown().expect("允许标记未知");
        tracker.into_outcome_for_test(failure, None, None)
    }

    fn trusted_pre_execution_rejected(
        failure: FailureClass,
        retry_after_ms: Option<u64>,
    ) -> AttemptOutcome {
        attempt::AttemptTracker::test_only(1).into_outcome_for_test(
            failure,
            retry_after_ms,
            Some(attempt::TrustedPreExecutionRejection::registered()),
        )
    }

    fn downstream_committed(failure: FailureClass) -> AttemptOutcome {
        let mut tracker = attempt::AttemptTracker::test_only(1);
        tracker.request_write_started().expect("允许记录写入");
        tracker
            .upstream_response_observed()
            .expect("允许记录响应头");
        tracker.downstream_committed().expect("允许下游提交");
        tracker.into_outcome_for_test(failure, None, None)
    }

    fn not_sent_for(permit: AttemptPermit, failure: FailureClass) -> attempt::AttemptCompletion {
        attempt::AttemptTracker::from_permit(permit).into_completion(failure, None, None)
    }

    fn sent_for(permit: AttemptPermit, failure: FailureClass) -> attempt::AttemptCompletion {
        let mut tracker = attempt::AttemptTracker::from_permit(permit);
        tracker.request_write_started().expect("允许记录写入");
        tracker.into_completion(failure, None, None)
    }

    fn plan_for_coordinator() -> RoutePlan {
        let candidates = [
            RouteCandidate::ready(target(1, 1), 0),
            RouteCandidate::ready(target(2, 2), 0),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        RoutePlanner::plan(&snapshot, 0).expect("存在可用目标")
    }

    #[test]
    fn runtime_source_stays_bounded() {
        let runtime = include_str!("lib.rs")
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
        assert!(code_lines <= 520, "运行时代码过大: {code_lines} 行");
    }

    #[test]
    fn identifier_rejects_zero() {
        assert_eq!(RouteTargetId::new(0), None);
        assert_eq!(RouteTargetId::new(9).map(RouteTargetId::get), Some(9));
    }

    #[test]
    fn snapshot_rejects_invalid_shape() {
        assert_eq!(
            RoutingSnapshot::new(version(1), &[], RoutingStrategy::Priority, 1).err(),
            Some(PlanError::NoCandidates)
        );

        let duplicate = [
            RouteCandidate::ready(target(1, 1), 0),
            RouteCandidate::ready(target(1, 2), 0),
        ];
        assert_eq!(
            RoutingSnapshot::new(version(1), &duplicate, RoutingStrategy::Priority, 1).err(),
            Some(PlanError::DuplicateTarget)
        );

        let candidates = [RouteCandidate::ready(target(1, 1), 0)];
        assert_eq!(
            RoutingSnapshot::new(version(1), &candidates, RoutingStrategy::Priority, 0).err(),
            Some(PlanError::InvalidMaxAttempts)
        );
    }

    #[test]
    fn priority_plan_filters_and_orders_targets() {
        let candidates = [
            RouteCandidate::ready(target(1, 20), 7),
            RouteCandidate::cooling_down(target(2, 1)),
            RouteCandidate::ready(target(3, 10), 9),
            RouteCandidate::disabled(target(4, 0)),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 3);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("存在可用目标");

        assert_eq!(plan.len(), 2);
        assert_eq!(plan.target_id(0), Some(id(3)));
        assert_eq!(plan.target_id(1), Some(id(1)));
        assert_eq!(plan.target_id(2), None);
    }

    #[test]
    fn round_robin_only_rotates_sorted_start() {
        let candidates = [
            RouteCandidate::ready(target(1, 10), 0),
            RouteCandidate::ready(target(2, 20), 0),
            RouteCandidate::ready(target(3, 30), 0),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::RoundRobin, 3);
        let plan = RoutePlanner::plan(&snapshot, 4).expect("存在可用目标");

        assert_eq!(plan.target_id(0), Some(id(2)));
        assert_eq!(plan.target_id(1), Some(id(3)));
        assert_eq!(plan.target_id(2), Some(id(1)));
    }

    #[test]
    fn least_penalty_is_deterministic() {
        let candidates = [
            RouteCandidate::ready(target(1, 10), 8),
            RouteCandidate::ready(target(2, 30), 2),
            RouteCandidate::ready(target(3, 20), 2),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::LeastPenalty, 3);
        let plan = RoutePlanner::plan(&snapshot, 99).expect("存在可用目标");

        assert_eq!(plan.target_id(0), Some(id(3)));
        assert_eq!(plan.target_id(1), Some(id(2)));
        assert_eq!(plan.target_id(2), Some(id(1)));
    }

    #[test]
    fn plan_is_bound_to_snapshot_version() {
        let candidates = [RouteCandidate::ready(target(1, 1), 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 1);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("存在可用目标");
        let stale = RoutingSnapshot::new(version(2), &candidates, RoutingStrategy::Priority, 1)
            .expect("测试快照有效");

        assert_eq!(plan.resolve(&stale, 0), Err(PlanError::StaleSnapshot));
        assert_eq!(
            plan.resolve(&snapshot, 0)
                .expect("版本一致")
                .map(|resolved| resolved.id()),
            Some(id(1))
        );
    }

    #[test]
    fn retry_advances_only_with_replay_proof() {
        let candidates = [
            RouteCandidate::ready(target(1, 1), 0),
            RouteCandidate::ready(target(2, 2), 0),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("存在可用目标");
        let policy = RetryPolicy::new(2, 5_000).expect("策略有效");
        let gate = RetryGate::new(policy);

        assert_eq!(
            gate.decide(&plan, 0, &not_sent(FailureClass::Connect, None)),
            RetryDecision::Advance { delay_ms: 0 }
        );
        assert_eq!(
            gate.decide(&plan, 0, &sent(FailureClass::Server, None)),
            RetryDecision::Stop(RetryStopReason::ReplayNotProven)
        );
        assert_eq!(
            gate.decide(&plan, 0, &delivery_unknown(FailureClass::Timeout)),
            RetryDecision::Stop(RetryStopReason::ReplayNotProven)
        );
    }

    #[test]
    fn only_trusted_zero_byte_rejection_honors_bounded_retry_after() {
        let candidates = [
            RouteCandidate::ready(target(1, 1), 0),
            RouteCandidate::ready(target(2, 2), 0),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("存在可用目标");
        let policy = RetryPolicy::new(2, 3_000).expect("策略有效");
        let gate = RetryGate::new(policy);

        assert_eq!(
            gate.decide(
                &plan,
                0,
                &trusted_pre_execution_rejected(FailureClass::RateLimited, Some(30_000))
            ),
            RetryDecision::Advance { delay_ms: 3_000 }
        );
        assert_eq!(
            gate.decide(&plan, 0, &sent(FailureClass::RateLimited, Some(1_000))),
            RetryDecision::Stop(RetryStopReason::ReplayNotProven)
        );
    }

    #[test]
    fn bytes_or_sse_semantics_never_replay_even_with_trusted_proof() {
        let candidates = [
            RouteCandidate::ready(target(1, 1), 0),
            RouteCandidate::ready(target(2, 2), 0),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("存在可用目标");
        let gate = RetryGate::new(RetryPolicy::new(2, 3_000).expect("策略有效"));

        let mut bytes_written = attempt::AttemptTracker::test_only(1);
        bytes_written.request_write_started().expect("允许记录写入");
        assert_eq!(
            gate.decide(
                &plan,
                0,
                &bytes_written.into_outcome_for_test(
                    FailureClass::RateLimited,
                    Some(1_000),
                    Some(attempt::TrustedPreExecutionRejection::registered()),
                )
            ),
            RetryDecision::Stop(RetryStopReason::ReplayNotProven)
        );

        let mut semantic_event = attempt::AttemptTracker::test_only(1);
        semantic_event
            .request_write_started()
            .expect("允许记录写入");
        semantic_event
            .upstream_response_observed()
            .expect("允许记录响应头");
        semantic_event
            .first_semantic_event_observed()
            .expect("允许记录首个语义事件");
        assert_eq!(
            gate.decide(
                &plan,
                0,
                &semantic_event.into_outcome_for_test(
                    FailureClass::RateLimited,
                    Some(1_000),
                    Some(attempt::TrustedPreExecutionRejection::registered()),
                )
            ),
            RetryDecision::Stop(RetryStopReason::ReplayNotProven)
        );
    }

    #[test]
    fn retry_stops_for_terminal_conditions() {
        let candidates = [
            RouteCandidate::ready(target(1, 1), 0),
            RouteCandidate::ready(target(2, 2), 0),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("存在可用目标");
        let policy = RetryPolicy::new(2, 1_000).expect("策略有效");
        let gate = RetryGate::new(policy);

        assert_eq!(
            gate.decide(&plan, 0, &delivery_unknown(FailureClass::Cancelled)),
            RetryDecision::Stop(RetryStopReason::Cancelled)
        );
        assert_eq!(
            gate.decide(&plan, 0, &downstream_committed(FailureClass::Server)),
            RetryDecision::Stop(RetryStopReason::DownstreamCommitted)
        );
        assert_eq!(
            gate.decide(&plan, 0, &not_sent(FailureClass::Authentication, None)),
            RetryDecision::Stop(RetryStopReason::NonRetryable)
        );
        assert_eq!(
            gate.decide(&plan, 1, &not_sent(FailureClass::Connect, None)),
            RetryDecision::Stop(RetryStopReason::Exhausted)
        );
    }

    #[test]
    fn coordinator_only_advances_once_and_in_plan_order() {
        let mut coordinator = plan_for_coordinator()
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"));
        let first = coordinator.start().expect("允许签发首次尝试");
        assert_eq!(first.index(), 0);
        assert_eq!(first.target(), id(1));
        assert!(coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(),
            Err(AttemptStartError::ActiveAttempt)
        ));

        let first_outcome = not_sent_for(first, FailureClass::Connect);
        let CoordinatorStep::Next {
            permit: second,
            delay_ms,
        } = coordinator
            .complete(first_outcome)
            .expect("零写入连接失败允许推进")
        else {
            panic!("应签发唯一的第二次尝试");
        };
        assert_eq!(delay_ms, 0);
        assert_eq!(second.index(), 1);
        assert_eq!(second.target(), id(2));
        assert!(coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(),
            Err(AttemptStartError::ActiveAttempt)
        ));

        let second_outcome = not_sent_for(second, FailureClass::Connect);
        assert!(matches!(
            coordinator.complete(second_outcome).expect("第二次可完成"),
            CoordinatorStep::Stop(RetryStopReason::Exhausted)
        ));
        assert!(coordinator.is_stopped());
        assert!(matches!(
            coordinator.start(),
            Err(AttemptStartError::Stopped)
        ));
    }

    #[test]
    fn coordinator_stops_after_written_request_or_regular_rate_limit() {
        let mut coordinator = plan_for_coordinator()
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"));
        let first = coordinator.start().expect("允许签发首次尝试");
        let written = sent_for(first, FailureClass::RateLimited);
        assert!(matches!(
            coordinator.complete(written).expect("写后结果可完成"),
            CoordinatorStep::Stop(RetryStopReason::ReplayNotProven)
        ));
        assert!(coordinator.is_stopped());
        assert!(!coordinator.has_active_attempt());
    }

    #[test]
    fn coordinator_never_issues_next_after_sse_or_downstream_commit() {
        let mut semantic = plan_for_coordinator()
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"));
        let permit = semantic.start().expect("允许签发首次尝试");
        let mut tracker = attempt::AttemptTracker::from_permit(permit);
        tracker.request_write_started().expect("允许记录写入");
        tracker
            .upstream_response_observed()
            .expect("允许记录响应头");
        tracker
            .first_semantic_event_observed()
            .expect("允许记录首个语义事件");
        let completion = tracker.into_completion(FailureClass::Server, None, None);
        assert!(matches!(
            semantic.complete(completion).expect("语义事件可完成"),
            CoordinatorStep::Stop(RetryStopReason::ReplayNotProven)
        ));
        assert!(semantic.is_stopped());

        let mut downstream = plan_for_coordinator()
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"));
        let permit = downstream.start().expect("允许签发首次尝试");
        let mut tracker = attempt::AttemptTracker::from_permit(permit);
        tracker.downstream_committed().expect("允许下游提交");
        let completion = tracker.into_completion(FailureClass::Server, None, None);
        assert!(matches!(
            downstream.complete(completion).expect("下游提交可完成"),
            CoordinatorStep::Stop(RetryStopReason::DownstreamCommitted)
        ));
        assert!(downstream.is_stopped());
    }

    #[test]
    fn other_coordinator_completion_fails_closed_without_next_permit() {
        let mut coordinator = plan_for_coordinator()
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"));
        let mut other = plan_for_coordinator()
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"));
        let foreign = other.start().expect("允许签发另一请求的首次尝试");
        let mismatched = not_sent_for(foreign, FailureClass::Connect);
        let _current = coordinator.start().expect("允许签发当前请求的首次尝试");

        assert!(matches!(
            coordinator.complete(mismatched),
            Err(AttemptCompleteError::Mismatched)
        ));
        assert!(coordinator.is_stopped());
        assert!(!coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(),
            Err(AttemptStartError::Stopped)
        ));
    }

    #[test]
    fn route_plan_stays_small_and_stack_only() {
        assert!(core::mem::size_of::<RoutePlan>() <= 160);
        assert!(core::mem::size_of::<AttemptCoordinator>() <= 224);
        assert_eq!(core::mem::size_of::<RouteTargetId>(), 8);
    }
}
