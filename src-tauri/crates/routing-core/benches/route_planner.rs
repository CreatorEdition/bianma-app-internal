use routing_core::{
    AccountId, CredentialId, EndpointId, ModelDeploymentId, RouteCandidate, RoutePlanner,
    RouteTarget, RoutingSnapshot, RoutingStrategy, SiteId, SnapshotVersion,
};
use std::hint::black_box;
use std::time::Instant;

const ITERATIONS: u64 = 100_000;

fn target(value: u64, priority: u16) -> RouteTarget {
    RouteTarget::new(
        routing_core::RouteTargetId::new(value).expect("基准 ID 非零"),
        SiteId::new(value).expect("站点 ID 非零"),
        ModelDeploymentId::new(value).expect("部署 ID 非零"),
        EndpointId::new(value).expect("端点 ID 非零"),
        AccountId::new(value).expect("账户 ID 非零"),
        CredentialId::new(value).expect("凭据 ID 非零"),
        priority,
    )
}

fn main() {
    let mut candidates = [RouteCandidate::ready(target(1, 1), 0); 16];
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let value = (index + 1) as u64;
        *candidate = RouteCandidate::ready(target(value, index as u16), index as u16);
    }
    let snapshot = RoutingSnapshot::new(
        SnapshotVersion::new(1).expect("快照版本非零"),
        &candidates,
        RoutingStrategy::LeastPenalty,
        16,
    )
    .expect("基准快照有效");

    let started = Instant::now();
    let mut checksum = 0u64;
    for cursor in 0..ITERATIONS {
        let plan = black_box(RoutePlanner::plan(&snapshot, cursor).expect("基准计划有效"));
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
