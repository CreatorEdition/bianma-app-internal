//! `routing-core v2` 的纯 Rust 入站验证合同。
//!
//! 本 crate 验证请求并产生不可伪造的 [`VerifiedIngressRequest`]；生产 classifier 与
//! receiver 在同一 crate 私有边界内消费它。它不包含 HTTP 客户端、数据库、Secret 解析、
//! Planner、Transport 或生产代理接线。
//!
//! [`VerifiedIngressRequest`] 故意不实现 `Clone`、`Debug` 或反序列化：
//!
//! ```compile_fail
//! use ingress_contract::VerifiedIngressRequest;
//!
//! fn duplicate(request: &VerifiedIngressRequest) -> VerifiedIngressRequest {
//!     request.clone()
//! }
//! ```
//!
//! ```compile_fail
//! use ingress_contract::VerifiedIngressRequest;
//!
//! fn leak(request: &VerifiedIngressRequest) -> String {
//!     format!("{request:?}")
//! }
//! ```
//!
//! verifier 结果不提供请求读取面，不能绕过实例绑定 receiver：
//!
//! ```compile_fail
//! use ingress_contract::VerifiedIngressRequest;
//!
//! fn bypass_receiver(request: &VerifiedIngressRequest) -> &[u8] {
//!     request.body()
//! }
//! ```
//!
//! receiver 的接受面是 crate-private，外部宿主不能绕过 classifier 读取正文：
//!
//! ```compile_fail
//! use ingress_contract::{VerifiedIngressReceiver, VerifiedIngressRequest};
//!
//! fn bypass(receiver: &VerifiedIngressReceiver, request: VerifiedIngressRequest) {
//!     let _ = receiver.accept(request);
//! }
//! ```
//!
//! receiver 接受后的内部 typestate 也不会导出为公共 API：
//!
//! ```compile_fail
//! use ingress_contract::ReceiverAcceptedIngressRequest;
//! ```
//!
//! verifier、listener/consent authority 与 classifier receiver 由同一 runtime seal 绑定；
//! 其他 verifier 实例产生的请求无法通过生产 receiver。

#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate self as ingress_contract;

mod error;
mod ids;
mod limits;
mod operation;
mod replay;
mod request;
mod routing_core_impl;
pub mod signed;
pub mod verified;
pub mod verifier;

pub use error::{IngressReject, NonceReject};
pub use ids::*;
pub use limits::*;
pub use operation::{
    BodyPolicy, IngressProtocol, LocalOperationAuthScope, QueryPolicy, RequestDispatchDomain,
    RequestKind, RouteSpec, RouteSpecRegistry,
};
pub use replay::{MemoryNonceStore, NonceNamespace, OneShotNonceStore};
pub use request::{HttpMethod, RawHeader, RawIngressRequest};
pub use routing_core_impl::classifier::{ClassifierRuntime, ClosedRequestClassifier};
pub use routing_core_impl::clock::{ClassifierClock, SystemClassifierClock};
pub use routing_core_impl::disposition::{
    BoundFallbackPolicy, ContextExecutionMode, VerifiedBoundDeploymentRequest,
    VerifiedIngressDisposition, VerifiedLocalDispatch, VerifiedRouteRequest,
};
pub use routing_core_impl::error::{ProtocolNormalizeError, RouteReject};
pub use routing_core_impl::normalizer::{
    ClientProtocolNormalizer, NormalizedRequestSemantic, NormalizerInput, ProtocolNormalizedRequest,
};
pub use routing_core_impl::snapshot::{
    ClassifierBoundTarget, ClassifierCapabilityBound, ClassifierGatewayContext,
    ClassifierIngressBinding, ClassifierManagedActivation, ClassifierManagedContext,
    ClassifierRequestBinding, ClassifierSnapshot, ClassifierSnapshotAuthority,
};
pub use signed::{CanonicalOrigin, EncodedCapabilityAuthorization, SignedIngressRequest};
pub use verified::{
    VerifiedAuthorizationKind, VerifiedCapabilityBindingView, VerifiedContinuationConstraint,
    VerifiedEgressPurpose, VerifiedGatewayOnlyView, VerifiedHeaderRef, VerifiedIngressReceiver,
    VerifiedIngressRequest, VerifiedManagedActivationKeyView,
    VerifiedManagedCapabilityRequirementsView, VerifiedManagedEgressPermitView,
    VerifiedManagedEgressTargetView, VerifiedManagedRequestView, VerifiedProofKind,
    VerifiedSensitivityClass,
};
pub use verifier::{
    CapabilityListenerContext, CapabilityVerificationKey, CapabilityVerificationKeyRing,
    ContextActivationBinding, FixedClock, GatewayConsentAuthority, GatewayOnlyConsentSnapshot,
    GatewayOnlyListenerContext, IngressVerifier, IngressVerifierRuntime, ListenerBindingAuthority,
    LocalListenerContext, ManagedActivationAuthority, ManagedActivationBinding,
    ManagedListenerContext, ManagedVerificationKey, ManagedVerificationKeyRing, SystemClock,
    VerifierClock,
};

#[cfg(test)]
mod tests;
