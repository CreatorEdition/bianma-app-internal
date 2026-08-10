//! 路由候选、模型部署与账户选择合同的同代编译快照。
//!
//! 本模块只在快照编译时验证 Target 对 ModelDeployment、AccountSelector、Account 与
//! Credential 的静态引用完整性。catalog 不会进入 Planner 热路径、RoutePlan、Coordinator
//! 或 Attempt 状态机。

use super::{
    AccountCatalog, AccountCatalogError, AccountCredentialDefinitions, AccountSelectionCandidates,
    AccountSelectionRequest, AccountSelectorCatalog, AccountSelectorCatalogError,
    AccountSelectorDefinition, CredentialCatalog, CredentialCatalogError, ModelDeploymentCatalog,
    ModelDeploymentCatalogError, ModelDeploymentDefinition, PlanError, RouteCandidate, RoutePlan,
    RouteStageId, RouteTarget, RoutingSnapshot, RoutingStrategy, SelectionSession, SnapshotVersion,
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
    #[allow(dead_code)]
    accounts: AccountCatalog<'a>,
    #[allow(dead_code)]
    credentials: CredentialCatalog<'a>,
}

impl<'a> CompiledRoutingSnapshot<'a> {
    /// 编译候选、模型部署、账户选择合同与静态身份目录，并拒绝任何悬空或不一致引用。
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
}
