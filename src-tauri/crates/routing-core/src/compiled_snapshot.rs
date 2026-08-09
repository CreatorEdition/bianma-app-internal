//! 路由候选、模型部署与账户选择合同的同代编译快照。
//!
//! 本模块只在快照编译时验证 Target 对 ModelDeployment、AccountSelector、Account 与
//! Credential 的静态引用完整性。catalog 不会进入 Planner 热路径、RoutePlan、Coordinator
//! 或 Attempt 状态机。

use super::{
    AccountCatalog, AccountCatalogError, AccountCredentialDefinitions, AccountSelectorCatalog,
    AccountSelectorCatalogError, AccountSelectorDefinition, CredentialCatalog,
    CredentialCatalogError, ModelDeploymentCatalog, ModelDeploymentCatalogError,
    ModelDeploymentDefinition, PlanError, RouteCandidate, RoutePlan, RouteStageId, RouteTarget,
    RoutingSnapshot, RoutingStrategy, SnapshotVersion,
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
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResolvedRouteTarget<'a> {
    snapshot_version: SnapshotVersion,
    stage: RouteStageId,
    target: RouteTarget,
    deployment: &'a ModelDeploymentDefinition,
    selector: &'a AccountSelectorDefinition<'a>,
}

impl fmt::Debug for ResolvedRouteTarget<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ResolvedRouteTarget")
            .field("snapshot_version", &self.snapshot_version)
            .field("stage", &self.stage)
            .field("target", &self.target)
            .finish()
    }
}

impl<'a> ResolvedRouteTarget<'a> {
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
    pub(crate) const fn deployment(self) -> &'a ModelDeploymentDefinition {
        self.deployment
    }

    /// 返回与该目标同代的账户选择合同。
    ///
    /// 仅供 crate 内受控执行层消费，crate 外不能取得裸 Definition 再手工配对计划。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn selector(self) -> &'a AccountSelectorDefinition<'a> {
        self.selector
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
    pub fn resolve_plan_target(
        &self,
        plan: &RoutePlan<'_, 'a>,
        attempt_index: u8,
    ) -> Result<Option<ResolvedRouteTarget<'a>>, PlanError> {
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
            snapshot_version: self.routing.version(),
            stage,
            target: *target,
            deployment,
            selector,
        }))
    }
}
