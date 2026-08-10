//! Bianma 的低开销纯 Rust 路由决策核心。
//!
//! 本 crate 只负责验证不可变路由/账户选择合同、基于内存快照生成有界路由计划，并在
//! 一次尝试结束后给出保守的下一步决策。它不依赖 Tauri、数据库、网络客户端、异步
//! 运行时或 ContextPipeline。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// 单个路由计划允许的最大目标数。
pub const MAX_ROUTE_TARGETS: usize = 16;

// 当前切片只定义合同，生产 Transport 适配器尚未接线；不能为消除未使用告警而暴露
// 可伪造的 Attempt 构造路径。单测会覆盖该内部状态机。
mod account_credential;
mod account_selector;
#[cfg_attr(not(test), allow(dead_code))]
mod credential_authorization;
#[cfg_attr(not(test), allow(dead_code))]
mod attempt;
mod compiled_snapshot;
#[cfg_attr(not(test), allow(dead_code))]
mod coordinator;
#[cfg_attr(not(test), allow(dead_code))]
mod health;
mod ingress;
mod model_deployment;
#[cfg_attr(not(test), allow(dead_code))]
mod selection_cooldown;
mod selection_input;
#[cfg_attr(not(test), allow(dead_code))]
mod selection_lease;
mod selection_runtime_layout;

pub use account_credential::*;
pub(crate) use account_selector::AccountSelectionCandidates;
pub use account_selector::{
    AccountSelectorCatalog, AccountSelectorCatalogError, AccountSelectorDefinition,
    AccountSelectorError, AccountSelectorMember, CredentialSelectionPolicy, QuotaSelectionUnit,
    QuotaTopologySource, SelectorAffinitySalt, SelectorRevision, MAX_ACCOUNT_SELECTORS,
    MAX_ACCOUNT_SELECTOR_MEMBERS, MAX_QUOTA_GROUPS_PER_UNIT, MAX_QUOTA_SELECTION_UNITS,
};
pub use attempt::{
    AttemptOutcome, ChargeState, DeliveryState, DownstreamCommitState, SendPhase,
    UpstreamWriteState,
};
pub use compiled_snapshot::*;
use coordinator::{AttemptCoordinator, AttemptCoordinatorBuildError};
pub use health::*;
pub use ingress::*;
pub use model_deployment::*;
pub(crate) use selection_input::AccountSelectionRequest;
pub use selection_input::{SelectionSession, SessionAffinityAlias, SESSION_AFFINITY_ALIAS_BYTES};
pub(crate) use selection_runtime_layout::SelectionRuntimeLayout;
pub use selection_runtime_layout::{
    AccountRuntimeDefinition, CredentialRuntimeDefinition, QuotaGroupRuntimeDefinition,
    SelectionRuntimeDefinitions, SelectionRuntimeDefinitionsError, SelectionRuntimeLayoutError,
    MAX_TRACKED_ACCOUNTS, MAX_TRACKED_CREDENTIALS, MAX_TRACKED_QUOTA_GROUPS,
};

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
    ///
    /// 宿主必须在静态路由或资源身份语义变化时单调递增此值；尤其不得以同一版本把一个
    /// Account、Credential 或 QuotaGroup 标识重新绑定为不同实际资源。资源冷却等跨请求
    /// 状态以该版本作为无快照指针的配置代边界。
    SnapshotVersion
);
id_type!(
    /// 路由目标标识。
    RouteTargetId
);
id_type!(
    /// 路由阶段的稳定标识。
    RouteStageId
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
    /// 路由目标内嵌账户选择合同的稳定标识。
    AccountSelectorId
);
id_type!(
    /// 账户标识。
    AccountId
);
id_type!(
    /// 凭据标识。
    CredentialId
);
id_type!(
    /// 额度组标识。
    QuotaGroupId
);
id_type!(
    /// 账户选择合同内独立额度单元的稳定标识。
    QuotaSelectionUnitId
);

impl RouteTargetId {
    const INVALID: Self = Self(0);
}

/// 一个已经由快照编译器绑定完整 Target 身份与账户选择合同的路由目标。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteTarget {
    id: RouteTargetId,
    site: SiteId,
    deployment: ModelDeploymentId,
    endpoint: EndpointId,
    account_selector: AccountSelectorId,
}

impl RouteTarget {
    /// 构造一个完整 Target 身份与账户选择合同的路由目标。
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

    /// 返回账户选择合同标识。
    pub const fn account_selector(self) -> AccountSelectorId {
        self.account_selector
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

/// 与稳定 RouteStage 绑定的快照目标候选。
///
/// 同一 Stage 的候选必须在快照中连续出现；候选顺序定义 Stage 的执行顺序，禁止靠
/// Target 数值、惩罚或全局游标让后续 Stage 抢占前序 Stage。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteCandidate {
    stage: RouteStageId,
    target: RouteTarget,
    state: TargetState,
}

impl RouteCandidate {
    /// 创建可用候选。
    pub const fn ready(stage: RouteStageId, target: RouteTarget, penalty: u16) -> Self {
        Self {
            stage,
            target,
            state: TargetState::Ready { penalty },
        }
    }

    /// 创建冷却中的候选。
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

    /// 返回所属的稳定路由阶段。
    pub const fn stage(self) -> RouteStageId {
        self.stage
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
    /// 严格按阶段顺序与阶段内声明顺序形成 `A -> B -> C`。
    Priority,
    /// 按请求游标只在每个阶段内轮转，不得让后续阶段抢占前序阶段。
    RoundRobin,
    /// 只在每个阶段内优先选择当前惩罚值更低的目标。
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
    /// 总尝试预算不足以首次访问每个已配置路由阶段。
    InsufficientMaxAttemptsForStages,
    /// 同一快照重复出现相同目标。
    DuplicateTarget,
    /// 同一已编译计划重复引用相同模型部署。
    DuplicateDeployment,
    /// 路由目标引用了当前编译代中不存在的账户选择合同。
    UnknownAccountSelector,
    /// 路由目标引用了当前编译代中不存在的模型部署。
    UnknownModelDeployment,
    /// 路由目标的站点与模型部署定义不一致。
    TargetDeploymentSiteMismatch,
    /// 路由目标的端点与模型部署定义不一致。
    TargetDeploymentEndpointMismatch,
    /// 账户选择成员引用了当前编译代中不存在的账户。
    UnknownAccount,
    /// 账户选择成员引用了当前编译代中不存在的凭据。
    UnknownCredential,
    /// 账户选择成员声明的账户与凭据 owner 不一致。
    CredentialAccountMismatch,
    /// 账户所属站点与当前路由目标的模型部署站点不一致。
    AccountDeploymentSiteMismatch,
    /// 同一阶段的候选在快照中被拆成了不连续的多个片段。
    NonContiguousStage,
    /// 当前没有可用目标。
    NoEligibleTargets,
    /// 已验证路由请求绑定的快照版本与实际规划快照不一致。
    RequestSnapshotMismatch,
    /// 动态 eligibility 未与当前快照的版本或候选身份完全匹配。
    EligibilitySnapshotMismatch,
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
    pub(crate) fn new(
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
        let mut stage_count = 0usize;
        for (index, candidate) in candidates.iter().enumerate() {
            if candidates[..index]
                .iter()
                .any(|previous| previous.target.id == candidate.target.id)
            {
                return Err(PlanError::DuplicateTarget);
            }
            if candidates[..index]
                .iter()
                .any(|previous| previous.target.deployment == candidate.target.deployment)
            {
                return Err(PlanError::DuplicateDeployment);
            }
            if index == 0 || candidates[index - 1].stage != candidate.stage {
                if candidates[..index]
                    .iter()
                    .any(|previous| previous.stage == candidate.stage)
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

    /// 返回路由策略。
    pub const fn strategy(&self) -> RoutingStrategy {
        self.strategy
    }

    /// 返回最大尝试数。
    pub const fn max_attempts(&self) -> u8 {
        self.max_attempts
    }

    /// 返回已通过快照形状验证的全部候选，仅供 crate 内编译期合同扫描。
    ///
    /// 请求热路径不能使用此入口建立动态额度状态；全局额度组布局只允许在快照激活前
    /// 扫描这些候选。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn candidates(&self) -> &'a [RouteCandidate] {
        self.candidates
    }

    fn resolve(&self, target_id: RouteTargetId) -> Option<&RouteTarget> {
        self.candidates
            .iter()
            .find(|candidate| candidate.target.id == target_id)
            .map(|candidate| &candidate.target)
    }

    fn stage_for(&self, target_id: RouteTargetId) -> Option<RouteStageId> {
        self.candidates
            .iter()
            .find(|candidate| candidate.target.id == target_id)
            .map(|candidate| candidate.stage)
    }

}

#[derive(Clone, Copy)]
struct CandidateOrder {
    index: u8,
    penalty: u16,
}

impl CandidateOrder {
    const EMPTY: Self = Self {
        index: 0,
        penalty: 0,
    };
}

/// 无状态、无分配的 Stage-first 路由计划器。
pub struct RoutePlanner;

impl RoutePlanner {
    /// 根据已验证的路由请求、同版本快照、同代 eligibility 与请求游标生成有界计划。
    ///
    /// 每个有可用候选的 Stage 只签发一个 Target；RoundRobin 与 LeastPenalty 仅决定
    /// 当前 Stage 的首选 Target。动态同 Stage failover 需要后续独立的策略、额度和
    /// 健康合同，不能由本编译器把同级 Target 静默扩成自动重试池。
    pub fn plan<'snapshot, 'candidates>(
        request: &VerifiedRouteDispatch,
        snapshot: &'snapshot RoutingSnapshot<'candidates>,
        eligibility: &RouteEligibility<'snapshot, 'candidates>,
        request_cursor: u64,
    ) -> Result<RoutePlan<'snapshot, 'candidates>, PlanError> {
        if request.snapshot() != snapshot.version {
            return Err(PlanError::RequestSnapshotMismatch);
        }
        if !eligibility.matches_snapshot(snapshot) {
            return Err(PlanError::EligibilitySnapshotMismatch);
        }
        let mut target_ids = [RouteTargetId::INVALID; MAX_ROUTE_TARGETS];
        let mut plan_len = 0usize;
        let mut stage_start = 0usize;

        while stage_start < snapshot.candidates.len()
            && plan_len < usize::from(snapshot.max_attempts)
        {
            let stage = snapshot.candidates[stage_start].stage;
            let mut ordered = [CandidateOrder::EMPTY; MAX_ROUTE_TARGETS];
            let mut eligible_count = 0usize;
            let mut stage_end = stage_start;

            while stage_end < snapshot.candidates.len()
                && snapshot.candidates[stage_end].stage == stage
            {
                let candidate = snapshot.candidates[stage_end];
                if let TargetState::Ready { penalty } = candidate.state {
                    if eligibility.allows_index(stage_end) {
                        ordered[eligible_count] = CandidateOrder {
                            index: stage_end as u8,
                            penalty,
                        };
                        eligible_count += 1;
                    }
                }
                stage_end += 1;
            }

            if eligible_count > 0 {
                let eligible = &mut ordered[..eligible_count];
                if snapshot.strategy == RoutingStrategy::LeastPenalty {
                    eligible.sort_unstable_by_key(|item| (item.penalty, item.index));
                }
                let rotation = if snapshot.strategy == RoutingStrategy::RoundRobin {
                    request_cursor as usize % eligible_count
                } else {
                    0
                };
                let source = eligible[rotation];
                target_ids[plan_len] = snapshot.candidates[usize::from(source.index)].target.id;
                plan_len += 1;
            }
            stage_start = stage_end;
        }

        if plan_len == 0 {
            return Err(PlanError::NoEligibleTargets);
        }

        Ok(RoutePlan {
            snapshot,
            target_ids,
            len: plan_len as u8,
        })
    }
}

/// 固定容量、无堆分配的路由执行计划。
pub struct RoutePlan<'snapshot, 'candidates> {
    snapshot: &'snapshot RoutingSnapshot<'candidates>,
    target_ids: [RouteTargetId; MAX_ROUTE_TARGETS],
    len: u8,
}

impl<'snapshot, 'candidates> RoutePlan<'snapshot, 'candidates> {
    /// 消费计划并创建只允许线性推进的 Attempt 协调器。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn into_attempt_coordinator(
        self,
        policy: RetryPolicy,
    ) -> Result<AttemptCoordinator<'snapshot, 'candidates>, AttemptCoordinatorBuildError> {
        AttemptCoordinator::new(self, policy)
    }

    /// 返回快照版本。
    pub const fn snapshot_version(&self) -> SnapshotVersion {
        self.snapshot.version
    }

    /// 返回计划中的尝试数。
    pub const fn len(&self) -> u8 {
        self.len
    }

    /// 判断计划是否为空。
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 在计划绑定的快照中解析指定尝试所属的稳定路由阶段。
    pub fn stage_id(&self, attempt_index: u8) -> Result<Option<RouteStageId>, PlanError> {
        let Some(target_id) = self.target_id(attempt_index) else {
            return Ok(None);
        };
        self.snapshot
            .stage_for(target_id)
            .map(Some)
            .ok_or(PlanError::UnknownTarget)
    }

    /// 返回指定尝试对应的目标标识。
    pub fn target_id(&self, attempt_index: u8) -> Option<RouteTargetId> {
        if attempt_index >= self.len {
            return None;
        }
        Some(self.target_ids[usize::from(attempt_index)])
    }

    /// 在计划绑定的快照中解析指定尝试的完整目标。
    pub fn resolve(&self, attempt_index: u8) -> Result<Option<&RouteTarget>, PlanError> {
        let Some(target_id) = self.target_id(attempt_index) else {
            return Ok(None);
        };
        self.snapshot
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

    pub(crate) const fn max_attempts(&self) -> u8 {
        self.max_attempts
    }
}

/// 停止推进路由计划的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryStopReason {
    /// 没有下一个目标或预算已耗尽。
    Exhausted,
    /// 当前 eligibility 下没有可签发的计划内目标。
    NoEligibleTargets,
    /// 当前 eligibility 未与已编译计划同代匹配。
    EligibilityMismatch,
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
    use super::attempt::test_rate_limit_observation;
    use super::coordinator::{
        AttemptCompleteError, AttemptCoordinatorBuildError, AttemptPermit, AttemptStartError,
        CoordinatorStep,
    };
    use super::selection_input::{AccountSelectionEligibility, AccountSelectionEligibilityError};
    use super::selection_runtime_layout::SelectionRuntimeBinding;
    use super::*;

    fn id(value: u64) -> RouteTargetId {
        RouteTargetId::new(value).expect("测试 ID 非零")
    }

    fn stage(value: u64) -> RouteStageId {
        RouteStageId::new(value).expect("测试阶段 ID 非零")
    }

    fn target(value: u64) -> RouteTarget {
        target_with_deployment(value, value)
    }

    fn target_with_deployment(value: u64, deployment_value: u64) -> RouteTarget {
        target_with_selector(value, deployment_value, value)
    }

    fn target_with_selector(value: u64, deployment_value: u64, selector_value: u64) -> RouteTarget {
        target_with_binding(value, value, deployment_value, value, selector_value)
    }

    fn target_with_binding(
        target_value: u64,
        site_value: u64,
        deployment_value: u64,
        endpoint_value: u64,
        selector_value: u64,
    ) -> RouteTarget {
        RouteTarget::new(
            id(target_value),
            SiteId::new(site_value).expect("站点 ID 非零"),
            ModelDeploymentId::new(deployment_value).expect("部署 ID 非零"),
            EndpointId::new(endpoint_value).expect("端点 ID 非零"),
            AccountSelectorId::new(selector_value).expect("账户选择合同 ID 非零"),
        )
    }

    fn deployment_definition(
        deployment_value: u64,
        site_value: u64,
        endpoint_value: u64,
    ) -> ModelDeploymentDefinition {
        ModelDeploymentDefinition::new(
            ModelDeploymentId::new(deployment_value).expect("测试部署 ID 非零"),
            SiteId::new(site_value).expect("测试站点 ID 非零"),
            EndpointId::new(endpoint_value).expect("测试端点 ID 非零"),
        )
    }

    fn account_definition(account_value: u64, site_value: u64) -> AccountDefinition {
        AccountDefinition::new(
            AccountId::new(account_value).expect("测试账户 ID 非零"),
            SiteId::new(site_value).expect("测试站点 ID 非零"),
        )
    }

    fn credential_definition(credential_value: u64, account_value: u64) -> CredentialDefinition {
        CredentialDefinition::new(
            CredentialId::new(credential_value).expect("测试凭据 ID 非零"),
            AccountId::new(account_value).expect("测试账户 ID 非零"),
        )
    }

    fn runtime_definition(group_value: u64, max_inflight: u16) -> QuotaGroupRuntimeDefinition {
        QuotaGroupRuntimeDefinition::new(
            QuotaGroupId::new(group_value).expect("测试额度组 ID 非零"),
            core::num::NonZeroU16::new(max_inflight).expect("测试在途上限非零"),
        )
    }

    fn account_runtime_definition(
        account_value: u64,
        max_inflight: u16,
    ) -> AccountRuntimeDefinition {
        AccountRuntimeDefinition::new(
            AccountId::new(account_value).expect("测试账户 ID 非零"),
            core::num::NonZeroU16::new(max_inflight).expect("测试在途上限非零"),
        )
    }

    fn credential_runtime_definition(
        credential_value: u64,
        max_inflight: u16,
    ) -> CredentialRuntimeDefinition {
        CredentialRuntimeDefinition::new(
            CredentialId::new(credential_value).expect("测试凭据 ID 非零"),
            core::num::NonZeroU16::new(max_inflight).expect("测试在途上限非零"),
        )
    }

    fn selector_definition<'a>(
        selector_value: u64,
        units: &'a [QuotaSelectionUnit<'a>],
        members: &'a [AccountSelectorMember],
    ) -> AccountSelectorDefinition<'a> {
        AccountSelectorDefinition::new(
            AccountSelectorId::new(selector_value).expect("账户选择合同 ID 非零"),
            SelectorRevision::new(1).expect("账户选择合同修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            units,
            members,
        )
        .expect("测试账户选择合同有效")
    }

