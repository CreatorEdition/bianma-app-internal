//! Bianma 的低开销 Stage-first 路由规划核心。
//!
//! 本 crate 只根据不可变内存快照生成有界的 `A -> B -> C` 计划，并定义一次发送
//! 结束后的保守重放门禁；不执行 HTTP、等待、健康检查、凭据解析、数据库访问或
//! 上下文治理。客户端差异在更上层归一化，默认配置因此可以共用同一份路由策略。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// 单次激活计划允许的最大候选数量。
pub const MAX_ROUTE_TARGETS: usize = 16;

// 当前切片只定义固定大小的发送证据合同，生产 Transport 尚未接线。状态构造路径必须
// 留在 crate 内，避免外部调用方把普通 HTTP 状态或错误文本伪装成安全重放证据。
#[cfg_attr(not(test), allow(dead_code))]
mod attempt;
#[cfg_attr(not(test), allow(dead_code))]
mod coordinator;

pub use attempt::{
    AttemptFailure, AttemptOutcome, ChargeState, DeliveryState, DownstreamCommitState,
    ReplayDecision, ReplayPermitReason, ReplayStopReason, SendPhase, UpstreamWriteState,
};
macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            /// 从非零整数构造稳定标识。
            pub const fn new(value: u64) -> Option<Self> {
                if value == 0 { None } else { Some(Self(value)) }
            }

            /// 返回底层稳定标识。
            pub const fn get(self) -> u64 { self.0 }
        }
    };
}

id_type!(/// 路由快照版本。
    SnapshotVersion);
id_type!(/// 路由阶段的稳定标识（例如 A、B、C 的内部 ID）。
    RouteStageId);
id_type!(/// 路由目标稳定标识。
    RouteTargetId);
id_type!(/// 站点稳定标识。
    SiteId);
id_type!(/// 具体模型部署稳定标识。
    ModelDeploymentId);
id_type!(/// 上游端点稳定标识。
    EndpointId);
id_type!(/// 目标内部账户选择合同稳定标识。
    AccountSelectorId);

/// 一个绑定站点、具体模型部署、端点和账户选择合同的目标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteTarget {
    id: RouteTargetId,
    site: SiteId,
    deployment: ModelDeploymentId,
    endpoint: EndpointId,
    account_selector: AccountSelectorId,
}

impl RouteTarget {
    /// 创建一个已经过上层编译器绑定的目标。
    pub const fn new(
        id: RouteTargetId,
        site: SiteId,
        deployment: ModelDeploymentId,
        endpoint: EndpointId,
        account_selector: AccountSelectorId,
    ) -> Self {
        Self {
            id,
            site,
            deployment,
            endpoint,
            account_selector,
        }
    }

    /// 返回目标 ID。
    pub const fn id(self) -> RouteTargetId {
        self.id
    }
    /// 返回站点 ID。
    pub const fn site(self) -> SiteId {
        self.site
    }
    /// 返回模型部署 ID。
    pub const fn deployment(self) -> ModelDeploymentId {
        self.deployment
    }
    /// 返回端点 ID。
    pub const fn endpoint(self) -> EndpointId {
        self.endpoint
    }
    /// 返回账户选择合同 ID。
    pub const fn account_selector(self) -> AccountSelectorId {
        self.account_selector
    }
}

/// 目标在当前快照中的选择状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetState {
    /// 可选目标；惩罚越小越优先。
    Ready {
        /// 由上层快照编译器计算的有界惩罚值。
        penalty: u16,
    },
    /// 暂时不可选。冷却的具体原因由后续状态模块负责。
    CoolingDown,
    /// 被用户或配置明确禁用。
    Disabled,
}

/// 与阶段绑定的快照候选。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteCandidate {
    stage: RouteStageId,
    target: RouteTarget,
    state: TargetState,
}

impl RouteCandidate {
    /// 创建可选候选。
    pub const fn ready(stage: RouteStageId, target: RouteTarget, penalty: u16) -> Self {
        Self {
            stage,
            target,
            state: TargetState::Ready { penalty },
        }
    }

    /// 创建冷却候选。
    pub const fn cooling_down(stage: RouteStageId, target: RouteTarget) -> Self {
        Self {
            stage,
            target,
            state: TargetState::CoolingDown,
        }
    }

