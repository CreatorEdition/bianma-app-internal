//! 多账户、多凭据与共享额度的静态选择合同。
//!
//! 本模块只验证不可变 Selector 定义，不选择账户、不维护健康或 Lease，也不解析 Secret。

use core::num::NonZeroU16;

use super::{
    AccountId, AccountSelectorId, CredentialId, QuotaGroupId, QuotaSelectionUnitId,
    MAX_ROUTE_TARGETS,
};

/// 单个账户选择合同允许的最大成员数。
pub const MAX_ACCOUNT_SELECTOR_MEMBERS: usize = MAX_ROUTE_TARGETS;

/// 单个账户选择合同允许的最大独立额度单元数。
pub const MAX_QUOTA_SELECTION_UNITS: usize = MAX_ROUTE_TARGETS;

/// 一个编译路由快照允许引用的最大账户选择合同数。
pub const MAX_ACCOUNT_SELECTORS: usize = MAX_ROUTE_TARGETS;

/// 单个独立额度单元允许关联的最大额度组数。
pub const MAX_QUOTA_GROUPS_PER_UNIT: usize = MAX_ROUTE_TARGETS;

/// 同一 Selector 内账户/凭据的静态选择策略。
///
/// 本枚举只描述未来运行时应采用的策略；本模块不执行选择。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSelectionPolicy {
    /// 在显式优先层内按故障转移顺序选择。
    PriorityFailover,
    /// 在独立额度单元间按加权在途负载选择。
    WeightedLeastInflight,
    /// 明确关闭会话粘性的兼容轮询模式。
    RoundRobinCompat,
}

/// 额度拓扑拆分成独立单元的可信来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaTopologySource {
    /// 未获得可信拓扑；全部成员必须保守地归入同一额度单元。
    ConservativeDefault,
    /// 用户明确确认独立额度关系。
    UserConfirmed,
    /// 受信 Adapter 提供稳定且不含 Secret 的额度身份。
    AdapterVerified,
}

/// 一个独立额度单元及其唯一有效权重。
///
/// 多个账户或凭据可关联到同一单元，但权重只存在于本类型，禁止随成员数累加。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QuotaSelectionUnit<'a> {
    id: QuotaSelectionUnitId,
    effective_weight: NonZeroU16,
    quota_groups: &'a [QuotaGroupId],
}

impl<'a> QuotaSelectionUnit<'a> {
    /// 创建一个由 SelectorDefinition 后续验证的额度单元。
    pub const fn new(
        id: QuotaSelectionUnitId,
        effective_weight: NonZeroU16,
        quota_groups: &'a [QuotaGroupId],
    ) -> Self {
        Self {
            id,
            effective_weight,
            quota_groups,
        }
    }

    /// 返回稳定额度单元标识。
    pub const fn id(self) -> QuotaSelectionUnitId {
        self.id
    }

    /// 返回该额度单元唯一的有效权重。
    pub const fn effective_weight(self) -> NonZeroU16 {
        self.effective_weight
    }

    /// 返回关联的全部额度组。
    pub const fn quota_groups(&self) -> &'a [QuotaGroupId] {
        self.quota_groups
    }
}

/// 一个可被账户选择合同引用的账户/凭据成员。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountSelectorMember {
    account: AccountId,
    credential: CredentialId,
    unit: QuotaSelectionUnitId,
    priority_tier: u8,
}

impl AccountSelectorMember {
    /// 创建账户/凭据与独立额度单元的静态关联。
    pub const fn new(
        account: AccountId,
        credential: CredentialId,
        unit: QuotaSelectionUnitId,
        priority_tier: u8,
    ) -> Self {
        Self {
            account,
            credential,
            unit,
            priority_tier,
        }
    }

    /// 返回账户标识。
    pub const fn account(self) -> AccountId {
        self.account
    }

    /// 返回凭据标识。
    pub const fn credential(self) -> CredentialId {
        self.credential
    }

    /// 返回所属独立额度单元。
    pub const fn unit(self) -> QuotaSelectionUnitId {
        self.unit
    }

