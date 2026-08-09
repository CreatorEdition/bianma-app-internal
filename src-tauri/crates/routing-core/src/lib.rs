//! `routing-core v2` 的纯 Rust 封闭请求分类合同。
//!
//! 本 crate 只消费同一运行时 [`ingress_contract::VerifiedIngressReceiver`] 接受的请求，
//! 同步调用受信协议 Normalizer，并在逐请求门禁后产生 Local、BoundDeployment 或
//! Routed 三种不可互换的 typestate。它不包含 Planner、Vault、SecretResolver、HTTP、
//! 数据库、重试、故障转移或生产代理接线。
//!
//! BoundDeployment typestate 不可复制：
//!
//! ```compile_fail
//! use routing_core::VerifiedBoundDeploymentRequest;
//!
//! fn duplicate(request: &VerifiedBoundDeploymentRequest) -> VerifiedBoundDeploymentRequest {
//!     request.clone()
//! }
//! ```
//!
//! BoundDeployment 不能提升为普通 Routed 请求：
//!
//! ```compile_fail
//! use routing_core::{VerifiedBoundDeploymentRequest, VerifiedRouteRequest};
//!
//! fn promote(request: VerifiedBoundDeploymentRequest) -> VerifiedRouteRequest {
//!     request.into()
//! }
//! ```
//!
//! Local typestate 同样不能提升为普通 Routed 请求：
//!
//! ```compile_fail
//! use routing_core::{VerifiedLocalDispatch, VerifiedRouteRequest};
//!
//! fn promote(request: VerifiedLocalDispatch) -> VerifiedRouteRequest {
//!     request.into()
//! }
//! ```
//!
//! Verified 输出不实现 `Debug`，避免被普通日志意外展开：
//!
//! ```compile_fail
//! use routing_core::VerifiedRouteRequest;
//!
//! fn render(request: &VerifiedRouteRequest) -> String {
//!     format!("{request:?}")
//! }
//! ```
//!
//! Verified 输出不能通过 Serde 反序列化伪造：
//!
//! ```compile_fail
//! use routing_core::VerifiedRouteRequest;
//! use serde::de::DeserializeOwned;
//!
//! fn require_deserialize<T: DeserializeOwned>() {}
//!
//! require_deserialize::<VerifiedRouteRequest>();
//! ```
//!
//! 三路输出的字段私有，调用方不能用结构体字面量绕过 classifier：
//!
//! ```compile_fail
//! use routing_core::VerifiedLocalDispatch;
//!
//! fn forge() -> VerifiedLocalDispatch {
//!     VerifiedLocalDispatch {}
//! }
//! ```
//!
//! classifier 内部中间状态没有公开构造入口：
//!
//! ```compile_fail
//! use routing_core::ClassifiedRequest;
//! ```
//!
//! Normalizer 的公开合同不授予正文读取权；正文只能由与 receiver 同 crate 编译的受信
//! 实现读取，外部实现不能在完整 gate 前复制请求：
//!
//! ```compile_fail
//! use routing_core::NormalizerInput;
//!
//! fn leak(input: &NormalizerInput<'_>) -> &[u8] {
//!     input.body()
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
mod tests;

pub use ingress_contract::{
    BoundFallbackPolicy, ClassifierBoundTarget, ClassifierCapabilityBound, ClassifierClock,
    ClassifierGatewayContext, ClassifierIngressBinding, ClassifierManagedActivation,
    ClassifierManagedContext, ClassifierRequestBinding, ClassifierRuntime, ClassifierSnapshot,
    ClassifierSnapshotAuthority, ClientProtocolNormalizer, ClosedRequestClassifier,
    ContextExecutionMode, NormalizedRequestSemantic, NormalizerInput, ProtocolNormalizeError,
    ProtocolNormalizedRequest, RouteReject, SystemClassifierClock, VerifiedBoundDeploymentRequest,
    VerifiedIngressDisposition, VerifiedLocalDispatch, VerifiedRouteRequest,
};
