//! 受信 adapter 限流合同与冷却投影的最小边界。
//!
//! 本模块不解析 HTTP、`Retry-After`、响应头或错误文本。未来受审 adapter 只能通过固定
//! Site 合同把已换算为 [`HealthTick`] 的截止时间交给当前 Transport handoff；普通兼容
//! adapter、裸状态码和任意资源 ID 均没有构造受信合同的入口。

use super::{
    AdapterRateLimitReporter, RateLimitReporterError, ResourceCooldownReporterError,
    ResourceCooldownScope, TransportHandoffAttempt,
};
use crate::{
    selection_cooldown::{SelectionCooldownRecordError, SelectionCooldownRegistry},
    HealthRegistry, HealthTick, RateLimitScope, ResolvedRouteTarget, SiteId,
};

/// 已登记 adapter 合同固定的限流种类。
///
/// 它仅供未来 Journal 与 adapter 审计读取；当前核心只按固定投影更新未来冷却，不据此决定
/// 重放、等待或故障转移。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(super) enum AdapterRateLimitKind {
    /// 短暂性请求频率限制。
    Transient,
    /// 并发额度限制。
    Concurrency,
    /// 额度已经耗尽。
    QuotaExhausted,
    /// 已登记合同不能进一步区分的限流种类。
    Unknown,
}

/// 已登记 adapter 合同固定的冷却投影。
///
/// 普通信号只允许投影到一个 Registry，避免半成功的隐式双写或 adapter 自由组合范围。
/// `ConservativeUnknown` 是唯一的封闭复合投影：它先原子记录当前 Credential 与全部共享
/// QuotaGroup 的资源冷却，再记录短时 Site 冷却；两个截止时间都由未来宿主完成换算。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CooldownProjection {
    /// 更新当前 Target 的 ModelDeployment 健康冷却。
    HealthDeployment,
    /// 更新当前 Target 所属 Site 的健康冷却。
    HealthSite,
    /// 更新当前实际 Lease 派生的资源冷却。
    Resource(ResourceCooldownScope),
    /// 对未知额度归因作保守复合冷却。
    ///
    /// 资源截止时间通常应长于 Site 截止时间：前者在 Site 恢复后继续阻止同 Key 或共享额度
    /// 组轮换，后者则短暂避免整站 429 风暴。此类型不接受资源 ID，也不改变当前 Attempt。
    ConservativeUnknown,
}

impl CooldownProjection {
    /// 将低层限流范围映射到 adapter 合同；未知范围只能走保守复合投影。
    pub(super) const fn from_rate_limit_scope(scope: RateLimitScope) -> Self {
        match scope {
            RateLimitScope::Deployment => Self::HealthDeployment,
            RateLimitScope::Site => Self::HealthSite,
            RateLimitScope::Unknown => Self::ConservativeUnknown,
        }
    }
}

/// 已由受审站点注册表验证的一次限流合同。
///
/// 它不实现 `Clone` 或 `Copy`，没有正常构造器，也不携带 HTTP 状态、响应头、错误文本、
/// `scope_ref` 或任何裸资源 ID。将来只有固定 Site、稳定错误码、adapter/version/revision
/// 与脱敏 fixture 都匹配的 verifier 才能签发它。
#[must_use = "受信 adapter 限流合同必须交给当前 Transport handoff，或显式丢弃"]
pub(super) struct VerifiedRateLimitContract {
    site: SiteId,
    #[allow(dead_code)]
    stable_error_code: u16,
    #[allow(dead_code)]
    adapter_version: u16,
    #[allow(dead_code)]
    contract_revision: u8,
    #[allow(dead_code)]
    evidence_kind: u8,
    #[allow(dead_code)]
    kind: AdapterRateLimitKind,
    projection: CooldownProjection,
}

/// 合同与当前路由目标不匹配时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VerifiedRateLimitContractError {
    /// 合同所属 Site 与当前实际 Target 不一致。
    SiteMismatch,
    /// 测试 fixture 缺少受信合同所需的版本化元数据。
    InvalidMetadata,
}

impl VerifiedRateLimitContract {
    /// 将合同投影绑定到当前 Target；跨 Site 信号一律拒绝。
    fn into_projection_for(
        self,
        target_site: SiteId,
    ) -> Result<CooldownProjection, VerifiedRateLimitContractError> {
        if self.site != target_site {
            return Err(VerifiedRateLimitContractError::SiteMismatch);
        }
        Ok(self.projection)
    }
}

