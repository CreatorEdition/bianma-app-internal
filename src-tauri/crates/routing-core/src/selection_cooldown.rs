//! Account、Credential 与额度组的有界动态冷却投影。
//!
//! 本模块只保存进程内、单所有者的固定容量冷却状态，并将其保守投影到既有账户选择
//! eligibility。它不解析 HTTP、Retry-After、错误文本或请求头，也不创建重试、网络、
//! 线程、任务、队列或持久化状态。

use super::{
    selection_input::{AccountSelectionEligibility, AccountSelectionEligibilityError},
    selection_lease::{ResourceCooldownScope, TrustedSelectionCooldownObservation},
    HealthTick, SnapshotVersion, MAX_ACCOUNT_SELECTOR_MEMBERS, MAX_TRACKED_ACCOUNTS,
    MAX_TRACKED_CREDENTIALS, MAX_TRACKED_QUOTA_GROUPS,
};

/// 消费受信资源冷却观测时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionCooldownRecordError {
    /// 受信 token 的固定额度组列表形状无效；理论不变量失效时拒绝写入。
    InvalidQuotaGroups,
    /// 观测来自已被更高配置代替代的旧快照，不能回写资源冷却。
    StaleGeneration,
}

/// 将冷却状态投影为选择 eligibility 时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SelectionCooldownFilterError {
    /// 当前选择请求来自已被更高配置代替代的旧快照，必须停止选择。
    StaleGeneration,
    /// Selector 中的 Member 找不到其静态 Unit；理论不变量失效时拒绝选择。
    UnknownMemberUnit,
    /// 重新构造 eligibility 时未能满足既有 C0 位图不变量。
    Eligibility(AccountSelectionEligibilityError),
}

/// 一个按稳定资源 ID 升序排列的冷却槽。
///
/// `id == 0` 仅表示未使用槽；真实 Account、Credential 与 QuotaGroup 标识均由静态合同
/// 保证为非零。保持裸 `u64` 让固定表不需要 `Option` 判别式，减少本地常驻内存。
#[derive(Clone, Copy)]
struct CooldownEntry {
    id: u64,
    until: HealthTick,
}

impl CooldownEntry {
    const EMPTY: Self = Self {
        id: 0,
        until: HealthTick::new(0),
    };
}

/// 稠密有序的固定容量冷却表。
///
/// 查询使用二分查找。过期项不影响读路径的 `until > now` 判定，只会在下一次同表写入时
/// 压缩回收，因此每个正常选择请求不需要扫描整张表。
struct CooldownTable<const N: usize> {
    entries: [CooldownEntry; N],
    len: u16,
}

impl<const N: usize> CooldownTable<N> {
    const fn new() -> Self {
        Self {
            entries: [CooldownEntry::EMPTY; N],
            len: 0,
        }
    }

    fn is_active(&self, id: u64, now: HealthTick) -> bool {
        self.find(id)
            .ok()
            .is_some_and(|index| self.entries[index].until > now)
    }

    /// 写入同 ID 的较大 deadline，或插入一个新 ID。
    ///
    /// 返回 `true` 表示没有可用槽，调用方必须触发全局保守 overflow。
    fn record(&mut self, id: u64, until: HealthTick, now: HealthTick) -> bool {
        self.prune_expired(now);
        match self.find(id) {
            Ok(index) => {
                if until > self.entries[index].until {
                    self.entries[index].until = until;
                }
                false
            }
            Err(index) if usize::from(self.len) < N => {
                let len = usize::from(self.len);
                self.entries.copy_within(index..len, index + 1);
                self.entries[index] = CooldownEntry { id, until };
                self.len += 1;
                false
            }
            Err(_) => true,
        }
    }

    fn find(&self, id: u64) -> Result<usize, usize> {
        let mut low = 0usize;
        let mut high = usize::from(self.len);
        while low < high {
            let middle = low + (high - low) / 2;
            match self.entries[middle].id.cmp(&id) {
                core::cmp::Ordering::Less => low = middle + 1,
                core::cmp::Ordering::Greater => high = middle,
                core::cmp::Ordering::Equal => return Ok(middle),
            }
        }
        Err(low)
    }