    /// 返回显式优先层；数值较小的层优先。
    pub const fn priority_tier(self) -> u8 {
        self.priority_tier
    }
}

/// 构造账户选择合同时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountSelectorError {
    /// 没有任何账户/凭据成员。
    EmptyMembers,
    /// 成员超过固定上限。
    TooManyMembers,
    /// 没有任何独立额度单元。
    EmptyUnits,
    /// 独立额度单元超过固定上限。
    TooManyUnits,
    /// 独立额度单元标识重复。
    DuplicateUnit,
    /// 独立额度单元没有关联额度组。
    EmptyQuotaGroups,
    /// 独立额度单元关联额度组超过固定上限。
    TooManyQuotaGroups,
    /// 同一独立额度单元重复关联同一额度组。
    DuplicateQuotaGroup,
    /// 同一额度组被不安全地拆入多个独立额度单元。
    QuotaGroupInMultipleUnits,
    /// 成员引用了不存在的独立额度单元。
    UnknownUnit,
    /// 同一凭据在一个 Selector 内重复出现。
    DuplicateCredential,
    /// 已定义的独立额度单元没有任何成员。
    UnusedUnit,
    /// 未知额度拓扑被不安全地拆分为多个独立单元。
    ConservativeTopologySplit,
}

/// 不可变、无分配的多账户选择合同。
///
/// 定义只借用已编译成员与额度单元；未来运行时必须先选择 Unit，再在 Unit 内选择
/// Account/Credential，不能按 Key 数量累加权重。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountSelectorDefinition<'a> {
    id: AccountSelectorId,
    policy: CredentialSelectionPolicy,
    topology_source: QuotaTopologySource,
    units: &'a [QuotaSelectionUnit<'a>],
    members: &'a [AccountSelectorMember],
}

impl<'a> AccountSelectorDefinition<'a> {
    /// 验证并创建账户选择合同。
    pub fn new(
        id: AccountSelectorId,
        policy: CredentialSelectionPolicy,
        topology_source: QuotaTopologySource,
        units: &'a [QuotaSelectionUnit<'a>],
        members: &'a [AccountSelectorMember],
    ) -> Result<Self, AccountSelectorError> {
        if members.is_empty() {
            return Err(AccountSelectorError::EmptyMembers);
        }
        if members.len() > MAX_ACCOUNT_SELECTOR_MEMBERS {
            return Err(AccountSelectorError::TooManyMembers);
        }
        if units.is_empty() {
            return Err(AccountSelectorError::EmptyUnits);
        }
        if units.len() > MAX_QUOTA_SELECTION_UNITS {
            return Err(AccountSelectorError::TooManyUnits);
        }
        if topology_source == QuotaTopologySource::ConservativeDefault && units.len() != 1 {
            return Err(AccountSelectorError::ConservativeTopologySplit);
        }

        for (index, unit) in units.iter().enumerate() {
            if units[..index].iter().any(|previous| previous.id == unit.id) {
                return Err(AccountSelectorError::DuplicateUnit);
            }
            if unit.quota_groups.is_empty() {
                return Err(AccountSelectorError::EmptyQuotaGroups);
            }
            if unit.quota_groups.len() > MAX_QUOTA_GROUPS_PER_UNIT {
                return Err(AccountSelectorError::TooManyQuotaGroups);
            }
            for (group_index, group) in unit.quota_groups.iter().enumerate() {
                if unit.quota_groups[..group_index]
                    .iter()
                    .any(|previous| previous == group)
                {
                    return Err(AccountSelectorError::DuplicateQuotaGroup);
                }
                if units[..index].iter().any(|previous_unit| {
                    previous_unit
                        .quota_groups
                        .iter()
                        .any(|previous_group| previous_group == group)
                }) {
                    return Err(AccountSelectorError::QuotaGroupInMultipleUnits);
                }
            }
        }

        for (index, member) in members.iter().enumerate() {
            if members[..index]
                .iter()
                .any(|previous| previous.credential == member.credential)
            {
                return Err(AccountSelectorError::DuplicateCredential);
            }
            if !units.iter().any(|unit| unit.id == member.unit) {
                return Err(AccountSelectorError::UnknownUnit);
            }
        }
        if units
            .iter()
            .any(|unit| !members.iter().any(|member| member.unit == unit.id))
        {
            return Err(AccountSelectorError::UnusedUnit);
        }

        Ok(Self {
            id,
            policy,
            topology_source,
            units,
            members,
        })
    }

    /// 返回稳定账户选择合同标识。
    pub const fn id(self) -> AccountSelectorId {
        self.id
    }

    /// 返回未来运行时应采用的选择策略。
    pub const fn policy(self) -> CredentialSelectionPolicy {
        self.policy
    }

    /// 返回额度拓扑的可信来源。
    pub const fn topology_source(self) -> QuotaTopologySource {
        self.topology_source
    }

    /// 返回独立额度单元。
    pub const fn units(&self) -> &'a [QuotaSelectionUnit<'a>] {
        self.units
    }

    /// 返回账户/凭据成员。
    pub const fn members(&self) -> &'a [AccountSelectorMember] {
        self.members
    }

    /// 根据稳定标识查找额度单元。
    pub fn unit(&self, id: QuotaSelectionUnitId) -> Option<QuotaSelectionUnit<'a>> {
        self.units.iter().copied().find(|unit| unit.id == id)
    }
}