/// 签发 handoff 限流上报器时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HandoffRateLimitReporterError {
    /// 已登记合同与当前 Target 所属 Site 不一致。
    Contract(VerifiedRateLimitContractError),
    /// Attempt 与已解析 Target / 快照不一致，或该 Attempt 已签发过 reporter。
    Attempt(RateLimitReporterError),
}

/// 消费 handoff 限流上报器时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HandoffRateLimitReportError {
    /// 调用方使用了与固定合同投影不一致的报告入口。
    ProjectionMismatch,
    /// 当前 handoff 已单独签发过资源冷却 reporter，不能再重复归因。
    ResourceReporter(ResourceCooldownReporterError),
    /// C3-A 资源冷却 Registry 保守拒绝该 observation。
    ResourceRegistry(SelectionCooldownRecordError),
}

/// 只能由当前 [`TransportHandoffAttempt`] 签发一次的限流上报器。
///
/// 它同时持有已验证 Target 的 Health reporter 与当前 handoff 的可变借用。`Resource`
/// 投影会丢弃前者，并只从实际 Lease 派生资源身份；`Health` 投影不会触碰资源 Registry。
/// 两种路径都不接触 Replay、Completion 或 Coordinator。
#[must_use = "handoff 限流上报器必须消费为冷却投影，或显式丢弃"]
pub(super) struct HandoffRateLimitReporter<'handoff, 'registry, 'snapshot, 'config> {
    health_reporter: AdapterRateLimitReporter,
    handoff: &'handoff mut TransportHandoffAttempt<'registry, 'snapshot, 'config>,
    projection: CooldownProjection,
}

impl<'handoff, 'registry, 'snapshot, 'config>
    HandoffRateLimitReporter<'handoff, 'registry, 'snapshot, 'config>
{
    /// 由已验证的 handoff 与合同投影创建上报器。
    fn new(
        health_reporter: AdapterRateLimitReporter,
        handoff: &'handoff mut TransportHandoffAttempt<'registry, 'snapshot, 'config>,
        projection: CooldownProjection,
    ) -> Self {
        Self {
            health_reporter,
            handoff,
            projection,
        }
    }

    /// 消费上报器并向恰当的单一 Registry 写入冷却。
    ///
    /// `deadline` 与 `now` 都是宿主已经换算完成的单调刻度；它们不是 HTTP 日期或重试秒数。
    /// 截止时间无效时沿用既有 Registry 的无冷却语义，绝不因此产生当前请求的重放资格。
    pub(super) fn report(
        self,
        deadline: HealthTick,
        now: HealthTick,
        health: &mut HealthRegistry,
        resources: &mut SelectionCooldownRegistry,
    ) -> Result<(), HandoffRateLimitReportError> {
        let Self {
            health_reporter,
            handoff,
            projection,
        } = self;
        match projection {
            CooldownProjection::HealthDeployment => {
                health.record_rate_limit(
                    health_reporter.report(RateLimitScope::Deployment, deadline),
                    now,
                );
                Ok(())
            }
            CooldownProjection::HealthSite => {
                health
                    .record_rate_limit(health_reporter.report(RateLimitScope::Site, deadline), now);
                Ok(())
            }
            CooldownProjection::Resource(scope) => {
                let _validated_target_reporter = health_reporter;
                let observation = handoff
                    .resource_cooldown_reporter()
                    .map_err(HandoffRateLimitReportError::ResourceReporter)?
                    .report(scope, deadline);
                resources
                    .record(observation, now)
                    .map_err(HandoffRateLimitReportError::ResourceRegistry)
            }
            CooldownProjection::ConservativeUnknown => {
                let _validated_target_reporter = health_reporter;
                let _handoff = handoff;
                Err(HandoffRateLimitReportError::ProjectionMismatch)
            }
        }
    }

    /// 消费保守 Unknown 合同，并按固定顺序写入复合冷却。
    ///
    /// `resource_deadline` 与 `site_deadline` 是宿主已经换算完成的单调刻度。资源 Registry
    /// 拒绝 observation 时不会写 Site，避免出现 Site-only 的半成功。成功时先冷却当前
    /// Credential 与全部共享 QuotaGroup，再冷却当前 Target 所属 Site。
    /// 此路径仅影响后续请求的 eligibility，不触碰 Replay、Completion 或 Coordinator。
    pub(super) fn report_conservative_unknown(
        self,
        site_deadline: HealthTick,
        resource_deadline: HealthTick,
        now: HealthTick,
        health: &mut HealthRegistry,
        resources: &mut SelectionCooldownRegistry,
    ) -> Result<(), HandoffRateLimitReportError> {
        let Self {
            health_reporter,
            handoff,
            projection,
        } = self;
        if projection != CooldownProjection::ConservativeUnknown {
            return Err(HandoffRateLimitReportError::ProjectionMismatch);
        }

        let observation = handoff
            .resource_cooldown_reporter()
            .map_err(HandoffRateLimitReportError::ResourceReporter)?
            .report(
                ResourceCooldownScope::CurrentCredentialAndQuotaGroups,
                resource_deadline,
            );
        resources
            .record(observation, now)
            .map_err(HandoffRateLimitReportError::ResourceRegistry)?;
        health.record_rate_limit(
            health_reporter.report(RateLimitScope::Site, site_deadline),
            now,
        );
        Ok(())
    }
}

