use routing_core::{
    AccountSelectorId, EndpointId, ModelDeploymentId, RouteCandidate, RoutePlanner, RouteStageId,
    RouteTarget, RouteTargetId, RoutingSnapshot, RoutingStrategy, SiteId, SnapshotVersion,
};
use std::{hint::black_box, time::Instant};

const ITERATIONS: u64 = 100_000;

fn id<T>(value: u64, make: fn(u64) -> Option<T>) -> T {
    make(value).expect("基准 ID 非零")
}

fn target(value: u64) -> RouteTarget {
    RouteTarget::new(
        id(value, RouteTargetId::new),
        id(value, SiteId::new),
        id(value, ModelDeploymentId::new),
        id(value, EndpointId::new),
        id(value, AccountSelectorId::new),
    )
}

fn main() {
    let first_stage = id(1, RouteStageId::new);
    let mut candidates = [RouteCandidate::ready(first_stage, target(1), 0); 16];
    for (index, candidate) in candidates.iter_mut().enumerate() {
        let value = (index + 1) as u64;
        *candidate = RouteCandidate::ready(
            id((index / 4 + 1) as u64, RouteStageId::new),
            target(value),
            index as u16,
        );
    }
    let snapshot = RoutingSnapshot::new(
        id(1, SnapshotVersion::new),
        &candidates,
        RoutingStrategy::LeastPenalty,
        16,
    )
    .expect("基准快照有效");

    for cursor in 0..10_000 {
        black_box(RoutePlanner::plan(&snapshot, cursor).expect("预热计划有效"));
    }
    let started = Instant::now();
    let mut checksum = 0u64;
    for cursor in 0..ITERATIONS {
        let plan = black_box(RoutePlanner::plan(&snapshot, cursor).expect("基准计划有效"));
        checksum = checksum.wrapping_add(u64::from(plan.len()));
        checksum = checksum.wrapping_add(plan.target_id(0).unwrap().unwrap().get());
    }
    let elapsed = started.elapsed();
    assert_ne!(checksum, 0);
    println!(
        "routing-core: {ITERATIONS} plans in {:?} ({:.2} ns/plan)",
        elapsed,
        elapsed.as_secs_f64() * 1_000_000_000.0 / ITERATIONS as f64
    );
}
