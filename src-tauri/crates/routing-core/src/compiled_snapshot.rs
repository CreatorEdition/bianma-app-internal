//! 路由候选与账户选择合同的同代编译快照。
//!
//! 本模块只在快照编译时验证 Target 对 AccountSelector 的引用完整性。catalog 不会进入
//! Planner 热路径、RoutePlan、Coordinator 或 Attempt 状态机。

use super::{
    AccountSelectorCatalog, AccountSelectorCatalogError, AccountSelectorDefinition,
    AccountSelectorId, PlanError, RouteCandidate, RoutingSnapshot, RoutingStrategy,
    SnapshotVersion,
};

/// 编译路由快照时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompiledRoutingSnapshotError {
    /// 账户选择合同目录本身非法。
    Catalog(AccountSelectorCatalogError),
    /// 路由候选或路由快照形状非法。
    Plan(PlanError),
}

/// 同时持有路由候选快照与账户选择合同目录的不可变编译结果。
///
/// 该类型是 crate 外构造 RoutingSnapshot 的唯一入口。它只借用同代配置，不维护全局
/// registry，也不在请求热路径重复查询 catalog。
pub struct CompiledRoutingSnapshot<'a> {
    routing: RoutingSnapshot<'a>,
    catalog: AccountSelectorCatalog<'a>,
}

impl<'a> CompiledRoutingSnapshot<'a> {
    /// 编译候选与账户选择合同，并拒绝任何悬空 Selector 引用。
    pub fn compile(
        version: SnapshotVersion,
        candidates: &'a [RouteCandidate],
        strategy: RoutingStrategy,
        max_attempts: u8,
        selectors: &'a [AccountSelectorDefinition<'a>],
    ) -> Result<Self, CompiledRoutingSnapshotError> {
        let catalog = AccountSelectorCatalog::new(selectors)
            .map_err(CompiledRoutingSnapshotError::Catalog)?;
        let routing = RoutingSnapshot::new(version, candidates, strategy, max_attempts)
            .map_err(CompiledRoutingSnapshotError::Plan)?;

        if candidates
            .iter()
            .any(|candidate| !catalog.contains(candidate.target().account_selector()))
        {
            return Err(CompiledRoutingSnapshotError::Plan(
                PlanError::UnknownAccountSelector,
            ));
        }

        Ok(Self { routing, catalog })
    }

    /// 返回已完成结构校验的路由候选快照。
    pub const fn routing(&self) -> &RoutingSnapshot<'a> {
        &self.routing
    }

    /// 在同一编译代内查询账户选择合同。
    pub fn selector(&self, id: AccountSelectorId) -> Option<&AccountSelectorDefinition<'a>> {
        self.catalog.get(id)
    }
}
