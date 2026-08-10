//! 全局额度组的静态运行时布局合同。
//!
//! 本模块只在快照激活前核对所有可达额度组的唯一静态定义。它不选择账户、不发放
//! Lease、不维护在途数、健康、游标或任何其他运行时状态。

use core::num::NonZeroU16;

use super::{PlanError, QuotaGroupId, ResolvedRouteTarget, RoutingSnapshot};

/// 单个编译快照允许追踪的最多全局额度组数量。
///
/// 该上限是本地路由核心的明确产品边界。超过上限的配置必须在激活前拒绝，不能拆分
/// Registry、静默降级，或把本应共享的额度组误当作多个独立状态。
pub const MAX_TRACKED_QUOTA_GROUPS: usize = 256;

/// 一个全局额度组的静态运行时定义。
///
/// 本定义不携带账户、凭据、API Key、Bearer、Secret 或任何可用于发起上游请求的材料；
/// `max_inflight` 只为后续同一布局上的原子 Lease 合同预留固定上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaGroupRuntimeDefinition {
    id: QuotaGroupId,
    max_inflight: NonZeroU16,
}

impl QuotaGroupRuntimeDefinition {
    /// 创建一个由快照布局工厂后续校验可达性的额度组定义。
    pub const fn new(id: QuotaGroupId, max_inflight: NonZeroU16) -> Self {
        Self { id, max_inflight }
    }

    /// 返回稳定的额度组标识。
    pub const fn id(self) -> QuotaGroupId {
        self.id
    }

    /// 返回该组允许的正整数在途上限。
    pub const fn max_inflight(self) -> NonZeroU16 {
        self.max_inflight
    }
}

/// 构造全局额度组静态定义输入时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaGroupRuntimeDefinitionsError {
    /// 没有任何额度组定义。
    Empty,
    /// 输入定义数量超过固定上限。
    TooMany,
    /// 同一额度组标识出现了多个定义。
    DuplicateDefinition,
}

/// 借用式、固定上限的全局额度组静态定义输入。
///
/// 该类型只验证输入切片自身的形状；是否刚好覆盖一个编译快照的全部可达额度组，必须由
/// 同代 `CompiledRoutingSnapshot` 的 crate 内布局工厂验证。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaGroupRuntimeDefinitions<'a> {
    definitions: &'a [QuotaGroupRuntimeDefinition],
}

impl<'a> QuotaGroupRuntimeDefinitions<'a> {
    /// 验证并借用固定上限内的额度组静态定义。
    pub fn new(
        definitions: &'a [QuotaGroupRuntimeDefinition],
    ) -> Result<Self, QuotaGroupRuntimeDefinitionsError> {
        if definitions.is_empty() {
            return Err(QuotaGroupRuntimeDefinitionsError::Empty);
        }
        if definitions.len() > MAX_TRACKED_QUOTA_GROUPS {
            return Err(QuotaGroupRuntimeDefinitionsError::TooMany);
        }
        for (index, definition) in definitions.iter().enumerate() {
            if definitions[..index]
                .iter()
                .any(|previous| previous.id() == definition.id())
            {
                return Err(QuotaGroupRuntimeDefinitionsError::DuplicateDefinition);
            }
        }
        Ok(Self { definitions })
    }

    /// 返回已验证定义数量。
    pub const fn len(&self) -> usize {
        self.definitions.len()
    }

    /// 判断定义输入是否为空；经 [`Self::new`] 构造后始终为 `false`。
    pub const fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// 返回已验证的、且不含 Secret 的静态定义切片。
    pub const fn as_slice(&self) -> &'a [QuotaGroupRuntimeDefinition] {
        self.definitions
    }

    pub(crate) fn get(&self, id: QuotaGroupId) -> Option<QuotaGroupRuntimeDefinition> {
        self.definitions
            .iter()
            .copied()
            .find(|definition| definition.id() == id)
    }
}

/// 将额度组定义绑定到编译快照时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectionRuntimeLayoutError {
    /// 同一可达路由候选引用的 Selector 在已编译目录中缺失；理论不变量失效时保守拒绝。
    UnknownReachableSelector,
    /// 所有可达 Selector 去重后引用的额度组超过固定上限。
    TooManyReachableGroups,
    /// 一个可达额度组没有对应的静态运行时定义。
    MissingReachableGroupDefinition,
    /// 输入定义未被任何可达 Selector 引用。
    UnusedDefinition,
}

/// 与唯一 `RoutingSnapshot` 实例绑定的只读全局额度组布局。
///
/// 本类型只能由同代 [`CompiledRoutingSnapshot`](super::CompiledRoutingSnapshot) 创建，且只
/// 保存已经验证的借用定义。它不保存容量计数器、不创建 Lease，也不执行账户选择。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct SelectionRuntimeLayout<'snapshot, 'config> {
    routing: &'snapshot RoutingSnapshot<'config>,
    definitions: QuotaGroupRuntimeDefinitions<'config>,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'snapshot, 'config> SelectionRuntimeLayout<'snapshot, 'config> {
    pub(crate) const fn new(
        routing: &'snapshot RoutingSnapshot<'config>,
        definitions: QuotaGroupRuntimeDefinitions<'config>,
    ) -> Self {
        Self {
            routing,
            definitions,
        }
    }

    /// 按稳定额度组标识查询静态在途上限；未知组不回退。
    pub(crate) fn max_inflight(&self, id: QuotaGroupId) -> Option<NonZeroU16> {
        self.definitions
            .get(id)
            .map(|definition| definition.max_inflight())
    }

    /// 在先确认解析目标属于同一快照实例后查询静态在途上限。
    ///
    /// 此入口不执行选择；它只为未来 C2 的受控 Lease 获取链保留同代绑定证据。版本号或
    /// TargetId 相同的其他快照解析目标一律拒绝，绝不重绑到当前布局。
    pub(crate) fn max_inflight_for(
        &self,
        resolved: ResolvedRouteTarget<'snapshot, 'config>,
        id: QuotaGroupId,
    ) -> Result<Option<NonZeroU16>, PlanError> {
        if !resolved.matches_snapshot(self.routing) {
            return Err(PlanError::StaleSnapshot);
        }
        Ok(self.max_inflight(id))
    }
}
