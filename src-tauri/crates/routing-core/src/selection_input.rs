//! 账户选择的会话与动态资格输入合同。
//!
//! 本模块只把宿主提供的已 HMAC 化会话别名和动态可用性收敛为有界输入。它不保存原始
//! Session、不实现 HMAC、不选择账户或凭据，也不维护 Lease、健康、额度或重试状态。

use super::{ResolvedRouteTarget, RouteTarget, SelectorAffinitySalt, SelectorRevision};

/// 会话亲和别名的固定字节长度。
///
/// 宿主应以私有 HMAC Key 对原始 Session 计算并截断为该长度后再传入本 crate；不可将
/// 原始 Session ID、Cookie、Bearer 或其他可识别文本传入路由核心。
pub const SESSION_AFFINITY_ALIAS_BYTES: usize = 16;

/// 由宿主 HMAC 化后的固定长度会话亲和别名。
///
/// 本类型没有 `Debug` 或 `Display` 实现，也不提供还原或读取字节的公开接口。它只能
/// 表达宿主已完成 HMAC 的别名，routing-core 不接收或保存原始会话内容。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct SessionAffinityAlias([u8; SESSION_AFFINITY_ALIAS_BYTES]);

impl SessionAffinityAlias {
    /// 接收宿主已 HMAC 化并截断到固定长度的会话别名。
    ///
    /// 本 crate 不持有 HMAC Key，无法也不尝试验证计算过程；调用方必须在边界处保证
    /// 输入不是原始 Session、Cookie、Bearer 或任何其他明文标识。
    pub const fn from_host_hmac(value: [u8; SESSION_AFFINITY_ALIAS_BYTES]) -> Self {
        Self(value)
    }
}

/// 本次选择可用的会话亲和输入。
///
/// `Stable` 仅承载宿主 HMAC 后的固定长度别名；本枚举刻意不实现 `Debug`，以免日志或
/// 错误路径意外记录会话关联材料。
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum SelectionSession {
    /// 当前请求没有可用的稳定会话亲和输入。
    Absent,
    /// 当前请求携带宿主已 HMAC 化的稳定会话别名。
    Stable(SessionAffinityAlias),
}

#[cfg_attr(not(test), allow(dead_code))]
impl SelectionSession {
    /// 返回本请求是否携带稳定会话亲和输入。
    pub(crate) const fn is_stable(self) -> bool {
        matches!(self, Self::Stable(_))
    }

}

/// 经同一编译快照校验的账户选择请求。
///
/// 只能由 `CompiledRoutingSnapshot::selection_request` 构造，因而不会把版本号或 TargetId
/// 恰好相同、但静态目录不同的另一个 RoutingSnapshot 重新绑定到当前选择合同。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct AccountSelectionRequest<'snapshot, 'config> {
    resolved: ResolvedRouteTarget<'snapshot, 'config>,
    session: SelectionSession,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'snapshot, 'config> AccountSelectionRequest<'snapshot, 'config> {
    /// 从已由编译快照验证的解析目标创建输入请求。
    pub(crate) const fn new(
        resolved: ResolvedRouteTarget<'snapshot, 'config>,
        session: SelectionSession,
    ) -> Self {
        Self { resolved, session }
    }

    /// 返回请求绑定的完整路由目标。
    pub(crate) const fn target(self) -> RouteTarget {
        self.resolved.target()
    }

    /// 返回请求绑定的不可变 Selector 修订。
    pub(crate) const fn selector_revision(self) -> SelectorRevision {
        self.resolved.selector().revision()
    }

    /// 返回请求绑定的 Selector 亲和盐。
    pub(crate) const fn selector_affinity_salt(self) -> SelectorAffinitySalt {
        self.resolved.selector().affinity_salt()
    }

    /// 返回是否携带稳定会话亲和输入，不暴露别名内容。
    pub(crate) const fn has_stable_session(self) -> bool {
        self.session.is_stable()
    }

    /// 仅供 crate 内未来选择实现取得会话输入。
    pub(crate) const fn session(self) -> SelectionSession {
        self.session
    }

    /// 返回同代解析目标，仅供 crate 内后续受控选择层使用。
    pub(crate) const fn resolved(self) -> ResolvedRouteTarget<'snapshot, 'config> {
        self.resolved
    }
}

