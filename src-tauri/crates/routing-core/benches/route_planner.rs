use routing_core::{
    AccountCredentialDefinitions, AccountDefinition, AccountId, AccountSelectorDefinition,
    AccountSelectorId, AccountSelectorMember, CompiledRoutingSnapshot, CredentialDefinition,
    CredentialId, CredentialSelectionPolicy, EndpointId, HealthRegistry, HealthTick,
    IngressClassifier, IngressRequest, ModelDeploymentDefinition, ModelDeploymentId, OperationId,
    QuotaGroupId, QuotaSelectionUnit, QuotaSelectionUnitId, QuotaTopologySource, RouteCandidate,
    RoutePlanner, RouteStageId, RouteTarget, RoutingStrategy, SiteId, SnapshotVersion,
    VerifiedIngressDisposition,
};
use std::hint::black_box;
use std::num::NonZeroU16;
use std::time::Instant;

const ITERATIONS: u64 = 100_000;

fn target(value: u64) -> RouteTarget {
    RouteTarget::new(
        routing_core::RouteTargetId::new(value).expect("基准 ID 非零"),
        SiteId::new(1).expect("站点 ID 非零"),
        ModelDeploymentId::new(value).expect("部署 ID 非零"),
        EndpointId::new(value).expect("端点 ID 非零"),
        AccountSelectorId::new(1).expect("账户选择合同 ID 非零"),
    )
}

fn main() {
    let first_stage = RouteStageId::new(1).expect("基准阶段 ID 非零");
    let mut candidates = [RouteCandidate::ready(first_stage, target(1), 0); 16];
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let value = (index + 1) as u64;
        let stage = RouteStageId::new((index / 4 + 1) as u64).expect("基准阶段 ID 非零");
        *candidate = RouteCandidate::ready(stage, target(value), index as u16);
    }
    let deployments: [ModelDeploymentDefinition; 16] = core::array::from_fn(|index| {
        let value = (index + 1) as u64;
        ModelDeploymentDefinition::new(
            ModelDeploymentId::new(value).expect("基准部署 ID 非零"),
            SiteId::new(1).expect("基准站点 ID 非零"),
            EndpointId::new(value).expect("基准端点 ID 非零"),
        )
    });
    let quota_groups = [QuotaGroupId::new(1).expect("基准额度组 ID 非零")];
    let units = [QuotaSelectionUnit::new(
        QuotaSelectionUnitId::new(1).expect("基准额度单元 ID 非零"),
        NonZeroU16::new(1).expect("基准权重非零"),
        &quota_groups,
    )];
    let members = [AccountSelectorMember::new(
        AccountId::new(1).expect("基准账户 ID 非零"),
        CredentialId::new(1).expect("基准凭据 ID 非零"),
        QuotaSelectionUnitId::new(1).expect("基准额度单元 ID 非零"),
        0,
    )];
    let selectors = [AccountSelectorDefinition::new(
        AccountSelectorId::new(1).expect("基准选择合同 ID 非零"),
        CredentialSelectionPolicy::PriorityFailover,
        QuotaTopologySource::ConservativeDefault,
        &units,
        &members,
    )
    .expect("基准选择合同有效")];
    let accounts = [AccountDefinition::new(
        AccountId::new(1).expect("基准账户 ID 非零"),
        SiteId::new(1).expect("基准站点 ID 非零"),
    )];
    let credentials = [CredentialDefinition::new(
        CredentialId::new(1).expect("基准凭据 ID 非零"),
        AccountId::new(1).expect("基准账户 ID 非零"),
    )];
    let compiled = CompiledRoutingSnapshot::compile(
        SnapshotVersion::new(1).expect("快照版本非零"),
        &candidates,
        RoutingStrategy::LeastPenalty,
        16,
        &deployments,
        AccountCredentialDefinitions::new(&accounts, &credentials),
        &selectors,
    )
    .expect("基准编译快照有效");
    let snapshot = compiled.routing();
    let eligibility = HealthRegistry::new().eligibility_for(snapshot, HealthTick::new(0));
    let disposition = IngressClassifier::new()
        .classify(IngressRequest::routed(
            OperationId::CONVERSATION,
            snapshot.version(),
        ))
        .expect("基准路由请求分类成功");
    let VerifiedIngressDisposition::Routed(request) = disposition else {
        panic!("会话操作必须得到 Routed 分发");
    };

    let started = Instant::now();
    let mut checksum = 0u64;
    for cursor in 0..ITERATIONS {
        let plan = black_box(
            RoutePlanner::plan(&request, snapshot, &eligibility, cursor).expect("基准计划有效"),
        );
        checksum = checksum.wrapping_add(u64::from(plan.len()));
        checksum = checksum.wrapping_add(
            plan.target_id((cursor % u64::from(plan.len())) as u8)
                .expect("基准目标存在")
                .get(),
        );
    }
    let elapsed = started.elapsed();

    assert_ne!(checksum, 0);
    println!(
        "routing-core: {ITERATIONS} plans in {:?} ({:.2} ns/plan)",
        elapsed,
        elapsed.as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64
    );
}