    fn prune_expired(&mut self, now: HealthTick) {
        let len = usize::from(self.len);
        let mut next = 0usize;
        for index in 0..len {
            let entry = self.entries[index];
            if entry.until > now {
                self.entries[next] = entry;
                next += 1;
            }
        }
        for entry in &mut self.entries[next..len] {
            *entry = CooldownEntry::EMPTY;
        }
        self.len = next as u16;
    }
}

/// 进程内、单所有者、固定容量的资源冷却 Registry。
///
/// Registry 不借用任何 Lease、Attempt、Snapshot 或静态布局；它只在一个单调的快照版本
/// 内按稳定 Account、Credential 与 QuotaGroup ID 保留状态。更高版本首次进入时会原子清空
/// 全部旧冷却；低版本的迟到观测或选择请求一律拒绝，不能回滚当前代。宿主必须在资源实际
/// 语义变更（包括 ID 重绑定）时递增快照版本，且不能以旧版本描述新绑定。
///
/// 任意资源表在仍有有效冷却时耗尽槽位，会拉高全局 overflow 冷却。overflow 活跃期间所有
/// 账户选择都输出合法空 eligibility，而不是静默遗漏某个资源冷却。
pub(crate) struct SelectionCooldownRegistry {
    last_tick: HealthTick,
    generation: Option<SnapshotVersion>,
    accounts: CooldownTable<MAX_TRACKED_ACCOUNTS>,
    credentials: CooldownTable<MAX_TRACKED_CREDENTIALS>,
    quota_groups: CooldownTable<MAX_TRACKED_QUOTA_GROUPS>,
    overflow_until: HealthTick,
}

impl SelectionCooldownRegistry {
    /// 创建没有任何资源冷却的 Registry。
    pub(crate) const fn new() -> Self {
        Self {
            last_tick: HealthTick::new(0),
            generation: None,
            accounts: CooldownTable::new(),
            credentials: CooldownTable::new(),
            quota_groups: CooldownTable::new(),
            overflow_until: HealthTick::new(0),
        }
    }

    /// 消费由实际 handoff 资源上报器签发的一次性冷却观测。
    ///
    /// 该操作只更新后续请求的选择资格，绝不创建 AttemptCompletion、重放收据或
    /// CoordinatorStep，也不能改变当前 Attempt 的交付状态。
    pub(crate) fn record(
        &mut self,
        observation: TrustedSelectionCooldownObservation,
        now: HealthTick,
    ) -> Result<(), SelectionCooldownRecordError> {
        let now = self.observe_now(now);
        let (generation, target, account, credential, quota_groups, quota_group_len, scope, until) =
            observation.into_parts();
        if !self.activate_generation(generation) {
            return Err(SelectionCooldownRecordError::StaleGeneration);
        }
        if self.overflow_until <= now {
            self.overflow_until = HealthTick::new(0);
        }
        if target.get() == 0
            || account == 0
            || credential == 0
            || usize::from(quota_group_len) > MAX_TRACKED_QUOTA_GROUPS.min(quota_groups.len())
            || quota_groups[..usize::from(quota_group_len)].contains(&0)
            || (matches!(
                scope,
                ResourceCooldownScope::CurrentQuotaGroups
                    | ResourceCooldownScope::CurrentCredentialAndQuotaGroups
            ) && quota_group_len == 0)
        {
            return Err(SelectionCooldownRecordError::InvalidQuotaGroups);
        }
        if until <= now {
            return Ok(());
        }

        match scope {
            ResourceCooldownScope::Credential => {
                self.record_entry(ResourceTable::Credential, credential, until, now);
            }
            ResourceCooldownScope::Account => {
                self.record_entry(ResourceTable::Account, account, until, now);
            }
            ResourceCooldownScope::CurrentQuotaGroups => {
                for id in &quota_groups[..usize::from(quota_group_len)] {
                    self.record_entry(ResourceTable::QuotaGroup, *id, until, now);
                }
            }
            ResourceCooldownScope::CurrentCredentialAndQuotaGroups => {
                self.record_entry(ResourceTable::Credential, credential, until, now);
                for id in &quota_groups[..usize::from(quota_group_len)] {
                    self.record_entry(ResourceTable::QuotaGroup, *id, until, now);
                }
            }
        }
        Ok(())
    }

