//! 与 ingress verifier 同 crate 编译的 routing-core 封闭分类实现。
//!
//! Rust 不提供 friend crate 可见性；这些模块必须与 receiver 同 crate 编译，才能让
//! `ReceiverAcceptedIngressRequest` 保持 crate-private。外部 `routing-core` package 只提供
//! 公开 facade 与合同测试，不重新编译实现。

#[path = "../../routing-core/src/classifier.rs"]
pub(crate) mod classifier;
#[path = "../../routing-core/src/clock.rs"]
pub(crate) mod clock;
#[path = "../../routing-core/src/disposition.rs"]
pub(crate) mod disposition;
#[path = "../../routing-core/src/error.rs"]
pub(crate) mod error;
#[path = "../../routing-core/src/normalizer.rs"]
pub(crate) mod normalizer;
#[path = "../../routing-core/src/snapshot.rs"]
pub(crate) mod snapshot;