    fn compile_result<'a>(
        candidates: &'a [RouteCandidate],
        deployments: &'a [ModelDeploymentDefinition],
        accounts: &'a [AccountDefinition],
        credentials: &'a [CredentialDefinition],
        selectors: &'a [AccountSelectorDefinition<'a>],
    ) -> Result<(), CompiledRoutingSnapshotError> {
        CompiledRoutingSnapshot::compile(
            version(1),
            candidates,
            RoutingStrategy::Priority,
            1,
            deployments,
            AccountCredentialDefinitions::new(accounts, credentials),
            selectors,
        )
        .map(|_| ())
    }

    fn ready(stage_value: u64, target_value: u64, penalty: u16) -> RouteCandidate {
        RouteCandidate::ready(stage(stage_value), target(target_value), penalty)
    }

    fn ready_with_deployment(
        stage_value: u64,
        target_value: u64,
        deployment_value: u64,
        penalty: u16,
    ) -> RouteCandidate {
        RouteCandidate::ready(
            stage(stage_value),
            target_with_deployment(target_value, deployment_value),
            penalty,
        )
    }

    fn cooling_down(stage_value: u64, target_value: u64) -> RouteCandidate {
        RouteCandidate::cooling_down(stage(stage_value), target(target_value))
    }

    fn disabled(stage_value: u64, target_value: u64) -> RouteCandidate {
        RouteCandidate::disabled(stage(stage_value), target(target_value))
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

    fn routed(snapshot_version: SnapshotVersion) -> VerifiedRouteDispatch {
        let disposition = IngressClassifier::new()
            .classify(IngressRequest::routed(
                OperationId::CONVERSATION,
                snapshot_version,
            ))
            .expect("路由请求分类成功");
        let VerifiedIngressDisposition::Routed(request) = disposition else {
            panic!("会话操作必须得到 Routed 分发");
        };
        request
    }

    fn eligibility<'snapshot, 'candidates>(
        snapshot: &'snapshot RoutingSnapshot<'candidates>,
    ) -> RouteEligibility<'snapshot, 'candidates> {
        HealthRegistry::new().eligibility_for(snapshot, HealthTick::new(0))
    }

    fn plan<'snapshot, 'candidates>(
        snapshot: &'snapshot RoutingSnapshot<'candidates>,
        request_cursor: u64,
    ) -> Result<RoutePlan<'snapshot, 'candidates>, PlanError> {
        RoutePlanner::plan(
            &routed(snapshot.version()),
            snapshot,
            &eligibility(snapshot),
            request_cursor,
        )
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
        let mut tracker = attempt::AttemptTracker::test_only(1);
        let receipt = tracker.test_only_rejection();
        tracker.into_outcome_for_test(failure, retry_after_ms, Some(receipt))
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

    fn not_sent_for<'snapshot, 'candidates>(
        permit: AttemptPermit<'snapshot, 'candidates>,
        failure: FailureClass,
    ) -> attempt::AttemptCompletion<'snapshot, 'candidates> {
        attempt::AttemptTracker::from_permit(permit).into_completion(failure, None, None)
    }

    fn sent_for<'snapshot, 'candidates>(
        permit: AttemptPermit<'snapshot, 'candidates>,
        failure: FailureClass,
    ) -> attempt::AttemptCompletion<'snapshot, 'candidates> {
        let mut tracker = attempt::AttemptTracker::from_permit(permit);
        tracker.request_write_started().expect("允许记录写入");
        tracker.into_completion(failure, None, None)
    }

    fn success_for<'snapshot, 'candidates>(
        permit: AttemptPermit<'snapshot, 'candidates>,
    ) -> attempt::AttemptSuccessCompletion {
        let mut tracker = attempt::AttemptTracker::from_permit(permit);
        tracker.request_write_started().expect("允许记录写入");
        tracker
            .upstream_response_observed()
            .expect("允许记录响应头");
        tracker.downstream_committed().expect("允许记录下游提交");
        tracker
            .into_response_completed()
            .expect("完整响应且已提交可转为成功 typestate")
    }

    fn consumes_success_completion(_: attempt::AttemptSuccessCompletion) {}

    fn plan_for_coordinator<'snapshot, 'candidates>(
        snapshot: &'snapshot RoutingSnapshot<'candidates>,
    ) -> (
        RoutePlan<'snapshot, 'candidates>,
        RouteEligibility<'snapshot, 'candidates>,
    ) {
        let eligibility = eligibility(snapshot);
        let plan = RoutePlanner::plan(&routed(snapshot.version()), snapshot, &eligibility, 0)
            .expect("存在可用目标");
        (plan, eligibility)
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
        assert_eq!(RouteStageId::new(0), None);
        assert_eq!(RouteStageId::new(3).map(RouteStageId::get), Some(3));
        assert_eq!(AccountSelectorId::new(0), None);
        assert_eq!(
            AccountSelectorId::new(7).map(AccountSelectorId::get),
            Some(7)
        );
    }

    #[test]
    fn snapshot_rejects_invalid_shape() {
        assert_eq!(
            RoutingSnapshot::new(version(1), &[], RoutingStrategy::Priority, 1).err(),
            Some(PlanError::NoCandidates)
        );

        let duplicate = [ready(1, 1, 0), ready(2, 1, 0)];
        assert_eq!(
            RoutingSnapshot::new(version(1), &duplicate, RoutingStrategy::Priority, 1).err(),
            Some(PlanError::DuplicateTarget)
        );

        let duplicate_deployment = [
            ready_with_deployment(1, 1, 99, 0),
            ready_with_deployment(2, 2, 99, 0),
        ];
        assert_eq!(
            RoutingSnapshot::new(
                version(1),
                &duplicate_deployment,
                RoutingStrategy::Priority,
                2,
            )
            .err(),
            Some(PlanError::DuplicateDeployment)
        );

        let first_policy = [ready_with_deployment(1, 1, 99, 0)];
        let second_policy = [ready_with_deployment(2, 2, 99, 0)];
        assert!(
            RoutingSnapshot::new(version(1), &first_policy, RoutingStrategy::Priority, 1,).is_ok()
        );
        assert!(
            RoutingSnapshot::new(version(2), &second_policy, RoutingStrategy::Priority, 1,).is_ok()
        );

        let candidates = [ready(1, 1, 0)];
        assert_eq!(
            RoutingSnapshot::new(version(1), &candidates, RoutingStrategy::Priority, 0).err(),
            Some(PlanError::InvalidMaxAttempts)
        );

        let non_contiguous = [ready(1, 1, 0), ready(2, 2, 0), ready(1, 3, 0)];
        assert_eq!(
            RoutingSnapshot::new(version(1), &non_contiguous, RoutingStrategy::Priority, 3,).err(),
            Some(PlanError::NonContiguousStage)
        );

        let two_stages = [ready(1, 1, 0), ready(2, 2, 0)];
        assert_eq!(
            RoutingSnapshot::new(version(1), &two_stages, RoutingStrategy::Priority, 1).err(),
            Some(PlanError::InsufficientMaxAttemptsForStages)
        );
    }

    #[test]
    fn selection_runtime_definition_input_rejects_each_resource_shape() {
        let quota_groups = [runtime_definition(1, 3)];
        let accounts = [account_runtime_definition(1, 4)];
        let credentials = [credential_runtime_definition(1, 5)];
        assert_eq!(
            SelectionRuntimeDefinitions::new(&[], &accounts, &credentials),
            Err(SelectionRuntimeDefinitionsError::EmptyQuotaGroupDefinitions)
        );
        assert_eq!(
            SelectionRuntimeDefinitions::new(&quota_groups, &[], &credentials),
            Err(SelectionRuntimeDefinitionsError::EmptyAccountDefinitions)
        );
        assert_eq!(
            SelectionRuntimeDefinitions::new(&quota_groups, &accounts, &[]),
            Err(SelectionRuntimeDefinitionsError::EmptyCredentialDefinitions)
        );

        let duplicate_groups = [runtime_definition(1, 1), runtime_definition(1, 2)];
        assert_eq!(
            SelectionRuntimeDefinitions::new(&duplicate_groups, &accounts, &credentials),
            Err(SelectionRuntimeDefinitionsError::DuplicateQuotaGroupDefinition)
        );
        let duplicate_accounts = [
            account_runtime_definition(1, 1),
            account_runtime_definition(1, 2),
        ];
        assert_eq!(
            SelectionRuntimeDefinitions::new(&quota_groups, &duplicate_accounts, &credentials),
            Err(SelectionRuntimeDefinitionsError::DuplicateAccountDefinition)
        );
        let duplicate_credentials = [
            credential_runtime_definition(1, 1),
            credential_runtime_definition(1, 2),
        ];
        assert_eq!(
            SelectionRuntimeDefinitions::new(&quota_groups, &accounts, &duplicate_credentials),
            Err(SelectionRuntimeDefinitionsError::DuplicateCredentialDefinition)
        );

        let too_many_groups: [QuotaGroupRuntimeDefinition; MAX_TRACKED_QUOTA_GROUPS + 1] =
            core::array::from_fn(|index| runtime_definition((index + 1) as u64, 1));
        assert_eq!(
            SelectionRuntimeDefinitions::new(&too_many_groups, &accounts, &credentials),
            Err(SelectionRuntimeDefinitionsError::TooManyQuotaGroupDefinitions)
        );
        let too_many_accounts: [AccountRuntimeDefinition; MAX_TRACKED_ACCOUNTS + 1] =
            core::array::from_fn(|index| account_runtime_definition((index + 1) as u64, 1));
        assert_eq!(
            SelectionRuntimeDefinitions::new(&quota_groups, &too_many_accounts, &credentials),
            Err(SelectionRuntimeDefinitionsError::TooManyAccountDefinitions)
        );
        let too_many_credentials: [CredentialRuntimeDefinition; MAX_TRACKED_CREDENTIALS + 1] =
            core::array::from_fn(|index| credential_runtime_definition((index + 1) as u64, 1));
        assert_eq!(
            SelectionRuntimeDefinitions::new(&quota_groups, &accounts, &too_many_credentials),
            Err(SelectionRuntimeDefinitionsError::TooManyCredentialDefinitions)
        );

        let definitions = SelectionRuntimeDefinitions::new(&quota_groups, &accounts, &credentials)
            .expect("三类单一定义有效");
        assert_eq!(definitions.quota_group_len(), 1);
        assert_eq!(definitions.account_len(), 1);
        assert_eq!(definitions.credential_len(), 1);

        let max_accounts: [AccountRuntimeDefinition; MAX_TRACKED_ACCOUNTS] =
            core::array::from_fn(|index| account_runtime_definition((index + 1) as u64, 1));
        let max_credentials: [CredentialRuntimeDefinition; MAX_TRACKED_CREDENTIALS] =
            core::array::from_fn(|index| credential_runtime_definition((index + 1) as u64, 1));
        let at_limit =
            SelectionRuntimeDefinitions::new(&quota_groups, &max_accounts, &max_credentials)
                .expect("16 个 Account 与 Credential 定义应有效");
        assert_eq!(at_limit.account_len(), MAX_TRACKED_ACCOUNTS);
        assert_eq!(at_limit.credential_len(), MAX_TRACKED_CREDENTIALS);
    }

    #[test]
    fn global_quota_runtime_layout_deduplicates_shared_reachable_group() {
        let shared_group = QuotaGroupId::new(1).expect("测试额度组 ID 非零");
        let first_only_group = QuotaGroupId::new(2).expect("测试额度组 ID 非零");
        let second_only_group = QuotaGroupId::new(3).expect("测试额度组 ID 非零");
        let first_groups = [shared_group, first_only_group];
        let second_groups = [shared_group, second_only_group];
        let first_unit = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &first_groups,
        )];
        let second_unit = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &second_groups,
        )];
        let first_members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let second_members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [
            selector_definition(1, &first_unit, &first_members),
            selector_definition(2, &second_unit, &second_members),
        ];
        let candidates = [
            RouteCandidate::ready(stage(1), target_with_binding(1, 1, 1, 1, 1), 0),
            RouteCandidate::ready(stage(2), target_with_binding(2, 1, 2, 1, 2), 0),
        ];
        let deployments = [
            deployment_definition(1, 1, 1),
            deployment_definition(2, 1, 1),
        ];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            2,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("共享额度组的编译快照有效");
        let runtime = [
            runtime_definition(1, 5),
            runtime_definition(2, 6),
            runtime_definition(3, 7),
        ];
        let account_runtime = [account_runtime_definition(1, 8)];
        let credential_runtime = [credential_runtime_definition(1, 9)];
        let definitions =
            SelectionRuntimeDefinitions::new(&runtime, &account_runtime, &credential_runtime)
                .expect("共享资源定义有效");
        let layout = compiled
            .selection_runtime_layout(&definitions)
            .expect("跨 Selector 共享的 Group 只占一个布局项");

        let route_eligibility = eligibility(compiled.routing());
        let route_plan = RoutePlanner::plan(
            &routed(compiled.routing().version()),
            compiled.routing(),
            &route_eligibility,
            0,
        )
        .expect("两个可达目标都可规划");
        let first_resolved = compiled
            .resolve_plan_target(&route_plan, 0)
            .expect("计划绑定当前快照")
            .expect("第一个解析目标存在");
        let binding = layout
            .binding_for(
                first_resolved,
                QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
                first_members[0],
            )
            .expect("目标自己的成员可绑定三类资源");
        assert_eq!(
            binding.account_id(),
            AccountId::new(1).expect("测试账户 ID 非零")
        );
        assert_eq!(binding.account_max_inflight().get(), 8);
        assert_eq!(
            binding.credential_id(),
            CredentialId::new(1).expect("测试凭据 ID 非零")
        );
        assert_eq!(binding.credential_max_inflight().get(), 9);
        let mut limits = binding.quota_group_limits();
        assert_eq!(
            limits.next(),
            Some((shared_group, core::num::NonZeroU16::new(5).unwrap()))
        );
        assert_eq!(
            limits.next(),
            Some((first_only_group, core::num::NonZeroU16::new(6).unwrap()))
        );
        assert_eq!(limits.next(), None);
        assert!(core::mem::size_of::<SelectionRuntimeDefinitions<'_>>() <= 48);
        assert!(core::mem::size_of::<SelectionRuntimeLayout<'_, '_>>() <= 64);
        assert!(core::mem::size_of::<SelectionRuntimeBinding<'_, '_>>() <= 128);
    }

    #[test]
    fn global_quota_runtime_layout_rejects_missing_and_unused_definitions() {
        let reachable_group = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let unreachable_group = [QuotaGroupId::new(2).expect("测试额度组 ID 非零")];
        let reachable_unit = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &reachable_group,
        )];
        let unreachable_unit = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &unreachable_group,
        )];
        let reachable_members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let unreachable_members = [AccountSelectorMember::new(
            AccountId::new(2).expect("测试账户 ID 非零"),
            CredentialId::new(2).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [
            selector_definition(1, &reachable_unit, &reachable_members),
            selector_definition(2, &unreachable_unit, &unreachable_members),
        ];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1), account_definition(2, 1)];
        let credentials = [credential_definition(1, 1), credential_definition(2, 2)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("包含不可达 Selector 的编译快照有效");

        let reachable_groups = [runtime_definition(1, 1)];
        let reachable_accounts = [account_runtime_definition(1, 1)];
        let reachable_credentials = [credential_runtime_definition(1, 1)];
        let missing_groups = [runtime_definition(2, 1)];
        let missing = SelectionRuntimeDefinitions::new(
            &missing_groups,
            &reachable_accounts,
            &reachable_credentials,
        )
        .expect("输入形状有效");
        assert!(matches!(
            compiled.selection_runtime_layout(&missing),
            Err(SelectionRuntimeLayoutError::MissingReachableQuotaGroupDefinition)
        ));

        let missing_accounts = [account_runtime_definition(2, 1)];
        let missing = SelectionRuntimeDefinitions::new(
            &reachable_groups,
            &missing_accounts,
            &reachable_credentials,
        )
        .expect("输入形状有效");
        assert!(matches!(
            compiled.selection_runtime_layout(&missing),
            Err(SelectionRuntimeLayoutError::MissingReachableAccountDefinition)
        ));

        let missing_credentials = [credential_runtime_definition(2, 1)];
        let missing = SelectionRuntimeDefinitions::new(
            &reachable_groups,
            &reachable_accounts,
            &missing_credentials,
        )
        .expect("输入形状有效");
        assert!(matches!(
            compiled.selection_runtime_layout(&missing),
            Err(SelectionRuntimeLayoutError::MissingReachableCredentialDefinition)
        ));

        let unused_groups = [runtime_definition(1, 1), runtime_definition(2, 1)];
        let unused = SelectionRuntimeDefinitions::new(
            &unused_groups,
            &reachable_accounts,
            &reachable_credentials,
        )
        .expect("输入形状有效");
        assert!(matches!(
            compiled.selection_runtime_layout(&unused),
            Err(SelectionRuntimeLayoutError::UnusedQuotaGroupDefinition)
        ));

        let unused_accounts = [
            account_runtime_definition(1, 1),
            account_runtime_definition(2, 1),
        ];
        let unused = SelectionRuntimeDefinitions::new(
            &reachable_groups,
            &unused_accounts,
            &reachable_credentials,
        )
        .expect("输入形状有效");
        assert!(matches!(
            compiled.selection_runtime_layout(&unused),
            Err(SelectionRuntimeLayoutError::UnusedAccountDefinition)
        ));

        let unused_credentials = [
            credential_runtime_definition(1, 1),
            credential_runtime_definition(2, 1),
        ];
        let unused = SelectionRuntimeDefinitions::new(
            &reachable_groups,
            &reachable_accounts,
            &unused_credentials,
        )
        .expect("输入形状有效");
        assert!(matches!(
            compiled.selection_runtime_layout(&unused),
            Err(SelectionRuntimeLayoutError::UnusedCredentialDefinition)
        ));
    }

    #[test]
    fn global_quota_runtime_layout_rejects_257_unique_reachable_groups() {
        let groups: [QuotaGroupId; MAX_TRACKED_QUOTA_GROUPS + 1] = core::array::from_fn(|index| {
            QuotaGroupId::new((index + 1) as u64).expect("测试额度组 ID 非零")
        });
        let units: [QuotaSelectionUnit<'_>; MAX_QUOTA_SELECTION_UNITS] =
            core::array::from_fn(|index| {
                let group_start = index * MAX_QUOTA_GROUPS_PER_UNIT;
                QuotaSelectionUnit::new(
                    QuotaSelectionUnitId::new((index + 1) as u64).expect("测试额度单元 ID 非零"),
                    core::num::NonZeroU16::new(1).expect("测试权重非零"),
                    &groups[group_start..group_start + MAX_QUOTA_GROUPS_PER_UNIT],
                )
            });
        let members: [AccountSelectorMember; MAX_ACCOUNT_SELECTOR_MEMBERS] =
            core::array::from_fn(|index| {
                AccountSelectorMember::new(
                    AccountId::new(1).expect("测试账户 ID 非零"),
                    CredentialId::new((index + 1) as u64).expect("测试凭据 ID 非零"),
                    QuotaSelectionUnitId::new((index + 1) as u64).expect("测试额度单元 ID 非零"),
                    0,
                )
            });
        let first_selector = AccountSelectorDefinition::new(
            AccountSelectorId::new(1).expect("账户选择合同 ID 非零"),
            SelectorRevision::new(1).expect("账户选择合同修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )
        .expect("256 个额度组的 Selector 合同有效");
        let last_group = [groups[MAX_TRACKED_QUOTA_GROUPS]];
        let last_unit = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &last_group,
        )];
        let last_members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let last_selector = selector_definition(2, &last_unit, &last_members);
        let selectors = [first_selector, last_selector];
        let candidates = [
            RouteCandidate::ready(stage(1), target_with_binding(1, 1, 1, 1, 1), 0),
            RouteCandidate::ready(stage(2), target_with_binding(2, 1, 2, 1, 2), 0),
        ];
        let deployments = [
            deployment_definition(1, 1, 1),
            deployment_definition(2, 1, 1),
        ];
        let accounts = [account_definition(1, 1)];
        let credentials: [CredentialDefinition; MAX_ACCOUNT_SELECTOR_MEMBERS] =
            core::array::from_fn(|index| credential_definition((index + 1) as u64, 1));
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            2,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("257 个可达额度组之前的快照编译有效");
        let runtime: [QuotaGroupRuntimeDefinition; MAX_TRACKED_QUOTA_GROUPS] =
            core::array::from_fn(|index| {
                QuotaGroupRuntimeDefinition::new(
                    groups[index],
                    core::num::NonZeroU16::new(1).expect("测试在途上限非零"),
                )
            });
        let runtime_accounts = [account_runtime_definition(1, 1)];
        let runtime_credentials: [CredentialRuntimeDefinition; MAX_TRACKED_CREDENTIALS] =
            core::array::from_fn(|index| credential_runtime_definition((index + 1) as u64, 1));
        let definitions =
            SelectionRuntimeDefinitions::new(&runtime, &runtime_accounts, &runtime_credentials)
                .expect("256 个额度组与可达身份的静态定义输入有效");

        let at_limit_candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let at_limit = CompiledRoutingSnapshot::compile(
            version(1),
            &at_limit_candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("恰好 256 个可达额度组的快照编译有效");
        assert!(at_limit.selection_runtime_layout(&definitions).is_ok());

        assert!(matches!(
            compiled.selection_runtime_layout(&definitions),
            Err(SelectionRuntimeLayoutError::TooManyReachableGroups)
        ));
    }

    #[test]
    fn global_quota_runtime_layout_rejects_same_version_foreign_resolved_target() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];
        let first = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("第一个编译快照有效");
        let second = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("第二个同版本快照有效");
        let runtime = [runtime_definition(1, 2)];
        let runtime_accounts = [account_runtime_definition(1, 3)];
        let runtime_credentials = [credential_runtime_definition(1, 4)];
        let definitions =
            SelectionRuntimeDefinitions::new(&runtime, &runtime_accounts, &runtime_credentials)
                .expect("静态定义输入有效");
        let first_layout = first
            .selection_runtime_layout(&definitions)
            .expect("第一个布局有效");
        let second_layout = second
            .selection_runtime_layout(&definitions)
            .expect("第二个布局有效");
        let first_eligibility = eligibility(first.routing());
        let first_plan = RoutePlanner::plan(
            &routed(first.routing().version()),
            first.routing(),
            &first_eligibility,
            0,
        )
        .expect("第一个快照可规划");
        let first_resolved = first
            .resolve_plan_target(&first_plan, 0)
            .expect("计划绑定第一个快照")
            .expect("首个解析目标存在");

        let first_binding = first_layout
            .binding_for(
                first_resolved,
                QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
                members[0],
            )
            .expect("同快照目标可绑定");
        assert_eq!(first_binding.account_max_inflight().get(), 3);
        assert!(matches!(
            second_layout.binding_for(
                first_resolved,
                QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
                members[0],
            ),
            Err(SelectionRuntimeLayoutError::StaleSnapshot)
        ));
    }

    #[test]
    fn selection_runtime_binding_rejects_unknown_unit_external_member_and_unit_mismatch() {
        let first_group = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let second_group = [QuotaGroupId::new(2).expect("测试额度组 ID 非零")];
        let first_unit_id = QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零");
        let second_unit_id = QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零");
        let units = [
            QuotaSelectionUnit::new(
                first_unit_id,
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &first_group,
            ),
            QuotaSelectionUnit::new(
                second_unit_id,
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &second_group,
            ),
        ];
        let members = [
            AccountSelectorMember::new(
                AccountId::new(1).expect("测试账户 ID 非零"),
                CredentialId::new(1).expect("测试凭据 ID 非零"),
                first_unit_id,
                0,
            ),
            AccountSelectorMember::new(
                AccountId::new(2).expect("测试账户 ID 非零"),
                CredentialId::new(2).expect("测试凭据 ID 非零"),
                second_unit_id,
                0,
            ),
        ];
        let selectors = [AccountSelectorDefinition::new(
            AccountSelectorId::new(1).expect("账户选择合同 ID 非零"),
            SelectorRevision::new(1).expect("账户选择合同修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )
        .expect("双 Unit 合同有效")];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1), account_definition(2, 1)];
        let credentials = [credential_definition(1, 1), credential_definition(2, 2)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("双 Unit 编译快照有效");
        let groups = [runtime_definition(1, 3), runtime_definition(2, 4)];
        let runtime_accounts = [
            account_runtime_definition(1, 5),
            account_runtime_definition(2, 6),
        ];
        let runtime_credentials = [
            credential_runtime_definition(1, 7),
            credential_runtime_definition(2, 8),
        ];
        let definitions =
            SelectionRuntimeDefinitions::new(&groups, &runtime_accounts, &runtime_credentials)
                .expect("双 Unit 运行时定义有效");
        let layout = compiled
            .selection_runtime_layout(&definitions)
            .expect("双 Unit 布局有效");
        let route_eligibility = eligibility(compiled.routing());
        let route_plan = RoutePlanner::plan(
            &routed(compiled.routing().version()),
            compiled.routing(),
            &route_eligibility,
            0,
        )
        .expect("目标可规划");
        let resolved = compiled
            .resolve_plan_target(&route_plan, 0)
            .expect("计划绑定当前快照")
            .expect("首个解析目标存在");

        assert!(matches!(
            layout.binding_for(
                resolved,
                QuotaSelectionUnitId::new(3).expect("测试额度单元 ID 非零"),
                members[0],
            ),
            Err(SelectionRuntimeLayoutError::UnknownTargetUnit)
        ));
        let external_member = AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            first_unit_id,
            1,
        );
        assert!(matches!(
            layout.binding_for(resolved, first_unit_id, external_member),
            Err(SelectionRuntimeLayoutError::UnknownTargetMember)
        ));
        assert!(matches!(
            layout.binding_for(resolved, second_unit_id, members[0]),
            Err(SelectionRuntimeLayoutError::MemberNotInTargetUnit)
        ));
    }

    #[test]
    fn selection_runtime_binding_is_bound_to_its_exact_target_provenance() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let unit_id = QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零");
        let units = [QuotaSelectionUnit::new(
            unit_id,
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            unit_id,
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates = [
            RouteCandidate::ready(stage(1), target_with_binding(1, 1, 1, 1, 1), 0),
            RouteCandidate::ready(stage(2), target_with_binding(2, 1, 2, 1, 1), 0),
        ];
        let deployments = [
            deployment_definition(1, 1, 1),
            deployment_definition(2, 1, 1),
        ];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            2,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("共享 Selector 的双 Target 快照有效");
        let runtime_groups = [runtime_definition(1, 3)];
        let runtime_accounts = [account_runtime_definition(1, 4)];
        let runtime_credentials = [credential_runtime_definition(1, 5)];
        let definitions = SelectionRuntimeDefinitions::new(
            &runtime_groups,
            &runtime_accounts,
            &runtime_credentials,
        )
        .expect("共享资源定义有效");
        let layout = compiled
            .selection_runtime_layout(&definitions)
            .expect("共享 Selector 布局有效");
        let route_eligibility = eligibility(compiled.routing());
        let route_plan = RoutePlanner::plan(
            &routed(compiled.routing().version()),
            compiled.routing(),
            &route_eligibility,
            0,
        )
        .expect("双 Target 可规划");
        let first = compiled
            .resolve_plan_target(&route_plan, 0)
            .expect("计划绑定当前快照")
            .expect("首个解析目标存在");
        let second = compiled
            .resolve_plan_target(&route_plan, 1)
            .expect("计划绑定当前快照")
            .expect("第二个解析目标存在");
        let binding = layout
            .binding_for(first, unit_id, members[0])
            .expect("第一个目标可以创建 binding");

        assert!(binding.matches_provenance(first, unit_id, members[0]));
        assert!(binding.matches_attempt_target(compiled.routing(), first.target().id()));
        assert!(!binding.matches_provenance(second, unit_id, members[0]));
        assert!(!binding.matches_attempt_target(compiled.routing(), second.target().id()));
    }

    #[test]
    fn compiled_snapshot_rejects_unknown_account_selector() {
        let groups = [QuotaGroupId::new(2).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(2).expect("测试账户 ID 非零"),
            CredentialId::new(2).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(2, &units, &members)];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];

        assert_eq!(
            CompiledRoutingSnapshot::compile(
                version(1),
                &candidates,
                RoutingStrategy::Priority,
                1,
                &deployments,
                AccountCredentialDefinitions::new(&accounts, &credentials),
                &selectors,
            )
            .err(),
            Some(CompiledRoutingSnapshotError::Plan(
                PlanError::UnknownAccountSelector
            ))
        );
    }

    #[test]
    fn compiled_snapshot_rejects_invalid_model_deployment_binding() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let unknown_deployment = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 2, 1),
            0,
        )];
        let site_mismatch = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let endpoint_mismatch = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let known_deployment = [deployment_definition(1, 1, 1)];
        let wrong_site = [deployment_definition(1, 2, 1)];
        let wrong_endpoint = [deployment_definition(1, 1, 2)];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];

        for (candidates, deployments, expected) in [
            (
                &unknown_deployment[..],
                &known_deployment[..],
                PlanError::UnknownModelDeployment,
            ),
            (
                &site_mismatch[..],
                &wrong_site[..],
                PlanError::TargetDeploymentSiteMismatch,
            ),
            (
                &endpoint_mismatch[..],
                &wrong_endpoint[..],
                PlanError::TargetDeploymentEndpointMismatch,
            ),
        ] {
            assert_eq!(
                CompiledRoutingSnapshot::compile(
                    version(1),
                    candidates,
                    RoutingStrategy::Priority,
                    1,
                    deployments,
                    AccountCredentialDefinitions::new(&accounts, &credentials),
                    &selectors,
                )
                .err(),
                Some(CompiledRoutingSnapshotError::Plan(expected))
            );
        }
    }

    #[test]
    fn compiled_snapshot_rejects_invalid_account_and_credential_catalogs() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1)];
        let unknown_owner = [credential_definition(1, 2)];

        assert_eq!(
            compile_result(&candidates, &deployments, &[], &[], &selectors),
            Err(CompiledRoutingSnapshotError::AccountCatalog(
                AccountCatalogError::Empty
            ))
        );
        assert_eq!(
            compile_result(&candidates, &deployments, &accounts, &[], &selectors),
            Err(CompiledRoutingSnapshotError::CredentialCatalog(
                CredentialCatalogError::Empty
            ))
        );
        assert_eq!(
            compile_result(
                &candidates,
                &deployments,
                &accounts,
                &unknown_owner,
                &selectors,
            ),
            Err(CompiledRoutingSnapshotError::CredentialCatalog(
                CredentialCatalogError::UnknownOwnerAccount
            ))
        );
    }

    #[test]
    fn compiled_snapshot_rejects_invalid_selector_member_bindings() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];

        let unknown_account_members = [AccountSelectorMember::new(
            AccountId::new(2).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let unknown_account_selectors = [selector_definition(1, &units, &unknown_account_members)];
        let account_one = [account_definition(1, 1)];
        let credential_one = [credential_definition(1, 1)];
        assert_eq!(
            compile_result(
                &candidates,
                &deployments,
                &account_one,
                &credential_one,
                &unknown_account_selectors,
            ),
            Err(CompiledRoutingSnapshotError::Plan(
                PlanError::UnknownAccount
            ))
        );

        let unknown_credential_members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(2).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let unknown_credential_selectors =
            [selector_definition(1, &units, &unknown_credential_members)];
        assert_eq!(
            compile_result(
                &candidates,
                &deployments,
                &account_one,
                &credential_one,
                &unknown_credential_selectors,
            ),
            Err(CompiledRoutingSnapshotError::Plan(
                PlanError::UnknownCredential
            ))
        );

        let mismatched_members = [AccountSelectorMember::new(
            AccountId::new(2).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let mismatched_selectors = [selector_definition(1, &units, &mismatched_members)];
        let two_accounts = [account_definition(1, 1), account_definition(2, 1)];
        assert_eq!(
            compile_result(
                &candidates,
                &deployments,
                &two_accounts,
                &credential_one,
                &mismatched_selectors,
            ),
            Err(CompiledRoutingSnapshotError::Plan(
                PlanError::CredentialAccountMismatch
            ))
        );

        let cross_site_members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let cross_site_selectors = [selector_definition(1, &units, &cross_site_members)];
        let other_site_account = [account_definition(1, 2)];
        assert_eq!(
            compile_result(
                &candidates,
                &deployments,
                &other_site_account,
                &credential_one,
                &cross_site_selectors,
            ),
            Err(CompiledRoutingSnapshotError::Plan(
                PlanError::AccountDeploymentSiteMismatch
            ))
        );
    }

    #[test]
    fn compiled_snapshot_allows_same_site_accounts_and_shared_quota_unit() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(7).expect("测试权重非零"),
            &groups,
        )];
        let members = [
            AccountSelectorMember::new(
                AccountId::new(1).expect("测试账户 ID 非零"),
                CredentialId::new(1).expect("测试凭据 ID 非零"),
                QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
                0,
            ),
            AccountSelectorMember::new(
                AccountId::new(1).expect("测试账户 ID 非零"),
                CredentialId::new(2).expect("测试凭据 ID 非零"),
                QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
                0,
            ),
            AccountSelectorMember::new(
                AccountId::new(2).expect("测试账户 ID 非零"),
                CredentialId::new(3).expect("测试凭据 ID 非零"),
                QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
                0,
            ),
        ];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1), account_definition(2, 1)];
        let credentials = [
            credential_definition(1, 1),
            credential_definition(2, 1),
            credential_definition(3, 2),
        ];

        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("同站账户与多凭据静态关系有效");

        assert_eq!(compiled.routing().version(), version(1));
        assert_eq!(selectors[0].members().len(), 3);
        assert_eq!(
            selectors[0]
                .unit(QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"))
                .expect("额度单元存在")
                .effective_weight()
                .get(),
            7
        );
    }

    #[test]
    fn compiled_snapshot_keeps_shared_selector_and_stage_first_plan() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates = [
            RouteCandidate::ready(stage(1), target_with_binding(1, 1, 11, 1, 1), 0),
            RouteCandidate::ready(stage(1), target_with_binding(2, 1, 12, 2, 1), 9),
            RouteCandidate::ready(stage(2), target_with_binding(3, 1, 13, 3, 1), 0),
        ];
        let deployments = [
            deployment_definition(11, 1, 1),
            deployment_definition(12, 1, 2),
            deployment_definition(13, 1, 3),
        ];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            3,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("共享账户选择合同的快照有效");

        let plan = plan(compiled.routing(), 0).expect("存在可用目标");
        assert_eq!(plan.len(), 2);
        assert_eq!(plan.target_id(0), Some(id(1)));
        assert_eq!(plan.target_id(1), Some(id(3)));
        let resolved = compiled
            .resolve_plan_target(&plan, 0)
            .expect("计划与快照一致")
            .expect("首个尝试存在");
        assert_eq!(resolved.snapshot_version(), version(1));
        assert_eq!(resolved.stage(), stage(1));
        assert_eq!(resolved.target(), target_with_selector(1, 11, 1));
        assert_eq!(resolved.deployment(), &deployments[0]);
        assert_eq!(resolved.selector(), &selectors[0]);
        let debug = format!("{resolved:?}");
        assert!(debug.starts_with("ResolvedRouteTarget { snapshot_version:"));
        assert!(debug.contains("target: RouteTarget"));
        assert!(!debug.contains("AccountSelectorDefinition"));
        assert!(!debug.contains("ModelDeploymentDefinition"));
        let resolved = compiled
            .resolve_plan_target(&plan, 1)
            .expect("计划与快照一致")
            .expect("第二次尝试存在");
        assert_eq!(resolved.snapshot_version(), version(1));
        assert_eq!(resolved.stage(), stage(2));
        assert_eq!(resolved.target(), target_with_binding(3, 1, 13, 3, 1));
        assert_eq!(resolved.deployment(), &deployments[2]);
        assert_eq!(resolved.selector(), &selectors[0]);
        assert_eq!(
            compiled
                .resolve_plan_target(&plan, 2)
                .expect("越界尝试不是错误"),
            None
        );
    }

    #[test]
    fn compiled_snapshots_do_not_mix_catalogs() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors_a = [selector_definition(1, &units, &members)];
        let selectors_b = [selector_definition(2, &units, &members)];
        let candidates_a = [RouteCandidate::ready(
            stage(1),
            target_with_selector(1, 1, 1),
            0,
        )];
        let candidates_b = [RouteCandidate::ready(
            stage(1),
            target_with_selector(2, 2, 2),
            0,
        )];
        let deployments_a = [deployment_definition(1, 1, 1)];
        let deployments_b = [deployment_definition(2, 2, 2)];
        let accounts_a = [account_definition(1, 1)];
        let credentials_a = [credential_definition(1, 1)];
        let accounts_b = [account_definition(1, 2)];
        let credentials_b = [credential_definition(1, 1)];
        let compiled_a = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates_a,
            RoutingStrategy::Priority,
            1,
            &deployments_a,
            AccountCredentialDefinitions::new(&accounts_a, &credentials_a),
            &selectors_a,
        )
        .expect("第一个编译快照有效");
        let compiled_b = CompiledRoutingSnapshot::compile(
            version(2),
            &candidates_b,
            RoutingStrategy::Priority,
            1,
            &deployments_b,
            AccountCredentialDefinitions::new(&accounts_b, &credentials_b),
            &selectors_b,
        )
        .expect("第二个编译快照有效");

        let plan_a = plan(compiled_a.routing(), 0).expect("第一个计划有效");
        let plan_b = plan(compiled_b.routing(), 0).expect("第二个计划有效");

        assert_eq!(
            compiled_a
                .resolve_plan_target(&plan_a, 0)
                .expect("第一个计划与快照一致")
                .map(ResolvedRouteTarget::deployment),
            Some(&deployments_a[0])
        );
        assert_eq!(
            compiled_a
                .resolve_plan_target(&plan_a, 0)
                .expect("第一个计划与快照一致")
                .map(ResolvedRouteTarget::selector),
            Some(&selectors_a[0])
        );
        assert_eq!(
            compiled_b
                .resolve_plan_target(&plan_b, 0)
                .expect("第二个计划与快照一致")
                .map(ResolvedRouteTarget::deployment),
            Some(&deployments_b[0])
        );
        assert_eq!(
            compiled_b
                .resolve_plan_target(&plan_b, 0)
                .expect("第二个计划与快照一致")
                .map(ResolvedRouteTarget::selector),
            Some(&selectors_b[0])
        );
        assert_eq!(
            compiled_a.resolve_plan_target(&plan_b, 0),
            Err(PlanError::StaleSnapshot)
        );
        assert_eq!(
            compiled_b.resolve_plan_target(&plan_a, 0),
            Err(PlanError::StaleSnapshot)
        );
    }

    #[test]
    fn resolved_route_target_stays_small_and_stack_only() {
        assert!(core::mem::size_of::<ResolvedRouteTarget<'_, '_>>() <= 128);
    }

    #[test]
    fn account_selection_candidates_keep_target_topology_and_shared_unit_members() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let unit_id = QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零");
        let units = [QuotaSelectionUnit::new(
            unit_id,
            core::num::NonZeroU16::new(7).expect("测试权重非零"),
            &groups,
        )];
        let members = [
            AccountSelectorMember::new(
                AccountId::new(1).expect("测试账户 ID 非零"),
                CredentialId::new(1).expect("测试凭据 ID 非零"),
                unit_id,
                0,
            ),
            AccountSelectorMember::new(
                AccountId::new(1).expect("测试账户 ID 非零"),
                CredentialId::new(2).expect("测试凭据 ID 非零"),
                unit_id,
                0,
            ),
            AccountSelectorMember::new(
                AccountId::new(2).expect("测试账户 ID 非零"),
                CredentialId::new(3).expect("测试凭据 ID 非零"),
                unit_id,
                0,
            ),
        ];
        let selectors = [AccountSelectorDefinition::new(
            AccountSelectorId::new(1).expect("账户选择合同 ID 非零"),
            SelectorRevision::new(1).expect("账户选择合同修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::WeightedLeastInflight,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )
        .expect("保守单元的静态选择合同有效")];
        let route_candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1), account_definition(2, 1)];
        let credentials = [
            credential_definition(1, 1),
            credential_definition(2, 1),
            credential_definition(3, 2),
        ];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &route_candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("同代静态目录有效");
        let route_plan = plan(compiled.routing(), 0).expect("存在计划目标");
        let resolved = compiled
            .resolve_plan_target(&route_plan, 0)
            .expect("计划与快照一致")
            .expect("首个尝试存在");
        let candidates = compiled
            .account_selection_candidates(resolved)
            .expect("同代目标可创建候选视图");

        assert_eq!(candidates.target(), target_with_binding(1, 1, 1, 1, 1));
        assert_eq!(
            candidates.policy(),
            CredentialSelectionPolicy::WeightedLeastInflight
        );
        assert_eq!(
            candidates.topology_source(),
            QuotaTopologySource::ConservativeDefault
        );
        let unit = candidates.unit_at(0).expect("保守拓扑的唯一单元存在");
        assert_eq!(unit.id(), unit_id);
        assert_eq!(unit.effective_weight().get(), 7);
        assert_eq!(candidates.unit_at(1), None);

        let mut unit_members = candidates.members_in_unit(unit_id);
        assert_eq!(unit_members.next(), Some(members[0]));
        assert_eq!(unit_members.next(), Some(members[1]));
        assert_eq!(unit_members.next(), Some(members[2]));
        assert_eq!(unit_members.next(), None);
        assert_eq!(
            candidates
                .members_in_unit(QuotaSelectionUnitId::new(99).expect("测试额度单元 ID 非零"))
                .count(),
            0
        );
    }

    #[test]
    fn selection_input_binds_session_and_validates_dynamic_masks() {
        let groups_one = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let groups_two = [QuotaGroupId::new(2).expect("测试额度组 ID 非零")];
        let unit_one = QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零");
        let unit_two = QuotaSelectionUnitId::new(2).expect("测试额度单元 ID 非零");
        let units = [
            QuotaSelectionUnit::new(
                unit_one,
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &groups_one,
            ),
            QuotaSelectionUnit::new(
                unit_two,
                core::num::NonZeroU16::new(1).expect("测试权重非零"),
                &groups_two,
            ),
        ];
        let members = [
            AccountSelectorMember::new(
                AccountId::new(1).expect("测试账户 ID 非零"),
                CredentialId::new(1).expect("测试凭据 ID 非零"),
                unit_one,
                0,
            ),
            AccountSelectorMember::new(
                AccountId::new(2).expect("测试账户 ID 非零"),
                CredentialId::new(2).expect("测试凭据 ID 非零"),
                unit_two,
                0,
            ),
        ];
        let selectors = [AccountSelectorDefinition::new(
            AccountSelectorId::new(1).expect("账户选择合同 ID 非零"),
            SelectorRevision::new(9).expect("账户选择合同修订非零"),
            SelectorAffinitySalt::new([9; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )
        .expect("双独立额度单元的静态选择合同有效")];
        let route_candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts = [account_definition(1, 1), account_definition(2, 1)];
        let credentials = [credential_definition(1, 1), credential_definition(2, 2)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &route_candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("同代账户目录有效");
        let route_plan = plan(compiled.routing(), 0).expect("存在计划目标");
        let resolved = compiled
            .resolve_plan_target(&route_plan, 0)
            .expect("计划与快照一致")
            .expect("首个尝试存在");

        let absent = compiled
            .selection_request(resolved, SelectionSession::Absent)
            .expect("同代目标可创建无会话选择请求");
        assert_eq!(absent.target(), target_with_binding(1, 1, 1, 1, 1));
        assert!(!absent.has_stable_session());
        assert_eq!(absent.selector_revision().get(), 9);
        assert_eq!(absent.selector_affinity_salt().get(), [9; 16]);

        let alias = SessionAffinityAlias::from_host_hmac([7; 16]);
        let alias_copy = alias;
        let alias_second_copy = alias;
        assert!(alias_copy == alias_second_copy);

        let stable = compiled
            .selection_request(resolved, SelectionSession::Stable(alias))
            .expect("同代目标可创建稳定会话选择请求");
        assert!(stable.has_stable_session());
        assert!(matches!(stable.session(), SelectionSession::Stable(_)));

        let eligibility = AccountSelectionEligibility::new(absent, 0b11, 0b11)
            .expect("Unit 与 Member 位图同时匹配时有效");
        assert!(eligibility.request() == absent);
        assert_eq!(eligibility.unit_mask(), 0b11);
        assert_eq!(eligibility.member_mask(), 0b11);
        assert_eq!(eligibility.unit_allowed_at(0), Some(true));
        assert_eq!(eligibility.unit_allowed_at(1), Some(true));
        assert_eq!(eligibility.unit_allowed_at(2), None);
        assert_eq!(eligibility.member_allowed_at(0), Some(true));
        assert_eq!(eligibility.member_allowed_at(1), Some(true));
        assert_eq!(eligibility.member_allowed_at(2), None);
        assert!(AccountSelectionEligibility::new(absent, 0, 0).is_ok());

        assert!(matches!(
            AccountSelectionEligibility::new(absent, 0b100, 0b1),
            Err(AccountSelectionEligibilityError::UnitMaskOutOfBounds)
        ));
        assert!(matches!(
            AccountSelectionEligibility::new(absent, 0b1, 0b100),
            Err(AccountSelectionEligibilityError::MemberMaskOutOfBounds)
        ));
        assert!(matches!(
            AccountSelectionEligibility::new(absent, 0b1, 0b10),
            Err(AccountSelectionEligibilityError::MemberUnitNotAllowed)
        ));
        assert!(matches!(
            AccountSelectionEligibility::new(absent, 0b10, 0),
            Err(AccountSelectionEligibilityError::AllowedUnitWithoutMember)
        ));
    }

    #[test]
    fn selection_input_accepts_full_capacity_masks() {
        let groups: [QuotaGroupId; MAX_QUOTA_SELECTION_UNITS] = core::array::from_fn(|index| {
            QuotaGroupId::new((index + 1) as u64).expect("测试额度组 ID 非零")
        });
        let units: [QuotaSelectionUnit<'_>; MAX_QUOTA_SELECTION_UNITS] =
            core::array::from_fn(|index| {
                QuotaSelectionUnit::new(
                    QuotaSelectionUnitId::new((index + 1) as u64).expect("测试额度单元 ID 非零"),
                    core::num::NonZeroU16::new(1).expect("测试权重非零"),
                    &groups[index..=index],
                )
            });
        let members: [AccountSelectorMember; MAX_ACCOUNT_SELECTOR_MEMBERS] =
            core::array::from_fn(|index| {
                let id = (index + 1) as u64;
                AccountSelectorMember::new(
                    AccountId::new(id).expect("测试账户 ID 非零"),
                    CredentialId::new(id).expect("测试凭据 ID 非零"),
                    units[index].id(),
                    0,
                )
            });
        let selectors = [AccountSelectorDefinition::new(
            AccountSelectorId::new(1).expect("账户选择合同 ID 非零"),
            SelectorRevision::new(1).expect("账户选择合同修订非零"),
            SelectorAffinitySalt::new([1; 16]),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )
        .expect("满容量独立额度单元的静态选择合同有效")];
        let route_candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let deployments = [deployment_definition(1, 1, 1)];
        let accounts: [AccountDefinition; MAX_ACCOUNT_SELECTOR_MEMBERS] =
            core::array::from_fn(|index| account_definition((index + 1) as u64, 1));
        let credentials: [CredentialDefinition; MAX_ACCOUNT_SELECTOR_MEMBERS] =
            core::array::from_fn(|index| {
                credential_definition((index + 1) as u64, (index + 1) as u64)
            });
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &route_candidates,
            RoutingStrategy::Priority,
            1,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("满容量同代账户目录有效");
        let route_plan = plan(compiled.routing(), 0).expect("存在计划目标");
        let resolved = compiled
            .resolve_plan_target(&route_plan, 0)
            .expect("计划与快照一致")
            .expect("首个尝试存在");
        let request = compiled
            .selection_request(resolved, SelectionSession::Absent)
            .expect("同代目标可创建选择请求");

        let eligibility = AccountSelectionEligibility::new(request, u16::MAX, u16::MAX)
            .expect("16 个 Unit 与 Member 的满位资格掩码有效");

        assert_eq!(eligibility.unit_allowed_at(15), Some(true));
        assert_eq!(eligibility.member_allowed_at(15), Some(true));
    }

    #[test]
    fn account_selection_candidates_reject_same_version_foreign_snapshot() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let unit_id = QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零");
        let units = [QuotaSelectionUnit::new(
            unit_id,
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            unit_id,
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates_a = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let candidates_b = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 2, 2, 2, 1),
            0,
        )];
        let deployments_a = [deployment_definition(1, 1, 1)];
        let deployments_b = [deployment_definition(2, 2, 2)];
        let accounts_a = [account_definition(1, 1)];
        let accounts_b = [account_definition(1, 2)];
        let credentials = [credential_definition(1, 1)];
        let compiled_a = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates_a,
            RoutingStrategy::Priority,
            1,
            &deployments_a,
            AccountCredentialDefinitions::new(&accounts_a, &credentials),
            &selectors,
        )
        .expect("第一个编译快照有效");
        let compiled_b = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates_b,
            RoutingStrategy::Priority,
            1,
            &deployments_b,
            AccountCredentialDefinitions::new(&accounts_b, &credentials),
            &selectors,
        )
        .expect("第二个编译快照有效");
        let route_plan = plan(compiled_a.routing(), 0).expect("存在第一个计划目标");
        let resolved = compiled_a
            .resolve_plan_target(&route_plan, 0)
            .expect("计划与首个快照一致")
            .expect("首个尝试存在");

        assert!(matches!(
            compiled_b.selection_request(resolved, SelectionSession::Absent),
            Err(PlanError::StaleSnapshot)
        ));

        assert!(matches!(
            compiled_b.account_selection_candidates(resolved),
            Err(PlanError::StaleSnapshot)
        ));
    }

    #[test]
    fn account_selection_candidates_stay_small_and_stack_only() {
        assert!(core::mem::size_of::<AccountSelectionCandidates<'_, '_>>() <= 128);
        assert!(core::mem::size_of::<account_selector::AccountSelectionMembers<'_>>() <= 32);
    }

    #[test]
    fn priority_plan_filters_and_orders_targets() {
        let candidates = [
            cooling_down(1, 2),
            ready(1, 3, 9),
            ready(2, 1, 7),
            disabled(3, 4),
        ];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 3);
        let plan = plan(&snapshot, 0).expect("存在可用目标");

        assert_eq!(plan.len(), 2);
        assert_eq!(plan.target_id(0), Some(id(3)));
        assert_eq!(plan.target_id(1), Some(id(1)));
        assert_eq!(plan.target_id(2), None);
    }

    #[test]
    fn round_robin_only_rotates_inside_each_stage() {
        let candidates = [ready(1, 1, 0), ready(1, 2, 0), ready(2, 3, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::RoundRobin, 3);
        let plan = plan(&snapshot, 1).expect("存在可用目标");

        assert_eq!(plan.target_id(0), Some(id(2)));
        assert_eq!(plan.target_id(1), Some(id(3)));
        assert_eq!(plan.target_id(2), None);
    }

    #[test]
    fn least_penalty_is_scoped_to_stage() {
        let candidates = [ready(1, 1, 8), ready(1, 2, 2), ready(2, 3, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::LeastPenalty, 3);
        let plan = plan(&snapshot, 99).expect("存在可用目标");

        assert_eq!(plan.target_id(0), Some(id(2)));
        assert_eq!(plan.target_id(1), Some(id(3)));
        assert_eq!(plan.target_id(2), None);
    }

    #[test]
    fn next_stage_is_used_only_when_current_stage_has_no_eligible_target() {
        let candidates = [cooling_down(1, 1), disabled(1, 2), ready(2, 3, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 3);
        let plan = plan(&snapshot, 0).expect("后一阶段存在可用目标");

        assert_eq!(plan.len(), 1);
        assert_eq!(plan.target_id(0), Some(id(3)));
        assert_eq!(plan.target_id(1), None);
    }

    #[test]
    fn planner_rejects_request_bound_to_another_snapshot() {
        let candidates = [ready(1, 1, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 1);
        let eligibility = eligibility(&snapshot);

        assert!(matches!(
            RoutePlanner::plan(&routed(version(2)), &snapshot, &eligibility, 0),
            Err(PlanError::RequestSnapshotMismatch)
        ));
    }

    #[test]
    fn coordinator_rejects_current_eligibility_from_other_candidate_identity() {
        let first_candidates = [ready(1, 1, 0)];
        let first_snapshot = snapshot(&first_candidates, RoutingStrategy::Priority, 1);
        let first_eligibility = eligibility(&first_snapshot);
        let plan = RoutePlanner::plan(
            &routed(first_snapshot.version()),
            &first_snapshot,
            &first_eligibility,
            0,
        )
        .expect("首个快照可规划");
        let second_candidates = [ready(1, 2, 0)];
        let second_snapshot = snapshot(&second_candidates, RoutingStrategy::Priority, 1);
        let mismatched_eligibility = eligibility(&second_snapshot);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 1_000).expect("策略有效"))
            .expect("计划与预算一致");

        assert!(matches!(
            coordinator.start(&mismatched_eligibility),
            Err(AttemptStartError::EligibilityMismatch)
        ));
        assert!(coordinator.is_stopped());
    }

    #[test]
    fn coordinator_rejects_same_version_same_target_from_other_snapshot_instance() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let first_candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 1, 1, 1, 1),
            0,
        )];
        let second_candidates = [RouteCandidate::ready(
            stage(1),
            target_with_binding(1, 2, 2, 2, 1),
            0,
        )];
        let first_deployments = [deployment_definition(1, 1, 1)];
        let second_deployments = [deployment_definition(2, 2, 2)];
        let first_accounts = [account_definition(1, 1)];
        let second_accounts = [account_definition(1, 2)];
        let credentials = [credential_definition(1, 1)];
        let first = CompiledRoutingSnapshot::compile(
            version(1),
            &first_candidates,
            RoutingStrategy::Priority,
            1,
            &first_deployments,
            AccountCredentialDefinitions::new(&first_accounts, &credentials),
            &selectors,
        )
        .expect("第一个编译快照有效");
        let second = CompiledRoutingSnapshot::compile(
            version(1),
            &second_candidates,
            RoutingStrategy::Priority,
            1,
            &second_deployments,
            AccountCredentialDefinitions::new(&second_accounts, &credentials),
            &selectors,
        )
        .expect("第二个编译快照有效");
        let request = routed(first.routing().version());
        let mut registry = HealthRegistry::new();
        let first_eligibility = registry.eligibility_for(first.routing(), HealthTick::new(1));
        let first_plan = RoutePlanner::plan(&request, first.routing(), &first_eligibility, 0)
            .expect("第一个计划有效");
        let cooldown_plan = RoutePlanner::plan(&request, first.routing(), &first_eligibility, 1)
            .expect("限流尝试计划有效");
        let first_resolved = first
            .resolve_plan_target(&first_plan, 0)
            .expect("第一个计划可解析")
            .expect("第一个目标存在");
        let second_eligibility = registry.eligibility_for(second.routing(), HealthTick::new(1));
        let second_plan = RoutePlanner::plan(&request, second.routing(), &second_eligibility, 0)
            .expect("第二个计划有效");
        let second_resolved = second
            .resolve_plan_target(&second_plan, 0)
            .expect("第二个计划可解析")
            .expect("第二个目标存在");
        let mut cooldown_coordinator = cooldown_plan
            .into_attempt_coordinator(RetryPolicy::new(1, 1_000).expect("策略有效"))
            .expect("计划与预算一致");
        let cooldown_permit = cooldown_coordinator
            .start(&first_eligibility)
            .expect("限流尝试可签发");
        let mut cooldown_tracker = attempt::AttemptTracker::from_permit(cooldown_permit);
        assert!(matches!(
            cooldown_tracker.rate_limit_reporter(second_resolved),
            Err(attempt::RateLimitReporterError::TargetMismatch)
        ));
        assert!(matches!(
            cooldown_tracker.replay_reporter(second_resolved),
            Err(attempt::ReplayReporterError::TargetMismatch)
        ));
        let replay_reporter = cooldown_tracker
            .replay_reporter(first_resolved)
            .expect("同一快照与 Target 可签发回放上报器");
        assert!(matches!(
            cooldown_tracker.replay_reporter(first_resolved),
            Err(attempt::ReplayReporterError::AlreadyReported)
        ));
        let _receipt = replay_reporter
            .pre_execution_rejected(
                attempt::VerifiedPreExecutionContract::test_only_registered(
                    first_resolved.target().site(),
                    0x1001,
                )
                .expect("已登记测试合同"),
            )
            .expect("同站合同可签发收据");
        let observation = cooldown_tracker
            .rate_limit_reporter(first_resolved)
            .expect("同 Target 可签发上报器")
            .report(RateLimitScope::Deployment, HealthTick::new(10));
        registry.record_rate_limit(observation, HealthTick::new(1));
        assert!(!registry
            .eligibility_for(first.routing(), HealthTick::new(1))
            .allows_index(0));
        let second_eligibility = registry.eligibility_for(second.routing(), HealthTick::new(1));
        assert!(second_eligibility.allows_index(0));

        let mut coordinator = first_plan
            .into_attempt_coordinator(RetryPolicy::new(1, 1_000).expect("策略有效"))
            .expect("计划与预算一致");
        assert!(matches!(
            coordinator.start(&second_eligibility),
            Err(AttemptStartError::EligibilityMismatch)
        ));
        assert!(coordinator.is_stopped());
    }

    #[test]
    fn plan_is_borrowed_from_its_snapshot() {
        let candidates = [ready(1, 1, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 1);
        let plan = plan(&snapshot, 0).expect("存在可用目标");

        assert_eq!(plan.snapshot_version(), version(1));
        assert_eq!(
            plan.resolve(0)
                .expect("计划绑定快照中存在目标")
                .map(|resolved| resolved.id()),
            Some(id(1))
        );
    }

    #[test]
    fn plan_exposes_stage_identity_for_each_attempt() {
        let candidates = [ready(11, 1, 0), ready(22, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = plan(&snapshot, 0).expect("存在可用目标");

        assert_eq!(plan.stage_id(0), Ok(Some(stage(11))));
        assert_eq!(plan.stage_id(1), Ok(Some(stage(22))));
        assert_eq!(plan.stage_id(2), Ok(None));
    }

    #[test]
    fn resolved_cooldown_filters_plans_and_never_turns_written_429_into_replay() {
        let groups = [QuotaGroupId::new(1).expect("测试额度组 ID 非零")];
        let units = [QuotaSelectionUnit::new(
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            core::num::NonZeroU16::new(1).expect("测试权重非零"),
            &groups,
        )];
        let members = [AccountSelectorMember::new(
            AccountId::new(1).expect("测试账户 ID 非零"),
            CredentialId::new(1).expect("测试凭据 ID 非零"),
            QuotaSelectionUnitId::new(1).expect("测试额度单元 ID 非零"),
            0,
        )];
        let selectors = [selector_definition(1, &units, &members)];
        let candidates = [
            RouteCandidate::ready(stage(1), target_with_binding(1, 1, 1, 1, 1), 0),
            RouteCandidate::ready(stage(2), target_with_binding(2, 1, 2, 2, 1), 0),
            RouteCandidate::ready(stage(3), target_with_binding(3, 1, 3, 3, 1), 0),
        ];
        let deployments = [
            deployment_definition(1, 1, 1),
            deployment_definition(2, 1, 2),
            deployment_definition(3, 1, 3),
        ];
        let accounts = [account_definition(1, 1)];
        let credentials = [credential_definition(1, 1)];
        let compiled = CompiledRoutingSnapshot::compile(
            version(1),
            &candidates,
            RoutingStrategy::Priority,
            3,
            &deployments,
            AccountCredentialDefinitions::new(&accounts, &credentials),
            &selectors,
        )
        .expect("测试编译快照有效");
        let request = routed(compiled.routing().version());

        let mut written_registry = HealthRegistry::new();
        let written_initial =
            written_registry.eligibility_for(compiled.routing(), HealthTick::new(1));
        let written_plan = RoutePlanner::plan(&request, compiled.routing(), &written_initial, 0)
            .expect("初始计划有效");
        let written_resolved = compiled
            .resolve_plan_target(&written_plan, 0)
            .expect("计划同代")
            .expect("首个目标存在");
        let mismatched_resolved = compiled
            .resolve_plan_target(&written_plan, 1)
            .expect("计划同代")
            .expect("第二个目标存在");
        let repeated_resolved = compiled
            .resolve_plan_target(&written_plan, 0)
            .expect("计划同代")
            .expect("首个目标存在");
        let mut written_coordinator = written_plan
            .into_attempt_coordinator(RetryPolicy::new(3, 1_000).expect("策略有效"))
            .expect("计划与预算一致");
        let written_first = written_coordinator
            .start(&written_initial)
            .expect("初始目标可签发");
        let mut written_tracker = attempt::AttemptTracker::from_permit(written_first);
        assert!(matches!(
            written_tracker.rate_limit_reporter(mismatched_resolved),
            Err(attempt::RateLimitReporterError::TargetMismatch)
        ));
        let observation = written_tracker
            .rate_limit_reporter(written_resolved)
            .expect("当前目标可签发上报器")
            .report(RateLimitScope::Deployment, HealthTick::new(10));
        assert!(matches!(
            written_tracker.rate_limit_reporter(repeated_resolved),
            Err(attempt::RateLimitReporterError::AlreadyReported)
        ));
        assert!(matches!(
            written_tracker.replay_reporter(mismatched_resolved),
            Err(attempt::ReplayReporterError::TargetMismatch)
        ));
        let replay_reporter = written_tracker
            .replay_reporter(written_resolved)
            .expect("当前目标可签发回放上报器");
        assert!(matches!(
            written_tracker.replay_reporter(repeated_resolved),
            Err(attempt::ReplayReporterError::AlreadyReported)
        ));
        let receipt = replay_reporter
            .pre_execution_rejected(
                attempt::VerifiedPreExecutionContract::test_only_registered(
                    written_resolved.target().site(),
                    0x1001,
                )
                .expect("已登记测试合同"),
            )
            .expect("同站合同可签发收据");
        written_registry.record_rate_limit(observation, HealthTick::new(1));
        let written_current =
            written_registry.eligibility_for(compiled.routing(), HealthTick::new(1));
        written_tracker
            .request_write_started()
            .expect("允许记录请求写入");
        assert!(matches!(
            written_coordinator
                .complete(
                    written_tracker.into_completion(FailureClass::RateLimited, None, Some(receipt)),
                    &written_current
                )
                .expect("写后结果可完成"),
            CoordinatorStep::Stop(RetryStopReason::ReplayNotProven)
        ));

        let mut safe_registry = HealthRegistry::new();
        let safe_initial = safe_registry.eligibility_for(compiled.routing(), HealthTick::new(1));
        let safe_plan = RoutePlanner::plan(&request, compiled.routing(), &safe_initial, 0)
            .expect("初始计划有效");
        let safe_first_resolved = compiled
            .resolve_plan_target(&safe_plan, 0)
            .expect("计划同代")
            .expect("首个目标存在");
        let safe_resolved = compiled
            .resolve_plan_target(&safe_plan, 1)
            .expect("计划同代")
            .expect("第二个目标存在");
        safe_registry.record_rate_limit(
            test_rate_limit_observation(
                safe_resolved.target(),
                RateLimitScope::Deployment,
                HealthTick::new(10),
            ),
            HealthTick::new(1),
        );
        let safe_current = safe_registry.eligibility_for(compiled.routing(), HealthTick::new(1));
        let refreshed = RoutePlanner::plan(&request, compiled.routing(), &safe_current, 0)
            .expect("冷却后的计划有效");
        assert_eq!(refreshed.target_id(0), Some(id(1)));
        assert_eq!(refreshed.target_id(1), Some(id(3)));

        let mut safe_coordinator = safe_plan
            .into_attempt_coordinator(RetryPolicy::new(3, 1_000).expect("策略有效"))
            .expect("计划与预算一致");
        let safe_first = safe_coordinator
            .start(&safe_initial)
            .expect("初始目标可签发");
        let mut safe_tracker = attempt::AttemptTracker::from_permit(safe_first);
        let receipt = safe_tracker
            .replay_reporter(safe_first_resolved)
            .expect("当前目标可签发回放上报器")
            .pre_execution_rejected(
                attempt::VerifiedPreExecutionContract::test_only_registered(
                    safe_first_resolved.target().site(),
                    0x1001,
                )
                .expect("已登记测试合同"),
            )
            .expect("同站合同可签发收据");
        let completion =
            safe_tracker.into_completion(FailureClass::RateLimited, Some(200), Some(receipt));
        let metadata = completion
            .outcome()
            .pre_execution_contract_metadata()
            .expect("受信收据摘要必须保留");
        assert_eq!(metadata.adapter_version, 1);
        assert_eq!(metadata.contract_revision, 1);
        assert_eq!(metadata.evidence_kind, 1);
        let CoordinatorStep::Next {
            permit: next,
            delay_ms,
        } = safe_coordinator
            .complete(completion, &safe_current)
            .expect("受信执行前拒绝允许推进")
        else {
            panic!("第二阶段已冷却时必须只签发第三阶段目标");
        };
        assert_eq!(delay_ms, 200);
        assert_eq!(next.index(), 2);
        assert_eq!(next.target(), id(3));

        let mut exhausted_registry = HealthRegistry::new();
        let exhausted_initial =
            exhausted_registry.eligibility_for(compiled.routing(), HealthTick::new(1));
        let exhausted_plan =
            RoutePlanner::plan(&request, compiled.routing(), &exhausted_initial, 0)
                .expect("初始计划有效");
        for attempt_index in [1, 2] {
            let resolved = compiled
                .resolve_plan_target(&exhausted_plan, attempt_index)
                .expect("计划同代")
                .expect("后续目标存在");
            exhausted_registry.record_rate_limit(
                test_rate_limit_observation(
                    resolved.target(),
                    RateLimitScope::Deployment,
                    HealthTick::new(10),
                ),
                HealthTick::new(1),
            );
        }
        let exhausted_current =
            exhausted_registry.eligibility_for(compiled.routing(), HealthTick::new(1));
        let mut exhausted_coordinator = exhausted_plan
            .into_attempt_coordinator(RetryPolicy::new(3, 1_000).expect("策略有效"))
            .expect("计划与预算一致");
        let exhausted_first = exhausted_coordinator
            .start(&exhausted_initial)
            .expect("初始目标可签发");
        let mut exhausted_tracker = attempt::AttemptTracker::from_permit(exhausted_first);
        let receipt = exhausted_tracker.test_only_rejection();
        let exhausted_completion =
            exhausted_tracker.into_completion(FailureClass::RateLimited, Some(200), Some(receipt));
        assert!(matches!(
            exhausted_coordinator
                .complete(exhausted_completion, &exhausted_current)
                .expect("受信执行前拒绝可完成"),
            CoordinatorStep::Stop(RetryStopReason::NoEligibleTargets)
        ));
    }

    #[test]
    fn retry_advances_only_with_replay_proof() {
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = plan(&snapshot, 0).expect("存在可用目标");
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
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = plan(&snapshot, 0).expect("存在可用目标");
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
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = plan(&snapshot, 0).expect("存在可用目标");
        let gate = RetryGate::new(RetryPolicy::new(2, 3_000).expect("策略有效"));

        let mut bytes_written = attempt::AttemptTracker::test_only(1);
        bytes_written.request_write_started().expect("允许记录写入");
        let receipt = bytes_written.test_only_rejection();
        assert_eq!(
            gate.decide(
                &plan,
                0,
                &bytes_written.into_outcome_for_test(
                    FailureClass::RateLimited,
                    Some(1_000),
                    Some(receipt),
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
        let receipt = semantic_event.test_only_rejection();
        assert_eq!(
            gate.decide(
                &plan,
                0,
                &semantic_event.into_outcome_for_test(
                    FailureClass::RateLimited,
                    Some(1_000),
                    Some(receipt),
                )
            ),
            RetryDecision::Stop(RetryStopReason::ReplayNotProven)
        );
    }

    #[test]
    fn retry_stops_for_terminal_conditions() {
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let plan = plan(&snapshot, 0).expect("存在可用目标");
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
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let (plan, eligibility) = plan_for_coordinator(&snapshot);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let first = coordinator.start(&eligibility).expect("允许签发首次尝试");
        assert_eq!(first.index(), 0);
        assert_eq!(first.target(), id(1));
        assert!(coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(&eligibility),
            Err(AttemptStartError::ActiveAttempt)
        ));

        let first_outcome = not_sent_for(first, FailureClass::Connect);
        let CoordinatorStep::Next {
            permit: second,
            delay_ms,
        } = coordinator
            .complete(first_outcome, &eligibility)
            .expect("零写入连接失败允许推进")
        else {
            panic!("应签发唯一的第二次尝试");
        };
        assert_eq!(delay_ms, 0);
        assert_eq!(second.index(), 1);
        assert_eq!(second.target(), id(2));
        assert!(coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(&eligibility),
            Err(AttemptStartError::ActiveAttempt)
        ));

        let second_outcome = not_sent_for(second, FailureClass::Connect);
        assert!(matches!(
            coordinator
                .complete(second_outcome, &eligibility)
                .expect("第二次可完成"),
            CoordinatorStep::Stop(RetryStopReason::Exhausted)
        ));
        assert!(coordinator.is_stopped());
        assert!(matches!(
            coordinator.start(&eligibility),
            Err(AttemptStartError::Stopped)
        ));
    }

    #[test]
    fn coordinator_stops_after_written_request_or_regular_rate_limit() {
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let (plan, eligibility) = plan_for_coordinator(&snapshot);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let first = coordinator.start(&eligibility).expect("允许签发首次尝试");
        let written = sent_for(first, FailureClass::RateLimited);
        assert!(matches!(
            coordinator
                .complete(written, &eligibility)
                .expect("写后结果可完成"),
            CoordinatorStep::Stop(RetryStopReason::ReplayNotProven)
        ));
        assert!(coordinator.is_stopped());
        assert!(!coordinator.has_active_attempt());
    }

    #[test]
    fn success_typestate_requires_written_complete_and_committed_response() {
        let tracker = attempt::AttemptTracker::test_only(1);
        let tracker = tracker
            .into_response_completed()
            .expect_err("未写入、未观察或未提交不能转为成功");
        assert_eq!(
            tracker
                .into_completion(FailureClass::Connect, None, None)
                .outcome()
                .delivery(),
            DeliveryState::NotSent
        );

        let mut tracker = attempt::AttemptTracker::test_only(1);
        tracker.request_write_started().expect("允许记录写入");
        tracker.downstream_committed().expect("允许记录下游提交");
        let tracker = tracker
            .into_response_completed()
            .expect_err("未观察上游响应不能转为成功");
        assert_eq!(
            tracker
                .into_completion(FailureClass::Server, None, None)
                .outcome()
                .delivery(),
            DeliveryState::Sent
        );

        let mut tracker = attempt::AttemptTracker::test_only(1);
        tracker.request_write_started().expect("允许记录写入");
        tracker
            .upstream_response_observed()
            .expect("允许记录响应头");
        let tracker = tracker
            .into_response_completed()
            .expect_err("未提交下游不能转为成功");
        assert_eq!(
            tracker
                .into_completion(FailureClass::Server, None, None)
                .outcome()
                .delivery(),
            DeliveryState::Sent
        );

        let mut tracker = attempt::AttemptTracker::test_only(1);
        tracker.request_write_started().expect("允许记录写入");
        tracker
            .upstream_response_observed()
            .expect("允许记录响应头");
        tracker.downstream_committed().expect("允许记录下游提交");
        let completion = tracker
            .into_response_completed()
            .expect("完整响应、已写入且下游提交可转为成功 typestate");
        consumes_success_completion(completion);
    }

    #[test]
    fn coordinator_success_completion_terminates_without_retry() {
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let (plan, eligibility) = plan_for_coordinator(&snapshot);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let permit = coordinator.start(&eligibility).expect("允许签发首次尝试");

        assert!(coordinator.complete_success(success_for(permit)).is_ok());
        assert!(coordinator.is_stopped());
        assert!(!coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(&eligibility),
            Err(AttemptStartError::Stopped)
        ));
    }

    #[test]
    fn foreign_success_completion_fails_closed() {
        let candidates = [ready(1, 1, 0)];
        let primary_snapshot = snapshot(&candidates, RoutingStrategy::Priority, 1);
        let (plan, eligibility) = plan_for_coordinator(&primary_snapshot);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(1, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let foreign_candidates = [ready(1, 1, 0)];
        let foreign_snapshot = snapshot(&foreign_candidates, RoutingStrategy::Priority, 1);
        let (foreign_plan, foreign_eligibility) = plan_for_coordinator(&foreign_snapshot);
        let mut foreign = foreign_plan
            .into_attempt_coordinator(RetryPolicy::new(1, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let foreign_permit = foreign
            .start(&foreign_eligibility)
            .expect("允许签发另一请求的首次尝试");
        let _current_permit = coordinator.start(&eligibility).expect("允许签发当前尝试");

        assert!(matches!(
            coordinator.complete_success(success_for(foreign_permit)),
            Err(AttemptCompleteError::Mismatched)
        ));
        assert!(coordinator.is_stopped());
        assert!(!coordinator.has_active_attempt());
    }

    #[test]
    fn coordinator_never_issues_next_after_sse_or_downstream_commit() {
        let semantic_candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let semantic_snapshot = snapshot(&semantic_candidates, RoutingStrategy::Priority, 2);
        let (semantic_plan, semantic_eligibility) = plan_for_coordinator(&semantic_snapshot);
        let mut semantic = semantic_plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let permit = semantic
            .start(&semantic_eligibility)
            .expect("允许签发首次尝试");
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
            semantic
                .complete(completion, &semantic_eligibility)
                .expect("语义事件可完成"),
            CoordinatorStep::Stop(RetryStopReason::ReplayNotProven)
        ));
        assert!(semantic.is_stopped());

        let downstream_candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let downstream_snapshot = snapshot(&downstream_candidates, RoutingStrategy::Priority, 2);
        let (downstream_plan, downstream_eligibility) = plan_for_coordinator(&downstream_snapshot);
        let mut downstream = downstream_plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let permit = downstream
            .start(&downstream_eligibility)
            .expect("允许签发首次尝试");
        let mut tracker = attempt::AttemptTracker::from_permit(permit);
        tracker.downstream_committed().expect("允许下游提交");
        let completion = tracker.into_completion(FailureClass::Server, None, None);
        assert!(matches!(
            downstream
                .complete(completion, &downstream_eligibility)
                .expect("下游提交可完成"),
            CoordinatorStep::Stop(RetryStopReason::DownstreamCommitted)
        ));
        assert!(downstream.is_stopped());
    }

    #[test]
    fn other_coordinator_completion_fails_closed_without_next_permit() {
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let primary_snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let (plan, eligibility) = plan_for_coordinator(&primary_snapshot);
        let mut coordinator = plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let other_candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let other_snapshot = snapshot(&other_candidates, RoutingStrategy::Priority, 2);
        let (other_plan, other_eligibility) = plan_for_coordinator(&other_snapshot);
        let mut other = other_plan
            .into_attempt_coordinator(RetryPolicy::new(2, 3_000).expect("策略有效"))
            .expect("计划与预算一致");
        let foreign = other
            .start(&other_eligibility)
            .expect("允许签发另一请求的首次尝试");
        let current = coordinator
            .start(&eligibility)
            .expect("允许签发当前请求的首次尝试");
        let mut current_tracker = attempt::AttemptTracker::from_permit(current);
        let receipt = current_tracker.test_only_rejection();
        let foreign_tracker = attempt::AttemptTracker::from_permit(foreign);
        let mismatched =
            foreign_tracker.into_completion(FailureClass::Connect, None, Some(receipt));
        assert_eq!(mismatched.outcome().delivery(), DeliveryState::Unknown);
        assert!(mismatched
            .outcome()
            .pre_execution_contract_metadata()
            .is_none());

        assert!(matches!(
            coordinator.complete(mismatched, &eligibility),
            Err(AttemptCompleteError::Mismatched)
        ));
        assert!(coordinator.is_stopped());
        assert!(!coordinator.has_active_attempt());
        assert!(matches!(
            coordinator.start(&eligibility),
            Err(AttemptStartError::Stopped)
        ));
    }

    #[test]
    fn coordinator_rejects_retry_budget_that_cannot_reach_every_stage() {
        let candidates = [ready(1, 1, 0), ready(2, 2, 0)];
        let snapshot = snapshot(&candidates, RoutingStrategy::Priority, 2);
        let (plan, _) = plan_for_coordinator(&snapshot);
        let result = plan.into_attempt_coordinator(RetryPolicy::new(1, 3_000).expect("策略有效"));

        assert!(matches!(
            result,
            Err(AttemptCoordinatorBuildError::InsufficientAttemptBudget)
        ));
    }

    #[test]
    fn route_plan_stays_small_and_stack_only() {
        assert!(core::mem::size_of::<RoutePlan>() <= 160);
        assert!(core::mem::size_of::<AttemptCoordinator>() <= 224);
        assert_eq!(core::mem::size_of::<RouteTargetId>(), 8);
    }
}