    /// 创建禁用候选。
    pub const fn disabled(stage: RouteStageId, target: RouteTarget) -> Self {
        Self {
            stage,
            target,
            state: TargetState::Disabled,
        }
    }

    /// 返回阶段 ID。
    pub const fn stage(self) -> RouteStageId {
        self.stage
    }
    /// 返回目标。
    pub const fn target(self) -> RouteTarget {
        self.target
    }
    /// 返回选择状态。
    pub const fn state(self) -> TargetState {
        self.state
    }
}

/// 阶段内的候选选择策略。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingStrategy {
    /// 保留每个阶段与候选的声明顺序。
    Priority,
    /// 只在当前阶段的可用候选中轮转。
    RoundRobin,
    /// 只在当前阶段按 penalty 和声明顺序选择。
    LeastPenalty,
}

/// 快照或计划形状无效时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanError {
    /// 快照没有候选。
    NoCandidates,
    /// 候选超过固定容量。
    TooManyCandidates,
    /// 尝试预算为零或超过固定容量。
    InvalidMaxAttempts,
    /// 预算不足以覆盖每一个已配置阶段。
    InsufficientMaxAttemptsForStages,
    /// 目标 ID 重复。
    DuplicateTarget,
    /// 模型部署 ID 重复。
    DuplicateDeployment,
    /// 同一阶段在快照中出现多个不连续片段。
    NonContiguousStage,
    /// 没有任何可选目标。
    NoEligibleTargets,
    /// 计划索引无法解析为目标。
    UnknownTarget,
}

/// 借用候选数组的不可变路由快照。
pub struct RoutingSnapshot<'c> {
    version: SnapshotVersion,
    candidates: &'c [RouteCandidate],
    strategy: RoutingStrategy,
    max_attempts: u8,
}

impl<'c> RoutingSnapshot<'c> {
    /// 校验并创建快照。热路径只借用调用方的内存，不分配堆对象。
    pub fn new(
        version: SnapshotVersion,
        candidates: &'c [RouteCandidate],
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

        let mut stage_count = 0usize;
        for (index, candidate) in candidates.iter().enumerate() {
            if candidates[..index]
                .iter()
                .any(|p| p.target.id == candidate.target.id)
            {
                return Err(PlanError::DuplicateTarget);
            }
            if candidates[..index]
                .iter()
                .any(|p| p.target.deployment == candidate.target.deployment)
            {
                return Err(PlanError::DuplicateDeployment);
            }
            if index == 0 || candidates[index - 1].stage != candidate.stage {
                if candidates[..index]
                    .iter()
                    .any(|p| p.stage == candidate.stage)
                {
                    return Err(PlanError::NonContiguousStage);
                }
                stage_count += 1;
            }
        }
        if usize::from(max_attempts) < stage_count {
            return Err(PlanError::InsufficientMaxAttemptsForStages);
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
    /// 返回策略。
    pub const fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }
    /// 返回最大尝试预算。
    pub const fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    fn candidate(&self, index: u8) -> Option<&RouteCandidate> {
        self.candidates.get(usize::from(index))
    }
}

#[derive(Clone, Copy)]
struct CandidateOrder {
    index: u8,
    penalty: u16,
}

/// 无状态、固定容量、无 I/O 的 Stage-first 规划器。
pub struct RoutePlanner;

impl RoutePlanner {
    /// 为每个存在可用目标的阶段签发最多一个计划位置。
    pub fn plan<'s, 'c>(
        snapshot: &'s RoutingSnapshot<'c>,
        request_cursor: u64,
    ) -> Result<RoutePlan<'s, 'c>, PlanError> {
        let mut target_indices = [u8::MAX; MAX_ROUTE_TARGETS];
        let mut len = 0usize;
        let mut stage_start = 0usize;

        while stage_start < snapshot.candidates.len() && len < usize::from(snapshot.max_attempts) {
            let stage = snapshot.candidates[stage_start].stage;
            let mut stage_end = stage_start;
            let mut orders = [CandidateOrder {
                index: 0,
                penalty: 0,
            }; MAX_ROUTE_TARGETS];
            let mut eligible = 0usize;

            while stage_end < snapshot.candidates.len()
                && snapshot.candidates[stage_end].stage == stage
            {
                if let TargetState::Ready { penalty } = snapshot.candidates[stage_end].state {
                    orders[eligible] = CandidateOrder {
                        index: stage_end as u8,
                        penalty,
                    };
                    eligible += 1;
                }
                stage_end += 1;
            }

            if eligible > 0 {
                let chosen = match snapshot.strategy {
                    RoutingStrategy::Priority => 0,
                    RoutingStrategy::RoundRobin => request_cursor as usize % eligible,
                    RoutingStrategy::LeastPenalty => orders[..eligible]
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, item)| (item.penalty, item.index))
                        .map(|(index, _)| index)
                        .unwrap_or(0),
                };
                target_indices[len] = orders[chosen].index;
                len += 1;
            }
            stage_start = stage_end;
        }

