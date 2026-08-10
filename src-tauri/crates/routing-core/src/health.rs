//! Site 与 ModelDeployment 的最小动态冷却合同。
//!
//! 本模块只保存进程内、单所有者的有界冷却状态，并为同代路由快照生成不可伪造的
//! eligibility 视图。它不解析 HTTP、Retry-After、错误正文或请求头，也不处理
//! Account、Credential、Quota、Secret、健康探测、线程、锁、数据库或网络。

use super::{
    attempt::TrustedRateLimitObservation, ModelDeploymentId, RoutePlan, RouteTarget,
    RoutingSnapshot, SiteId, MAX_ROUTE_TARGETS,
};

/// Registry 为 Site 维护的固定冷却槽数量。
pub const MAX_HEALTH_SITES: usize = MAX_ROUTE_TARGETS;
/// Registry 为 ModelDeployment 维护的固定冷却槽数量。
pub const MAX_HEALTH_DEPLOYMENTS: usize = MAX_ROUTE_TARGETS;

/// 由调用方提供的单调时间刻度。
///
/// 该值不绑定墙上时钟单位。Registry 会记住已观察到的最大值，任何回拨都被钳制，
/// 因此回拨不能提前解除既有冷却。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HealthTick(u64);

impl HealthTick {
    const ZERO: Self = Self(0);

    /// 从调用方定义的刻度值构造时间点。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 返回原始刻度值。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 一次冷却应归属的最小范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitScope {
    /// 仅冷却当前已解析 Target 的 ModelDeployment。
    Deployment,
    /// 冷却当前已解析 Target 所属的整个 Site。
    Site,
    /// 来源无法安全归类时，保守按当前已解析 Target 的 Site 冷却。
    Unknown,
}

#[derive(Clone, Copy)]
struct CooldownEntry<Id> {
    id: Id,
    until: HealthTick,
}

/// 进程内、单所有者、固定容量的动态冷却状态。
///
/// 一个 Registry 由宿主串行拥有；它既不加锁也不启动后台工作。若某类槽位已满且仍
/// 全部未过期，新的命中会延长全局 overflow 冷却，而不是静默丢失。
pub struct HealthRegistry {
    last_tick: HealthTick,
    site_cooldowns: [Option<CooldownEntry<SiteId>>; MAX_HEALTH_SITES],
    deployment_cooldowns: [Option<CooldownEntry<ModelDeploymentId>>; MAX_HEALTH_DEPLOYMENTS],
    overflow_until: HealthTick,
}

impl HealthRegistry {
    /// 创建不含任何冷却状态的 Registry。
    pub const fn new() -> Self {
        Self {
            last_tick: HealthTick::ZERO,
            site_cooldowns: [None; MAX_HEALTH_SITES],
            deployment_cooldowns: [None; MAX_HEALTH_DEPLOYMENTS],
            overflow_until: HealthTick::ZERO,
        }
    }

    /// 消费一次受信限流观测并记录冷却。
    ///
    /// 该 crate-private 入口只接受 Reporter 生成的一次性 Observation；它不解析 HTTP，
    /// 也不会改变 Attempt 的交付状态或重放资格。
    pub(crate) fn record_rate_limit(
        &mut self,
        observation: TrustedRateLimitObservation,
        now: HealthTick,
    ) {
        let (target, scope, deadline) = observation.into_parts();
        self.record_target_cooldown(target, scope, deadline, now);
    }

    /// 为同代路由快照生成动态 eligibility 视图。
    ///
    /// 视图绑定完整候选身份、候选顺序和快照版本；被用于其他快照或身份改变的快照时，
    /// Planner 与 Coordinator 都会 fail closed。
    pub fn eligibility_for<'snapshot, 'candidates>(
        &mut self,
        snapshot: &'snapshot RoutingSnapshot<'candidates>,
        now: HealthTick,
    ) -> RouteEligibility<'snapshot, 'candidates> {
        let now = self.observe_now(now);
        self.prune_expired(now);
        let mut allowed_mask = 0u16;
        let overflow_active = self.overflow_until > now;

        for (index, candidate) in snapshot.candidates.iter().enumerate() {
            let target = candidate.target();
            if !overflow_active
                && !Self::is_active(&self.site_cooldowns, target.site(), now)
                && !Self::is_active(&self.deployment_cooldowns, target.deployment(), now)
            {
                allowed_mask |= 1u16 << index;
            }
        }

        RouteEligibility {
            snapshot,
            allowed_mask,
        }
    }