    /// 将当前有效冷却与已有 eligibility 取交集。
    ///
    /// 只清除输入已允许的 Member 位，随后由 C0 helper 重新推导 Unit 位并完整验证。任意
    /// 静态 Unit/Member 不变量异常都会返回错误而不是伪造一个可发送 eligibility。
    pub(crate) fn filter<'snapshot, 'config>(
        &mut self,
        eligibility: AccountSelectionEligibility<'snapshot, 'config>,
        now: HealthTick,
    ) -> Result<AccountSelectionEligibility<'snapshot, 'config>, SelectionCooldownFilterError> {
        let now = self.observe_now(now);
        let generation = eligibility.request().resolved().snapshot_version();
        if !self.activate_generation(generation) {
            return Err(SelectionCooldownFilterError::StaleGeneration);
        }
        if self.overflow_until > now {
            return eligibility
                .with_member_mask_subset(0)
                .map_err(SelectionCooldownFilterError::Eligibility);
        }

        let selector = eligibility.request().resolved().selector();
        let mut member_units = [0u8; MAX_ACCOUNT_SELECTOR_MEMBERS];
        for (member_index, member) in selector.members().iter().enumerate() {
            let Some(unit_index) = selector
                .units()
                .iter()
                .position(|unit| unit.id() == member.unit())
            else {
                return Err(SelectionCooldownFilterError::UnknownMemberUnit);
            };
            member_units[member_index] = unit_index as u8;
        }

        let mut quota_blocked_units = 0u16;
        for (unit_index, unit) in selector.units().iter().enumerate() {
            if !mask_includes(eligibility.unit_mask(), unit_index) {
                continue;
            }
            if unit
                .quota_groups()
                .iter()
                .any(|id| self.quota_groups.is_active(id.get(), now))
            {
                quota_blocked_units |= 1u16 << unit_index;
            }
        }

        let mut remaining_members = eligibility.member_mask();
        for (member_index, member) in selector.members().iter().enumerate() {
            if !mask_includes(remaining_members, member_index) {
                continue;
            }
            let unit_index = usize::from(member_units[member_index]);
            if mask_includes(quota_blocked_units, unit_index)
                || self.accounts.is_active(member.account().get(), now)
                || self.credentials.is_active(member.credential().get(), now)
            {
                remaining_members &= !(1u16 << member_index);
            }
        }

        eligibility
            .with_member_mask_subset(remaining_members)
            .map_err(SelectionCooldownFilterError::Eligibility)
    }

    fn observe_now(&mut self, now: HealthTick) -> HealthTick {
        if now > self.last_tick {
            self.last_tick = now;
        }
        self.last_tick
    }

    /// 激活更高的不可变配置代，并拒绝任何试图回滚到较低代的输入。
    ///
    /// Registry 只保存固定表而不保存快照引用；版本转换时清空所有表可避免同一裸 ID 被新
    /// 配置重新绑定后继承旧资源冷却。相同版本的不同快照仍可共享状态，但宿主必须保证该
    /// 版本描述同一资源身份图。
    fn activate_generation(&mut self, generation: SnapshotVersion) -> bool {
        match self.generation {
            None => {
                self.generation = Some(generation);
                true
            }
            Some(active) if generation == active => true,
            Some(active) if generation > active => {
                self.generation = Some(generation);
                self.accounts = CooldownTable::new();
                self.credentials = CooldownTable::new();
                self.quota_groups = CooldownTable::new();
                self.overflow_until = HealthTick::new(0);
                true
            }
            Some(_) => false,
        }
    }

    fn record_entry(&mut self, table: ResourceTable, id: u64, until: HealthTick, now: HealthTick) {
        let full = match table {
            ResourceTable::Account => self.accounts.record(id, until, now),
            ResourceTable::Credential => self.credentials.record(id, until, now),
            ResourceTable::QuotaGroup => self.quota_groups.record(id, until, now),
        };
        if full && until > self.overflow_until {
            self.overflow_until = until;
        }
    }
}