/// 构造不可变账户选择合同目录时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountSelectorCatalogError {
    /// 目录没有任何账户选择合同。
    Empty,
    /// 目录超过固定上限。
    TooMany,
    /// 同一账户选择合同标识在目录中重复出现。
    DuplicateId,
}

/// 与一个已编译路由快照同代的账户选择合同目录。
///
/// 目录只借用经过验证的静态定义，使用有界线性查找；它不是全局注册表，也不承担
/// 账户选择、健康、额度或运行时状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountSelectorCatalog<'a> {
    selectors: &'a [AccountSelectorDefinition<'a>],
}

impl<'a> AccountSelectorCatalog<'a> {
    /// 验证并创建一个固定上限的账户选择合同目录。
    pub fn new(
        selectors: &'a [AccountSelectorDefinition<'a>],
    ) -> Result<Self, AccountSelectorCatalogError> {
        if selectors.is_empty() {
            return Err(AccountSelectorCatalogError::Empty);
        }
        if selectors.len() > MAX_ACCOUNT_SELECTORS {
            return Err(AccountSelectorCatalogError::TooMany);
        }
        for (index, selector) in selectors.iter().enumerate() {
            if selectors[..index]
                .iter()
                .any(|previous| previous.id == selector.id)
            {
                return Err(AccountSelectorCatalogError::DuplicateId);
            }
        }
        Ok(Self { selectors })
    }

    /// 返回目录中的账户选择合同数量。
    pub const fn len(&self) -> usize {
        self.selectors.len()
    }

    /// 返回目录是否为空；经由 [`Self::new`] 构造的目录始终返回 `false`。
    pub const fn is_empty(&self) -> bool {
        self.selectors.is_empty()
    }

    /// 根据稳定标识有界查找账户选择合同。
    pub(crate) fn get(&self, id: AccountSelectorId) -> Option<&'a AccountSelectorDefinition<'a>> {
        self.selectors.iter().find(|selector| selector.id == id)
    }

    pub(crate) fn contains(&self, id: AccountSelectorId) -> bool {
        self.get(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(value: u64) -> AccountId {
        AccountId::new(value).expect("测试账户 ID 非零")
    }

    fn credential(value: u64) -> CredentialId {
        CredentialId::new(value).expect("测试凭据 ID 非零")
    }

    fn group(value: u64) -> QuotaGroupId {
        QuotaGroupId::new(value).expect("测试额度组 ID 非零")
    }

    fn unit_id(value: u64) -> QuotaSelectionUnitId {
        QuotaSelectionUnitId::new(value).expect("测试额度单元 ID 非零")
    }

    fn selector(value: u64) -> AccountSelectorId {
        AccountSelectorId::new(value).expect("测试选择合同 ID 非零")
    }

    fn weight(value: u16) -> NonZeroU16 {
        NonZeroU16::new(value).expect("测试权重非零")
    }

    fn unit<'a>(
        id: u64,
        effective_weight: u16,
        groups: &'a [QuotaGroupId],
    ) -> QuotaSelectionUnit<'a> {
        QuotaSelectionUnit::new(unit_id(id), weight(effective_weight), groups)
    }

    fn member(account_value: u64, credential_value: u64, unit_value: u64) -> AccountSelectorMember {
        AccountSelectorMember::new(
            account(account_value),
            credential(credential_value),
            unit_id(unit_value),
            0,
        )
    }

    #[test]
    fn selector_rejects_empty_and_over_capacity() {
        let groups = [group(1)];
        let units = [unit(1, 1, &groups)];
        let members = [member(1, 1, 1)];
        let too_many_members = [member(1, 1, 1); MAX_ACCOUNT_SELECTOR_MEMBERS + 1];
        let too_many_units = [unit(1, 1, &groups); MAX_QUOTA_SELECTION_UNITS + 1];
        let too_many_groups = [
            group(1),
            group(2),
            group(3),
            group(4),
            group(5),
            group(6),
            group(7),
            group(8),
            group(9),
            group(10),
            group(11),
            group(12),
            group(13),
            group(14),
            group(15),
            group(16),
            group(17),
        ];
        let unit_with_too_many_groups = [unit(1, 1, &too_many_groups)];

        assert_eq!(
            AccountSelectorDefinition::new(
                selector(1),
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &units,
                &[],
            ),
            Err(AccountSelectorError::EmptyMembers)
        );
        assert_eq!(
            AccountSelectorDefinition::new(
                selector(1),
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &[],
                &members,
            ),
            Err(AccountSelectorError::EmptyUnits)
        );
        assert_eq!(
            AccountSelectorDefinition::new(
                selector(1),
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &units,
                &too_many_members,
            ),
            Err(AccountSelectorError::TooManyMembers)
        );
        assert_eq!(
            AccountSelectorDefinition::new(
                selector(1),
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::UserConfirmed,
                &too_many_units,
                &members,
            ),
            Err(AccountSelectorError::TooManyUnits)
        );
        assert_eq!(
            AccountSelectorDefinition::new(
                selector(1),
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &unit_with_too_many_groups,
                &members,
            ),
            Err(AccountSelectorError::TooManyQuotaGroups)
        );
    }

    #[test]
    fn selector_rejects_invalid_unit_and_member_relations() {
        let groups = [group(1)];
        let other_groups = [group(2)];
        let duplicate_groups = [group(1), group(1)];
        let units = [unit(1, 1, &groups)];
        let duplicate_units = [unit(1, 1, &groups), unit(1, 1, &groups)];
        let empty_groups = [unit(1, 1, &[])];
        let repeated_groups = [unit(1, 1, &duplicate_groups)];
        let unknown_unit = [member(1, 1, 2)];
        let duplicate_credential = [member(1, 1, 1), member(2, 1, 1)];
        let unused_unit = [unit(1, 1, &groups), unit(2, 1, &other_groups)];
        let repeated_across_units = [unit(1, 1, &groups), unit(2, 1, &groups)];
        let members_for_two_units = [member(1, 1, 1), member(2, 2, 2)];
        let members = [member(1, 1, 1)];

        for (units, members, expected) in [
            (
                &duplicate_units[..],
                &members[..],
                AccountSelectorError::DuplicateUnit,
            ),
            (
                &empty_groups[..],
                &members[..],
                AccountSelectorError::EmptyQuotaGroups,
            ),
            (
                &repeated_groups[..],
                &members[..],
                AccountSelectorError::DuplicateQuotaGroup,
            ),
            (
                &units[..],
                &unknown_unit[..],
                AccountSelectorError::UnknownUnit,
            ),
            (
                &units[..],
                &duplicate_credential[..],
                AccountSelectorError::DuplicateCredential,
            ),
            (
                &repeated_across_units[..],
                &members_for_two_units[..],
                AccountSelectorError::QuotaGroupInMultipleUnits,
            ),
            (
                &unused_unit[..],
                &members[..],
                AccountSelectorError::UnusedUnit,
            ),
        ] {
            assert_eq!(
                AccountSelectorDefinition::new(
                    selector(1),
                    CredentialSelectionPolicy::PriorityFailover,
                    QuotaTopologySource::UserConfirmed,
                    units,
                    members,
                ),
                Err(expected)
            );
        }
    }

    #[test]
    fn shared_quota_unit_owns_weight_once_regardless_of_member_count() {
        let groups = [group(1)];
        let units = [unit(10, 7, &groups)];
        let members = [member(1, 1, 10), member(1, 2, 10)];
        let definition = AccountSelectorDefinition::new(
            selector(1),
            CredentialSelectionPolicy::WeightedLeastInflight,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )
        .expect("共享额度单元定义有效");

        assert_eq!(definition.units().len(), 1);
        assert_eq!(definition.members().len(), 2);
        assert_eq!(
            definition
                .unit(unit_id(10))
                .expect("额度单元存在")
                .effective_weight()
                .get(),
            7
        );
    }

    #[test]
    fn topology_split_requires_trusted_source_and_allows_multi_group_unit() {
        let groups_one = [group(1), group(2)];
        let groups_two = [group(3)];
        let units = [unit(10, 1, &groups_one), unit(20, 2, &groups_two)];
        let members = [member(1, 1, 10), member(2, 2, 20)];

        assert_eq!(
            AccountSelectorDefinition::new(
                selector(1),
                CredentialSelectionPolicy::PriorityFailover,
                QuotaTopologySource::ConservativeDefault,
                &units,
                &members,
            ),
            Err(AccountSelectorError::ConservativeTopologySplit)
        );
        let definition = AccountSelectorDefinition::new(
            selector(1),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::UserConfirmed,
            &units,
            &members,
        )
        .expect("用户确认后允许拓扑拆分");

        assert_eq!(definition.units().len(), 2);
        assert_eq!(
            definition
                .unit(unit_id(10))
                .expect("第一个单元存在")
                .quota_groups(),
            &groups_one
        );
    }

    #[test]
    fn selector_definition_stays_borrowed_and_small() {
        assert!(core::mem::size_of::<AccountSelectorDefinition<'_>>() <= 56);
        assert!(core::mem::size_of::<QuotaSelectionUnit<'_>>() <= 32);
        assert!(core::mem::size_of::<AccountSelectorMember>() <= 32);
    }

    #[test]
    fn catalog_rejects_empty_over_capacity_and_duplicate_ids() {
        let groups = [group(1)];
        let units = [unit(1, 1, &groups)];
        let members = [member(1, 1, 1)];
        let definition = AccountSelectorDefinition::new(
            selector(1),
            CredentialSelectionPolicy::PriorityFailover,
            QuotaTopologySource::ConservativeDefault,
            &units,
            &members,
        )
        .expect("测试选择合同有效");
        let duplicate = [definition, definition];
        let too_many = [definition; MAX_ACCOUNT_SELECTORS + 1];
        let single = [definition];

        assert_eq!(
            AccountSelectorCatalog::new(&[]),
            Err(AccountSelectorCatalogError::Empty)
        );
        assert_eq!(
            AccountSelectorCatalog::new(&too_many),
            Err(AccountSelectorCatalogError::TooMany)
        );
        assert_eq!(
            AccountSelectorCatalog::new(&duplicate),
            Err(AccountSelectorCatalogError::DuplicateId)
        );

        let catalog = AccountSelectorCatalog::new(&single).expect("目录有效");
        assert_eq!(catalog.len(), 1);
        assert!(!catalog.is_empty());
        assert_eq!(
            catalog.get(selector(1)).map(|item| item.id()),
            Some(selector(1))
        );
        assert_eq!(catalog.get(selector(2)), None);
    }
}