    fn record_target_cooldown(
        &mut self,
        target: RouteTarget,
        scope: RateLimitScope,
        until: HealthTick,
        now: HealthTick,
    ) {
        let now = self.observe_now(now);
        self.prune_expired(now);
        if until <= now {
            return;
        }
        match scope {
            RateLimitScope::Deployment => Self::record_entry(
                &mut self.deployment_cooldowns,
                &mut self.overflow_until,
                target.deployment(),
                until,
            ),
            RateLimitScope::Site | RateLimitScope::Unknown => Self::record_entry(
                &mut self.site_cooldowns,
                &mut self.overflow_until,
                target.site(),
                until,
            ),
        }
    }

    fn observe_now(&mut self, now: HealthTick) -> HealthTick {
        if now > self.last_tick {
            self.last_tick = now;
        }
        self.last_tick
    }

    fn prune_expired(&mut self, now: HealthTick) {
        Self::prune_entries(&mut self.site_cooldowns, now);
        Self::prune_entries(&mut self.deployment_cooldowns, now);
        if self.overflow_until <= now {
            self.overflow_until = HealthTick::ZERO;
        }
    }

    fn prune_entries<Id>(entries: &mut [Option<CooldownEntry<Id>>], now: HealthTick) {
        for entry in entries {
            if entry.as_ref().is_some_and(|item| item.until <= now) {
                *entry = None;
            }
        }
    }

    fn record_entry<Id: Copy + Eq>(
        entries: &mut [Option<CooldownEntry<Id>>],
        overflow_until: &mut HealthTick,
        id: Id,
        until: HealthTick,
    ) {
        if let Some(existing) = entries.iter_mut().flatten().find(|entry| entry.id == id) {
            if until > existing.until {
                existing.until = until;
            }
            return;
        }
        if let Some(slot) = entries.iter_mut().find(|entry| entry.is_none()) {
            *slot = Some(CooldownEntry { id, until });
            return;
        }
        if until > *overflow_until {
            *overflow_until = until;
        }
    }