/// 在当前 handoff 内校验合同并签发唯一的限流上报器。
pub(super) fn issue<'handoff, 'registry, 'snapshot, 'config>(
    handoff: &'handoff mut TransportHandoffAttempt<'registry, 'snapshot, 'config>,
    resolved: ResolvedRouteTarget<'snapshot, 'config>,
    contract: VerifiedRateLimitContract,
) -> Result<
    HandoffRateLimitReporter<'handoff, 'registry, 'snapshot, 'config>,
    HandoffRateLimitReporterError,
> {
    let projection = contract
        .into_projection_for(resolved.target().site())
        .map_err(HandoffRateLimitReporterError::Contract)?;
    let health_reporter = handoff
        .rate_limit_reporter(resolved)
        .map_err(HandoffRateLimitReporterError::Attempt)?;
    Ok(HandoffRateLimitReporter::new(
        health_reporter,
        handoff,
        projection,
    ))
}

#[cfg(test)]
pub(super) fn test_only_verified_rate_limit_contract(
    site: SiteId,
    stable_error_code: u16,
    adapter_version: u16,
    contract_revision: u8,
    evidence_kind: u8,
    kind: AdapterRateLimitKind,
    projection: CooldownProjection,
) -> Result<VerifiedRateLimitContract, VerifiedRateLimitContractError> {
    if stable_error_code == 0
        || adapter_version == 0
        || contract_revision == 0
        || evidence_kind == 0
    {
        return Err(VerifiedRateLimitContractError::InvalidMetadata);
    }
    Ok(VerifiedRateLimitContract {
        site,
        stable_error_code,
        adapter_version,
        contract_revision,
        evidence_kind,
        kind,
        projection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(value: u64) -> SiteId {
        SiteId::new(value).expect("测试 Site ID 非零")
    }

    #[test]
    fn registered_fixture_requires_complete_metadata_and_exact_site() {
        assert!(matches!(
            test_only_verified_rate_limit_contract(
                site(1),
                0,
                1,
                1,
                1,
                AdapterRateLimitKind::Transient,
                CooldownProjection::HealthSite,
            ),
            Err(VerifiedRateLimitContractError::InvalidMetadata)
        ));

        let contract = test_only_verified_rate_limit_contract(
            site(1),
            0x1001,
            1,
            1,
            1,
            AdapterRateLimitKind::Transient,
            CooldownProjection::HealthSite,
        )
        .expect("完整 fixture 可模拟已登记合同");
        assert_eq!(
            contract.into_projection_for(site(2)),
            Err(VerifiedRateLimitContractError::SiteMismatch)
        );
    }

    #[test]
    fn registered_contract_keeps_all_scope_mappings_fixed() {
        let projections = [
            CooldownProjection::Resource(ResourceCooldownScope::Credential),
            CooldownProjection::Resource(ResourceCooldownScope::Account),
            CooldownProjection::Resource(ResourceCooldownScope::CurrentQuotaGroups),
            CooldownProjection::HealthDeployment,
            CooldownProjection::HealthSite,
            CooldownProjection::ConservativeUnknown,
        ];

        for projection in projections {
            let contract = test_only_verified_rate_limit_contract(
                site(1),
                0x1001,
                1,
                1,
                1,
                AdapterRateLimitKind::QuotaExhausted,
                projection,
            )
            .expect("完整 fixture 可模拟已登记合同");
            assert_eq!(contract.into_projection_for(site(1)), Ok(projection));
        }
    }

    #[test]
    fn unknown_scope_maps_only_to_conservative_composite_projection() {
        assert_eq!(
            CooldownProjection::from_rate_limit_scope(RateLimitScope::Unknown),
            CooldownProjection::ConservativeUnknown
        );
        assert_eq!(
            CooldownProjection::from_rate_limit_scope(RateLimitScope::Deployment),
            CooldownProjection::HealthDeployment
        );
        assert_eq!(
            CooldownProjection::from_rate_limit_scope(RateLimitScope::Site),
            CooldownProjection::HealthSite
        );
    }
}
