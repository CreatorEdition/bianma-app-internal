//! `routing-core v2` 的纯 Rust 入站验证合同。
//!
//! 本 crate 只验证请求并产生不可伪造的 [`VerifiedIngressRequest`]。它不包含 HTTP
//! 客户端、数据库、Secret 解析、Planner、Transport 或生产代理接线。
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
//! verifier、listener/consent authority 与 classifier receiver 由同一 runtime seal 绑定；
//! 其他 verifier 实例产生的请求无法通过生产 receiver。

#![warn(missing_docs)]

mod error;
mod ids;
mod limits;
mod operation;
mod replay;
mod request;
pub mod signed;
pub mod verified;
pub mod verifier;

pub use error::{IngressReject, NonceReject};
pub use ids::*;
pub use limits::*;
pub use operation::{
    BodyPolicy, LocalOperationAuthScope, QueryPolicy, RequestDispatchDomain, RequestKind,
    RouteSpec, RouteSpecRegistry,
};
pub use replay::{MemoryNonceStore, NonceNamespace, OneShotNonceStore};
pub use request::{HttpMethod, RawHeader, RawIngressRequest};
pub use signed::{CanonicalOrigin, EncodedCapabilityAuthorization, SignedIngressRequest};
pub use verified::{
    VerifiedAuthorizationKind, VerifiedCapabilityBindingView, VerifiedHeaderRef,
    VerifiedIngressReceiver, VerifiedIngressRequest, VerifiedProofKind,
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