    fn is_active<Id: Copy + Eq>(
        entries: &[Option<CooldownEntry<Id>>],
        id: Id,
        now: HealthTick,
    ) -> bool {
        entries
            .iter()
            .flatten()
            .any(|entry| entry.id == id && entry.until > now)
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 与一个特定 RoutingSnapshot 借用绑定、不可由调用方伪造的动态允许位图。
///
/// 该值只表达 Site/ModelDeployment 冷却结果，不选择 Account/Credential，也不会把
/// 静态 Disabled 或 CoolingDown 目标重新启用。
#[derive(Clone, Copy)]
pub struct RouteEligibility<'snapshot, 'candidates> {
    snapshot: &'snapshot RoutingSnapshot<'candidates>,
    allowed_mask: u16,
}

impl RouteEligibility<'_, '_> {
    pub(crate) fn matches_snapshot(&self, snapshot: &RoutingSnapshot<'_>) -> bool {
        core::ptr::eq(self.snapshot, snapshot)
    }

    pub(crate) const fn allows_index(&self, index: usize) -> bool {
        index < self.snapshot.candidates.len() && (self.allowed_mask & (1u16 << index)) != 0
    }

    pub(crate) fn supports_plan(&self, plan: &RoutePlan<'_, '_>) -> bool {
        core::ptr::eq(self.snapshot, plan.snapshot)
    }

    pub(crate) fn allows_plan_target(&self, target: super::RouteTargetId) -> bool {
        self.snapshot
            .candidates
            .iter()
            .enumerate()
            .find_map(|(index, candidate)| {
                (candidate.target().id() == target).then_some(self.allows_index(index))
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{
        super::attempt::test_rate_limit_observation, super::EndpointId, super::IngressClassifier,
        super::IngressRequest, super::ModelDeploymentId, super::OperationId, super::RouteCandidate,
        super::RoutePlanner, super::RouteStageId, super::SnapshotVersion,
        super::VerifiedIngressDisposition,
    };

    fn site(value: u64) -> SiteId {
        SiteId::new(value).expect("测试站点 ID 非零")
    }

    fn deployment(value: u64) -> ModelDeploymentId {
        ModelDeploymentId::new(value).expect("测试部署 ID 非零")
    }

    fn target(value: u64, site_value: u64, deployment_value: u64) -> super::super::RouteTarget {
        super::super::RouteTarget::new(
            super::super::RouteTargetId::new(value).expect("测试目标 ID 非零"),
            site(site_value),
            deployment(deployment_value),
            EndpointId::new(value).expect("测试端点 ID 非零"),
            super::super::AccountSelectorId::new(1).expect("测试选择合同 ID 非零"),
        )
    }

    fn candidate(
        stage: u64,
        target_value: u64,
        site_value: u64,
        deployment_value: u64,
    ) -> RouteCandidate {
        RouteCandidate::ready(
            RouteStageId::new(stage).expect("测试阶段 ID 非零"),
            target(target_value, site_value, deployment_value),
            0,
        )
    }

    fn snapshot<'a>(candidates: &'a [RouteCandidate]) -> RoutingSnapshot<'a> {
        RoutingSnapshot::new(
            SnapshotVersion::new(1).expect("测试快照版本非零"),
            candidates,
            super::super::RoutingStrategy::Priority,
            candidates.len() as u8,
        )
        .expect("测试快照有效")
    }

    fn routed(snapshot: &RoutingSnapshot<'_>) -> super::super::VerifiedRouteDispatch {
        let disposition = IngressClassifier::new()
            .classify(IngressRequest::routed(
                OperationId::CONVERSATION,
                snapshot.version(),
            ))
            .expect("测试路由请求有效");
        let VerifiedIngressDisposition::Routed(request) = disposition else {
            panic!("会话操作必须产生 Routed 分发");
        };
        request
    }

    fn record_test_rate_limit(
        registry: &mut HealthRegistry,
        target: RouteTarget,
        scope: RateLimitScope,
        deadline: HealthTick,
        now: HealthTick,
    ) {
        let observation = test_rate_limit_observation(target, scope, deadline);
        registry.record_rate_limit(observation, now);
    }

    #[test]
    fn deployment_cooldown_only_blocks_matching_deployment() {
        let candidates = [candidate(1, 1, 1, 1), candidate(1, 2, 1, 2)];
        let snapshot = snapshot(&candidates);
        let mut registry = HealthRegistry::new();
        record_test_rate_limit(
            &mut registry,
            target(1, 1, 1),
            RateLimitScope::Deployment,
            HealthTick::new(10),
            HealthTick::new(1),
        );

        let eligibility = registry.eligibility_for(&snapshot, HealthTick::new(1));
        assert!(!eligibility.allows_index(0));
        assert!(eligibility.allows_index(1));
    }

    #[test]
    fn site_and_unknown_cooldown_block_entire_site() {
        for scope in [RateLimitScope::Site, RateLimitScope::Unknown] {
            let candidates = [
                candidate(1, 1, 1, 1),
                candidate(1, 2, 1, 2),
                candidate(2, 3, 2, 3),
            ];
            let snapshot = snapshot(&candidates);
            let mut registry = HealthRegistry::new();
            record_test_rate_limit(
                &mut registry,
                target(1, 1, 1),
                scope,
                HealthTick::new(10),
                HealthTick::new(1),
            );

            let eligibility = registry.eligibility_for(&snapshot, HealthTick::new(1));
            assert!(!eligibility.allows_index(0));
            assert!(!eligibility.allows_index(1));
            assert!(eligibility.allows_index(2));
        }
    }

    #[test]
    fn rate_limit_observation_applies_across_snapshots_with_same_deployment() {
        let first_candidates = [candidate(1, 1, 1, 1)];
        let second_candidates = [candidate(1, 2, 1, 1)];
        let first_snapshot = snapshot(&first_candidates);
        let second_snapshot = snapshot(&second_candidates);
        let mut registry = HealthRegistry::new();

        record_test_rate_limit(
            &mut registry,
            target(1, 1, 1),
            RateLimitScope::Deployment,
            HealthTick::new(10),
            HealthTick::new(1),
        );

        assert!(!registry
            .eligibility_for(&first_snapshot, HealthTick::new(1))
            .allows_index(0));
        assert!(!registry
            .eligibility_for(&second_snapshot, HealthTick::new(1))
            .allows_index(0));
    }

    #[test]
    fn cooldown_uses_maximum_exact_expiry_and_clamped_time() {
        let candidates = [candidate(1, 1, 1, 1)];
        let snapshot = snapshot(&candidates);
        let mut registry = HealthRegistry::new();
        record_test_rate_limit(
            &mut registry,
            target(1, 1, 1),
            RateLimitScope::Deployment,
            HealthTick::new(20),
            HealthTick::new(10),
        );
        record_test_rate_limit(
            &mut registry,
            target(1, 1, 1),
            RateLimitScope::Deployment,
            HealthTick::new(15),
            HealthTick::new(10),
        );
        assert!(!registry
            .eligibility_for(&snapshot, HealthTick::new(15))
            .allows_index(0));
        assert!(!registry
            .eligibility_for(&snapshot, HealthTick::new(12))
            .allows_index(0));
        assert!(registry
            .eligibility_for(&snapshot, HealthTick::new(20))
            .allows_index(0));
    }

    #[test]
    fn expired_deadline_does_not_create_or_extend_cooldown() {
        let candidates = [candidate(1, 1, 1, 1)];
        let snapshot = snapshot(&candidates);
        let mut registry = HealthRegistry::new();
        let target = target(1, 1, 1);

        record_test_rate_limit(
            &mut registry,
            target,
            RateLimitScope::Deployment,
            HealthTick::new(10),
            HealthTick::new(10),
        );
        assert!(registry
            .eligibility_for(&snapshot, HealthTick::new(10))
            .allows_index(0));

        record_test_rate_limit(
            &mut registry,
            target,
            RateLimitScope::Deployment,
            HealthTick::new(20),
            HealthTick::new(10),
        );
        record_test_rate_limit(
            &mut registry,
            target,
            RateLimitScope::Deployment,
            HealthTick::new(10),
            HealthTick::new(10),
        );
        assert!(!registry
            .eligibility_for(&snapshot, HealthTick::new(10))
            .allows_index(0));
    }

    #[test]
    fn full_registry_uses_global_overflow_cooldown() {
        let candidates = [candidate(1, 1, 1, 1)];
        let snapshot = snapshot(&candidates);
        let mut registry = HealthRegistry::new();
        for value in 2..=MAX_HEALTH_DEPLOYMENTS as u64 + 1 {
            record_test_rate_limit(
                &mut registry,
                target(value, value, value),
                RateLimitScope::Deployment,
                HealthTick::new(20),
                HealthTick::new(1),
            );
        }
        record_test_rate_limit(
            &mut registry,
            target(18, 18, 18),
            RateLimitScope::Deployment,
            HealthTick::new(30),
            HealthTick::new(1),
        );

        assert!(!registry
            .eligibility_for(&snapshot, HealthTick::new(1))
            .allows_index(0));
        assert!(registry
            .eligibility_for(&snapshot, HealthTick::new(30))
            .allows_index(0));
    }

    #[test]
    fn planner_skips_site_cooled_stage_without_promoting_static_unavailable_target() {
        let first_stage = [
            candidate(1, 1, 1, 1),
            candidate(1, 2, 1, 2),
            candidate(2, 3, 2, 3),
        ];
        let first_snapshot = snapshot(&first_stage);
        let mut registry = HealthRegistry::new();
        record_test_rate_limit(
            &mut registry,
            target(1, 1, 1),
            RateLimitScope::Site,
            HealthTick::new(10),
            HealthTick::new(1),
        );
        let eligibility = registry.eligibility_for(&first_snapshot, HealthTick::new(1));
        let plan = RoutePlanner::plan(&routed(&first_snapshot), &first_snapshot, &eligibility, 3)
            .expect("后续站点仍可规划");
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan.target_id(0),
            Some(super::super::RouteTargetId::new(3).expect("测试目标 ID 非零"))
        );

        let static_unavailable = [
            RouteCandidate::cooling_down(
                RouteStageId::new(1).expect("测试阶段 ID 非零"),
                target(1, 1, 1),
            ),
            candidate(1, 2, 1, 2),
        ];
        let static_snapshot = snapshot(&static_unavailable);
        let static_eligibility =
            HealthRegistry::new().eligibility_for(&static_snapshot, HealthTick::new(1));
        let static_plan = RoutePlanner::plan(
            &routed(&static_snapshot),
            &static_snapshot,
            &static_eligibility,
            0,
        )
        .expect("静态可用目标仍可规划");
        assert_eq!(
            static_plan.target_id(0),
            Some(super::super::RouteTargetId::new(2).expect("测试目标 ID 非零"))
        );
    }

    #[test]
    fn planner_rejects_eligibility_from_same_version_other_candidate_identity() {
        let first_candidates = [candidate(1, 1, 1, 1)];
        let second_candidates = [candidate(1, 2, 1, 2)];
        let first = snapshot(&first_candidates);
        let second = snapshot(&second_candidates);
        let eligibility = HealthRegistry::new().eligibility_for(&first, HealthTick::new(1));

        assert!(matches!(
            RoutePlanner::plan(&routed(&second), &second, &eligibility, 0),
            Err(super::super::PlanError::EligibilitySnapshotMismatch)
        ));
    }
}