/// 构造动态账户选择资格时的拒绝原因。
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AccountSelectionEligibilityError {
    /// Unit 掩码设置了当前 Selector 不存在的声明位。
    UnitMaskOutOfBounds,
    /// Member 掩码设置了当前 Selector 不存在的声明位。
    MemberMaskOutOfBounds,
    /// 已允许 Member 所属的静态 Unit 不存在；理论不变量失效时保守拒绝。
    UnknownMemberUnit,
    /// 已允许 Member 的所属 Unit 没有同时获准。
    MemberUnitNotAllowed,
    /// 已允许 Unit 没有任何同 Unit 的已允许 Member。
    AllowedUnitWithoutMember,
}

/// 绑定一个选择请求的动态账户/凭据资格位图。
///
/// Unit 和 Member 位分别按同代 Selector 定义的声明顺序编号，最多各 16 位。该类型只
/// 表达宿主已核验的动态可用性；它不把任一 Account/Credential 标记为已选中，也不包含
/// 额度、健康、游标、在途数、Lease 或 Secret。
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct AccountSelectionEligibility<'snapshot, 'config> {
    request: AccountSelectionRequest<'snapshot, 'config>,
    unit_mask: u16,
    member_mask: u16,
}

#[cfg_attr(not(test), allow(dead_code))]
impl<'snapshot, 'config> AccountSelectionEligibility<'snapshot, 'config> {
    /// 验证并绑定动态资格位图到唯一的选择请求。
    ///
    /// 任何超出当前 Selector 声明范围的位、获准 Member 未获其 Unit 授权，或获准 Unit
    /// 缺少获准 Member 都被拒绝。两个掩码同时为零可保守表达“当前没有动态可用成员”。
    pub(crate) fn new(
        request: AccountSelectionRequest<'snapshot, 'config>,
        unit_mask: u16,
        member_mask: u16,
    ) -> Result<Self, AccountSelectionEligibilityError> {
        let selector = request.resolved().selector();
        if unit_mask & !known_mask(selector.units().len()) != 0 {
            return Err(AccountSelectionEligibilityError::UnitMaskOutOfBounds);
        }
        if member_mask & !known_mask(selector.members().len()) != 0 {
            return Err(AccountSelectionEligibilityError::MemberMaskOutOfBounds);
        }

        for (member_index, member) in selector.members().iter().enumerate() {
            if !mask_includes(member_mask, member_index) {
                continue;
            }
            let Some(unit_index) = selector
                .units()
                .iter()
                .position(|unit| unit.id() == member.unit())
            else {
                return Err(AccountSelectionEligibilityError::UnknownMemberUnit);
            };
            if !mask_includes(unit_mask, unit_index) {
                return Err(AccountSelectionEligibilityError::MemberUnitNotAllowed);
            }
        }

        for (unit_index, unit) in selector.units().iter().enumerate() {
            if !mask_includes(unit_mask, unit_index) {
                continue;
            }
            let has_allowed_member = selector.members().iter().enumerate().any(|(member_index, member)| {
                mask_includes(member_mask, member_index) && member.unit() == unit.id()
            });
            if !has_allowed_member {
                return Err(AccountSelectionEligibilityError::AllowedUnitWithoutMember);
            }
        }

        Ok(Self {
            request,
            unit_mask,
            member_mask,
        })
    }

    /// 返回资格绑定的原始选择请求。
    pub(crate) const fn request(self) -> AccountSelectionRequest<'snapshot, 'config> {
        self.request
    }

    /// 返回按 Selector 声明顺序编码的允许 Unit 位图。
    pub(crate) const fn unit_mask(self) -> u16 {
        self.unit_mask
    }

    /// 返回按 Selector 声明顺序编码的允许 Member 位图。
    pub(crate) const fn member_mask(self) -> u16 {
        self.member_mask
    }

    /// 返回某个已知 Unit 声明位是否获准；未知下标返回 `None`。
    pub(crate) fn unit_allowed_at(&self, index: u8) -> Option<bool> {
        self.request
            .resolved()
            .selector()
            .units()
            .get(usize::from(index))
            .map(|_| mask_includes(self.unit_mask, usize::from(index)))
    }

    /// 返回某个已知 Member 声明位是否获准；未知下标返回 `None`。
    pub(crate) fn member_allowed_at(&self, index: u8) -> Option<bool> {
        self.request
            .resolved()
            .selector()
            .members()
            .get(usize::from(index))
            .map(|_| mask_includes(self.member_mask, usize::from(index)))
    }
}

#[cfg_attr(not(test), allow(dead_code))]
const fn known_mask(count: usize) -> u16 {
    if count >= u16::BITS as usize {
        u16::MAX
    } else {
        (1u16 << count) - 1
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn mask_includes(mask: u16, index: usize) -> bool {
    index < u16::BITS as usize && mask & (1u16 << index) != 0
}