        if len == 0 {
            return Err(PlanError::NoEligibleTargets);
        }
        Ok(RoutePlan {
            snapshot,
            target_indices,
            len: len as u8,
        })
    }
}

/// 绑定生成快照的固定容量计划。
pub struct RoutePlan<'s, 'c> {
    snapshot: &'s RoutingSnapshot<'c>,
    target_indices: [u8; MAX_ROUTE_TARGETS],
    len: u8,
}

impl<'s, 'c> RoutePlan<'s, 'c> {
    /// 返回快照版本。
    pub const fn snapshot_version(&self) -> SnapshotVersion {
        self.snapshot.version
    }
    /// 返回计划位置数量。
    pub const fn len(&self) -> u8 {
        self.len
    }
    /// 判断计划是否为空。
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// 返回指定尝试对应的 Stage。
    pub fn stage_id(&self, attempt_index: u8) -> Result<Option<RouteStageId>, PlanError> {
        let Some(index) = self.target_index(attempt_index)? else {
            return Ok(None);
        };
        self.snapshot
            .candidate(index)
            .map(|candidate| Some(candidate.stage))
            .ok_or(PlanError::UnknownTarget)
    }
    /// 返回指定尝试对应的目标。
    pub fn resolve(&self, attempt_index: u8) -> Result<Option<RouteTarget>, PlanError> {
        let Some(index) = self.target_index(attempt_index)? else {
            return Ok(None);
        };
        self.snapshot
            .candidate(index)
            .map(|candidate| Some(candidate.target))
            .ok_or(PlanError::UnknownTarget)
    }
    /// 返回指定尝试对应的目标 ID。
    pub fn target_id(&self, attempt_index: u8) -> Result<Option<RouteTargetId>, PlanError> {
        self.resolve(attempt_index)
            .map(|target| target.map(RouteTarget::id))
    }
    fn target_index(&self, attempt_index: u8) -> Result<Option<u8>, PlanError> {
        if attempt_index >= self.len {
            return Ok(None);
        }
        let index = self.target_indices[usize::from(attempt_index)];
        if index == u8::MAX {
            return Err(PlanError::UnknownTarget);
        }
        Ok(Some(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id<T>(value: u64, make: fn(u64) -> Option<T>) -> T {
        make(value).expect("测试 ID 非零")
    }
    fn target(value: u64, deployment: u64) -> RouteTarget {
        RouteTarget::new(
            id(value, RouteTargetId::new),
            id(value, SiteId::new),
            id(deployment, ModelDeploymentId::new),
            id(value, EndpointId::new),
            id(value, AccountSelectorId::new),
        )
    }
    fn candidate(stage: u64, value: u64, deployment: u64) -> RouteCandidate {
        RouteCandidate::ready(id(stage, RouteStageId::new), target(value, deployment), 0)
    }
    fn snapshot<'c>(
        candidates: &'c [RouteCandidate],
        strategy: RoutingStrategy,
        max: u8,
    ) -> RoutingSnapshot<'c> {
        RoutingSnapshot::new(id(1, SnapshotVersion::new), candidates, strategy, max)
            .expect("快照有效")
    }

    #[test]
    fn rejects_zero_identifiers_and_invalid_shape() {
        assert_eq!(RouteTargetId::new(0), None);
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &[],
                RoutingStrategy::Priority,
                1
            )
            .err(),
            Some(PlanError::NoCandidates)
        );
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &[candidate(1, 1, 1), candidate(1, 1, 2)],
                RoutingStrategy::Priority,
                2
            )
            .err(),
            Some(PlanError::DuplicateTarget)
        );
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &[candidate(1, 1, 1), candidate(2, 2, 1)],
                RoutingStrategy::Priority,
                2
            )
            .err(),
            Some(PlanError::DuplicateDeployment)
        );
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &[candidate(1, 1, 1), candidate(2, 2, 2), candidate(1, 3, 3)],
                RoutingStrategy::Priority,
                3
            )
            .err(),
            Some(PlanError::NonContiguousStage)
        );
    }

    #[test]
    fn rejects_invalid_capacity_budget_and_empty_eligibility() {
        let one = [candidate(1, 1, 1)];
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &one,
                RoutingStrategy::Priority,
                0
            )
            .err(),
            Some(PlanError::InvalidMaxAttempts)
        );
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &one,
                RoutingStrategy::Priority,
                17
            )
            .err(),
            Some(PlanError::InvalidMaxAttempts)
        );
        let first = candidate(1, 1, 1);
        let too_many = [first; MAX_ROUTE_TARGETS + 1];
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &too_many,
                RoutingStrategy::Priority,
                16
            )
            .err(),
            Some(PlanError::TooManyCandidates)
        );
        let two_stages = [candidate(1, 1, 1), candidate(2, 2, 2)];
        assert_eq!(
            RoutingSnapshot::new(
                id(1, SnapshotVersion::new),
                &two_stages,
                RoutingStrategy::Priority,
                1
            )
            .err(),
            Some(PlanError::InsufficientMaxAttemptsForStages)
        );
        let unavailable = [RouteCandidate::disabled(
            id(1, RouteStageId::new),
            target(1, 1),
        )];
        let snapshot = snapshot(&unavailable, RoutingStrategy::Priority, 1);
        assert_eq!(
            RoutePlanner::plan(&snapshot, 0).err(),
            Some(PlanError::NoEligibleTargets)
        );
    }

    #[test]
    fn priority_and_fallback_are_stage_first() {
        let candidates = [
            RouteCandidate::cooling_down(id(1, RouteStageId::new), target(1, 1)),
            candidate(1, 2, 2),
            candidate(2, 3, 3),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("计划有效");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan.stage_id(0).unwrap(), Some(id(1, RouteStageId::new)));
        assert_eq!(plan.target_id(0).unwrap(), Some(id(2, RouteTargetId::new)));
        assert_eq!(plan.target_id(1).unwrap(), Some(id(3, RouteTargetId::new)));
    }

    #[test]
    fn round_robin_never_crosses_stage_boundary() {
        let candidates = [candidate(1, 1, 1), candidate(1, 2, 2), candidate(2, 3, 3)];
        let snapshot = snapshot(&candidates, RoutingStrategy::RoundRobin, 2);
        let plan = RoutePlanner::plan(&snapshot, 1).expect("计划有效");
        assert_eq!(plan.target_id(0).unwrap(), Some(id(2, RouteTargetId::new)));
        assert_eq!(plan.target_id(1).unwrap(), Some(id(3, RouteTargetId::new)));
    }

    #[test]
    fn least_penalty_is_local_to_stage() {
        let candidates = [
            RouteCandidate::ready(id(1, RouteStageId::new), target(1, 1), 9),
            RouteCandidate::ready(id(1, RouteStageId::new), target(2, 2), 2),
            candidate(2, 3, 3),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::LeastPenalty, 2);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("计划有效");
        assert_eq!(plan.target_id(0).unwrap(), Some(id(2, RouteTargetId::new)));
        assert_eq!(plan.target_id(1).unwrap(), Some(id(3, RouteTargetId::new)));
        assert_eq!(plan.resolve(2).unwrap(), None);
    }

    #[test]
    fn plan_stays_small_and_borrows_original_snapshot() {
        assert!(core::mem::size_of::<RoutePlan<'_, '_>>() <= 32);
        let candidates = [candidate(1, 1, 1)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 1);
        let plan = RoutePlanner::plan(&snapshot, 0).expect("计划有效");
        assert_eq!(plan.snapshot_version(), snapshot.version());
    }
}
