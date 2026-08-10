//! 路由候选、模型部署与账户选择合同的同代编译快照。
//!
//! 本模块只在快照编译时验证 Target 对 ModelDeployment、AccountSelector、Account 与
//! Credential 的静态引用完整性。catalog 不会进入 Planner 热路径、RoutePlan、Coordinator
//! 或 Attempt 状态机。

use super::{
    AccountCatalog, AccountCatalogError, AccountCredentialDefinitions, AccountSelectionCandidates,
    AccountSelectionRequest, AccountSelectorCatalog, AccountSelectorCatalogError,
    AccountSelectorDefinition, CredentialCatalog, CredentialCatalogError,
    CredentialSelectionPolicy, ModelDeploymentCatalog, ModelDeploymentCatalogError,
    ModelDeploymentDefinition, PlanError, RouteCandidate, RoutePlan, RouteStageId, RouteTarget,
    RouteTargetId, RoutingSnapshot, RoutingStrategy, SelectionRuntimeDefinitions,
    SelectionRuntimeLayout, SelectionRuntimeLayoutError, SelectionSession, SnapshotVersion,
    MAX_TRACKED_ACCOUNTS, MAX_TRACKED_CREDENTIALS, MAX_TRACKED_QUOTA_GROUPS,
};
use core::fmt;

/// 编译路由快照时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompiledRoutingSnapshotError {
    /// 账户目录本身非法。
    AccountCatalog(AccountCatalogError),
    /// 账户选择合同目录本身非法。
    Catalog(AccountSelectorCatalogError),
    /// 凭据目录本身非法。
    CredentialCatalog(CredentialCatalogError),
    /// 模型部署目录本身非法。
    DeploymentCatalog(ModelDeploymentCatalogError),
    /// 路由候选或路由快照形状非法。
    Plan(PlanError),
}

/// 一次计划尝试在同代编译快照中解析出的完整静态路由配置。
///
/// 该值不选择 Account/Credential，也不包含健康、额度、租约、Secret、传输或请求内容。
#[derive(Clone, Copy)]
pub struct ResolvedRouteTarget<'snapshot, 'config> {
    routing: &'snapshot RoutingSnapshot<'config>,
    snapshot_version: SnapshotVersion,
    stage: RouteStageId,
    target: RouteTarget,
    deployment: &'config ModelDeploymentDefinition,
    selector: &'config AccountSelectorDefinition<'config>,
}

impl PartialEq for ResolvedRouteTarget<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        core::ptr::eq(self.routing, other.routing)
            && self.snapshot_version == other.snapshot_version
            && self.stage == other.stage
            && self.target == other.target
            && self.deployment == other.deployment
            && self.selector == other.selector
    }
}

impl Eq for ResolvedRouteTarget<'_, '_> {}

impl fmt::Debug for ResolvedRouteTarget<'_, '_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedRouteTarget")
            .field("snapshot_version", &self.snapshot_version)
            .field("stage", &self.stage)
            .field("target", &self.target)
            .finish()
    }
}

impl<'snapshot, 'config> ResolvedRouteTarget<'snapshot, 'config> {
    /// 返回解析所用的编译快照版本。
    pub const fn snapshot_version(self) -> SnapshotVersion {
        self.snapshot_version
    }

    /// 返回当前尝试所属的稳定路由阶段。
    pub const fn stage(self) -> RouteStageId {
        self.stage
    }

    /// 返回当前尝试绑定的完整路由目标。
    pub const fn target(self) -> RouteTarget {
        self.target
    }

    /// 返回与该目标同代的模型部署定义。
    ///
    /// 仅供 crate 内受控执行层消费，crate 外不能取得裸 Definition 再手工配对计划。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn deployment(self) -> &'config ModelDeploymentDefinition {
        self.deployment
    }

    /// 返回与该目标同代的账户选择合同。
    ///
    /// 仅供 crate 内受控执行层消费，crate 外不能取得裸 Definition 再手工配对计划。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn selector(self) -> &'config AccountSelectorDefinition<'config> {
        self.selector
    }

    /// 判断是否由指定的同一 RoutingSnapshot 实例解析而来。
    ///
    /// 版本号或 TargetId 相同不足以代表静态配置相同；冷却能力必须依赖这一实例身份
    /// 拒绝跨快照归因。
    pub(crate) fn matches_snapshot(&self, snapshot: &RoutingSnapshot<'config>) -> bool {
        core::ptr::eq(self.routing, snapshot)
    }
}