impl Default for SelectionCooldownRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
enum ResourceTable {
    Account,
    Credential,
    QuotaGroup,
}

fn mask_includes(mask: u16, index: usize) -> bool {
    index < u16::BITS as usize && mask & (1u16 << index) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_table_uses_maximum_deadline_exact_expiry_and_recovers_slots_on_write() {
        let mut table = CooldownTable::<2>::new();
        assert!(!table.record(1, HealthTick::new(20), HealthTick::new(10)));
        assert!(!table.record(1, HealthTick::new(15), HealthTick::new(10)));
        assert!(table.is_active(1, HealthTick::new(15)));
        assert!(!table.is_active(1, HealthTick::new(20)));
        assert!(!table.record(2, HealthTick::new(30), HealthTick::new(20)));
        assert!(!table.record(3, HealthTick::new(30), HealthTick::new(20)));
        assert!(table.is_active(2, HealthTick::new(20)));
        assert!(table.is_active(3, HealthTick::new(20)));
    }

    #[test]
    fn full_cooldown_table_reports_overflow() {
        let mut table = CooldownTable::<2>::new();
        assert!(!table.record(1, HealthTick::new(10), HealthTick::new(1)));
        assert!(!table.record(2, HealthTick::new(10), HealthTick::new(1)));
        assert!(table.record(3, HealthTick::new(20), HealthTick::new(1)));
    }

    #[test]
    fn every_resource_table_escalates_full_capacity_to_global_overflow() {
        fn fill_then_overflow(
            registry: &mut SelectionCooldownRegistry,
            table: ResourceTable,
            capacity: usize,
        ) {
            for id in 1..=capacity as u64 {
                registry.record_entry(table, id, HealthTick::new(10), HealthTick::new(1));
            }
            assert_eq!(registry.overflow_until, HealthTick::new(0));
            registry.record_entry(
                table,
                capacity as u64 + 1,
                HealthTick::new(20),
                HealthTick::new(1),
            );
            assert_eq!(registry.overflow_until, HealthTick::new(20));
        }

        fill_then_overflow(
            &mut SelectionCooldownRegistry::new(),
            ResourceTable::Account,
            MAX_TRACKED_ACCOUNTS,
        );
        fill_then_overflow(
            &mut SelectionCooldownRegistry::new(),
            ResourceTable::Credential,
            MAX_TRACKED_CREDENTIALS,
        );
        fill_then_overflow(
            &mut SelectionCooldownRegistry::new(),
            ResourceTable::QuotaGroup,
            MAX_TRACKED_QUOTA_GROUPS,
        );
    }

    #[test]
    fn newer_generation_clears_all_resource_tables_and_rejects_rollback() {
        let mut registry = SelectionCooldownRegistry::new();
        assert!(registry.activate_generation(SnapshotVersion::new(1).expect("测试首代版本非零")));
        registry.record_entry(
            ResourceTable::Account,
            1,
            HealthTick::new(20),
            HealthTick::new(1),
        );
        registry.record_entry(
            ResourceTable::Credential,
            2,
            HealthTick::new(20),
            HealthTick::new(1),
        );
        registry.record_entry(
            ResourceTable::QuotaGroup,
            3,
            HealthTick::new(20),
            HealthTick::new(1),
        );
        registry.overflow_until = HealthTick::new(20);

        assert!(registry.activate_generation(SnapshotVersion::new(2).expect("测试新代版本非零")));
        assert!(!registry.accounts.is_active(1, HealthTick::new(1)));
        assert!(!registry.credentials.is_active(2, HealthTick::new(1)));
        assert!(!registry.quota_groups.is_active(3, HealthTick::new(1)));
        assert_eq!(registry.overflow_until, HealthTick::new(0));
        assert!(!registry.activate_generation(SnapshotVersion::new(1).expect("测试旧代版本非零")));
    }

    #[test]
    fn registry_state_stays_within_local_memory_budget() {
        assert!(core::mem::size_of::<SelectionCooldownRegistry>() <= 5 * 1024);
    }
}