/// 同时持有路由候选快照、模型部署目录、账户选择合同和静态身份目录的不可变编译结果。
///
/// 该类型是 crate 外构造 RoutingSnapshot 的唯一入口。它只借用同代配置，不维护全局
/// registry，也不在请求热路径重复查询 catalog。
pub struct CompiledRoutingSnapshot<'a> {
    routing: RoutingSnapshot<'a>,
    deployments: ModelDeploymentCatalog<'a>,
    selectors: AccountSelectorCatalog<'a>,
    accounts: AccountCatalog<'a>,
    credentials: CredentialCatalog<'a>,
}

impl<'a> CompiledRoutingSnapshot<'a> {
    /// 编译候选、模型部署、账户选择合同与静态身份目录，并拒绝任何悬空或不一致引用。
    ///
    /// `version` 是不可变配置代：宿主必须在任一静态路由或资源身份语义变更时递增它，
    /// 特别是禁止在同一版本内将既有 Account、Credential 或 QuotaGroup ID 重绑定到另一
    /// 实际资源。这样独立的资源冷却 Registry 才能在版本切换时安全丢弃旧状态。
    pub fn compile(
        version: SnapshotVersion,
        candidates: &'a [RouteCandidate],
        strategy: RoutingStrategy,
        max_attempts: u8,
        deployments: &'a [ModelDeploymentDefinition],
        account_credentials: AccountCredentialDefinitions<'a>,
        selectors: &'a [AccountSelectorDefinition<'a>],
    ) -> Result<Self, CompiledRoutingSnapshotError> {
        let routing = RoutingSnapshot::new(version, candidates, strategy, max_attempts)
            .map_err(CompiledRoutingSnapshotError::Plan)?;
        let deployments = ModelDeploymentCatalog::new(deployments)
            .map_err(CompiledRoutingSnapshotError::DeploymentCatalog)?;

        for candidate in candidates {
            let target = candidate.target();
            let deployment =
                deployments
                    .get(target.deployment())
                    .ok_or(CompiledRoutingSnapshotError::Plan(
                        PlanError::UnknownModelDeployment,
                    ))?;
            if deployment.site() != target.site() {
                return Err(CompiledRoutingSnapshotError::Plan(
                    PlanError::TargetDeploymentSiteMismatch,
                ));
            }
            if deployment.endpoint() != target.endpoint() {
                return Err(CompiledRoutingSnapshotError::Plan(
                    PlanError::TargetDeploymentEndpointMismatch,
                ));
            }
        }

        let selectors = AccountSelectorCatalog::new(selectors)
            .map_err(CompiledRoutingSnapshotError::Catalog)?;

        if candidates
            .iter()
            .any(|candidate| !selectors.contains(candidate.target().account_selector()))
        {
            return Err(CompiledRoutingSnapshotError::Plan(
                PlanError::UnknownAccountSelector,
            ));
        }

        let accounts = AccountCatalog::new(account_credentials.accounts())
            .map_err(CompiledRoutingSnapshotError::AccountCatalog)?;
        let credentials = CredentialCatalog::new(account_credentials.credentials(), &accounts)
            .map_err(CompiledRoutingSnapshotError::CredentialCatalog)?;

        for candidate in candidates {
            let target = candidate.target();
            let deployment =
                deployments
                    .get(target.deployment())
                    .ok_or(CompiledRoutingSnapshotError::Plan(
                        PlanError::UnknownModelDeployment,
                    ))?;
            let selector = selectors.get(target.account_selector()).ok_or(
                CompiledRoutingSnapshotError::Plan(PlanError::UnknownAccountSelector),
            )?;

            for member in selector.members() {
                let account =
                    accounts
                        .get(member.account())
                        .ok_or(CompiledRoutingSnapshotError::Plan(
                            PlanError::UnknownAccount,
                        ))?;
                let credential = credentials.get(member.credential()).ok_or(
                    CompiledRoutingSnapshotError::Plan(PlanError::UnknownCredential),
                )?;
                if credential.account() != member.account() {
                    return Err(CompiledRoutingSnapshotError::Plan(
                        PlanError::CredentialAccountMismatch,
                    ));
                }
                if account.site() != deployment.site() {
                    return Err(CompiledRoutingSnapshotError::Plan(
                        PlanError::AccountDeploymentSiteMismatch,
                    ));
                }
            }
        }

        Ok(Self {
            routing,
            deployments,
            selectors,
            accounts,
            credentials,
        })
    }

    /// 返回已完成结构校验的路由候选快照。
    pub const fn routing(&self) -> &RoutingSnapshot<'a> {
        &self.routing
    }

    /// 解析计划中某次尝试的同代 Target、模型部署与账户选择合同。
    ///
    /// 先用当前快照验证计划版本与 Target，再从同一编译代的 catalog 查询 deployment 与
    /// selector；任何理论不变量破坏均 fail closed，绝不回退到外部或全局目录。
    pub fn resolve_plan_target<'snapshot>(
        &'snapshot self,
        plan: &RoutePlan<'snapshot, 'a>,
        attempt_index: u8,
    ) -> Result<Option<ResolvedRouteTarget<'snapshot, 'a>>, PlanError> {
        if !core::ptr::eq(plan.snapshot, &self.routing) {
            return Err(PlanError::StaleSnapshot);
        }
        let Some(target) = plan.resolve(attempt_index)? else {
            return Ok(None);
        };
        let stage = self
            .routing
            .stage_for(target.id())
            .ok_or(PlanError::UnknownTarget)?;
        let deployment = self
            .deployments
            .get(target.deployment())
            .ok_or(PlanError::UnknownModelDeployment)?;
        let selector = self
            .selectors
            .get(target.account_selector())
            .ok_or(PlanError::UnknownAccountSelector)?;

        Ok(Some(ResolvedRouteTarget {
            routing: &self.routing,
            snapshot_version: self.routing.version(),
            stage,
            target: *target,
            deployment,
            selector,
        }))
    }

    /// 从同一 RoutingSnapshot 实例解析出的目标创建账户选择请求。
    ///
    /// SnapshotVersion 和 TargetId 相同不足以证明模型部署、账户目录或 Selector 定义一致；
    /// 因此来自其他实例的解析目标一律按陈旧快照拒绝。该工厂只建立输入绑定，不选择
    /// Account/Credential，也不验证动态额度、健康或 Lease。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn selection_request<'snapshot>(
        &'snapshot self,
        resolved: ResolvedRouteTarget<'snapshot, 'a>,
        session: SelectionSession,
    ) -> Result<AccountSelectionRequest<'snapshot, 'a>, PlanError> {
        if !resolved.matches_snapshot(&self.routing) {
            return Err(PlanError::StaleSnapshot);
        }
        Ok(AccountSelectionRequest::new(resolved, session))
    }

    /// 从同一 RoutingSnapshot 实例解析出的目标取得只读账户选择候选视图。
    ///
    /// 版本号或 TargetId 相同不足以保证静态账户目录、站点和部署一致；因此跨实例目标
    /// 一律按陈旧快照拒绝，而不允许候选视图回退或重绑到当前 catalog。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn account_selection_candidates<'snapshot>(
        &'snapshot self,
        resolved: ResolvedRouteTarget<'snapshot, 'a>,
    ) -> Result<AccountSelectionCandidates<'snapshot, 'a>, PlanError> {
        if !resolved.matches_snapshot(&self.routing) {
            return Err(PlanError::StaleSnapshot);
        }
        Ok(AccountSelectionCandidates::new(resolved))
    }

    /// 验证三类静态运行时定义，并创建绑定当前 RoutingSnapshot 实例的只读布局。
    ///
    /// 仅扫描全部可达 `RouteCandidate → Selector → Unit/Member`。相同 Account、Credential
    /// 或 Group 即使跨多个 Selector 出现，也只占用一个固定槽；任一资源超过边界、缺失
    /// 定义或输入中存在未被可达候选使用的定义均拒绝激活。该工厂不创建 Registry、Lease
    /// 或在途计数。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn selection_runtime_layout<'snapshot>(
        &'snapshot self,
        definitions: &SelectionRuntimeDefinitions<'a>,
    ) -> Result<SelectionRuntimeLayout<'snapshot, 'a>, SelectionRuntimeLayoutError> {
        let mut reachable_groups = [None; MAX_TRACKED_QUOTA_GROUPS];
        let mut reachable_group_len = 0usize;
        let mut reachable_accounts = [None; MAX_TRACKED_ACCOUNTS];
        let mut reachable_account_len = 0usize;
        let mut reachable_credentials = [None; MAX_TRACKED_CREDENTIALS];
        let mut reachable_credential_len = 0usize;

        for candidate in self.routing.candidates() {
            let selector = self
                .selectors
                .get(candidate.target().account_selector())
                .ok_or(SelectionRuntimeLayoutError::UnknownReachableSelector)?;
            for unit in selector.units() {
                for group in unit.quota_groups() {
                    if !append_unique(&mut reachable_groups, &mut reachable_group_len, *group) {
                        return Err(SelectionRuntimeLayoutError::TooManyReachableGroups);
                    }
                }
            }
            for member in selector.members() {
                if self.accounts.get(member.account()).is_none() {
                    return Err(SelectionRuntimeLayoutError::UnknownReachableAccount);
                }
                if self.credentials.get(member.credential()).is_none() {
                    return Err(SelectionRuntimeLayoutError::UnknownReachableCredential);
                }
                if !append_unique(
                    &mut reachable_accounts,
                    &mut reachable_account_len,
                    member.account(),
                ) {
                    return Err(SelectionRuntimeLayoutError::TooManyReachableAccounts);
                }
                if !append_unique(
                    &mut reachable_credentials,
                    &mut reachable_credential_len,
                    member.credential(),
                ) {
                    return Err(SelectionRuntimeLayoutError::TooManyReachableCredentials);
                }
            }
        }

        for group in reachable_groups[..reachable_group_len].iter().flatten() {
            if definitions.quota_group(*group).is_none() {
                return Err(SelectionRuntimeLayoutError::MissingReachableQuotaGroupDefinition);
            }
        }
        for account in reachable_accounts[..reachable_account_len].iter().flatten() {
            if definitions.account(*account).is_none() {
                return Err(SelectionRuntimeLayoutError::MissingReachableAccountDefinition);
            }
        }
        for credential in reachable_credentials[..reachable_credential_len]
            .iter()
            .flatten()
        {
            if definitions.credential(*credential).is_none() {
                return Err(SelectionRuntimeLayoutError::MissingReachableCredentialDefinition);
            }
        }

        for definition in definitions.quota_groups() {
            let is_reachable = reachable_groups[..reachable_group_len]
                .iter()
                .flatten()
                .any(|group| *group == definition.id());
            if !is_reachable {
                return Err(SelectionRuntimeLayoutError::UnusedQuotaGroupDefinition);
            }
        }
        for definition in definitions.accounts() {
            let is_reachable = reachable_accounts[..reachable_account_len]
                .iter()
                .flatten()
                .any(|account| *account == definition.id());
            if !is_reachable {
                return Err(SelectionRuntimeLayoutError::UnusedAccountDefinition);
            }
        }
        for definition in definitions.credentials() {
            let is_reachable = reachable_credentials[..reachable_credential_len]
                .iter()
                .flatten()
                .any(|credential| *credential == definition.id());
            if !is_reachable {
                return Err(SelectionRuntimeLayoutError::UnusedCredentialDefinition);
            }
        }

        Ok(SelectionRuntimeLayout::new(&self.routing, *definitions))
    }

    /// 返回一个同代 Target 声明的账户选择策略。
    ///
    /// 此入口只供 Registry 激活时拒绝未实现策略；无法解析 Target 或 Selector 时返回
    /// `None`，调用方必须 fail closed。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn selection_policy_for(
        &self,
        target: RouteTargetId,
    ) -> Option<CredentialSelectionPolicy> {
        let selector_id = self.routing.resolve(target)?.account_selector();
        self.selectors
            .get(selector_id)
            .map(|selector| selector.policy())
    }
}

fn append_unique<T: Copy + PartialEq>(slots: &mut [Option<T>], len: &mut usize, value: T) -> bool {
    if slots[..*len]
        .iter()
        .flatten()
        .any(|previous| *previous == value)
    {
        return true;
    }
    if *len == slots.len() {
        return false;
    }
    slots[*len] = Some(value);
    *len += 1;
    true
}
