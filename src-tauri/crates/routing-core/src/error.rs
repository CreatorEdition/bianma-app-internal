//! 分类、协议规范化与逐请求门禁错误。

use ingress_contract::IngressReject;
use thiserror::Error;

/// 受信协议 Normalizer 的封闭失败类别。
///
/// 错误不携带正文、Header、URL 或任何凭据相关输入，因而可以安全地映射到结构化错误码。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ProtocolNormalizeError {
    /// 请求正文或协议 frame 不满足注册 Operation 的结构合同。
    #[error("协议请求结构无效")]
    MalformedRequest,
    /// 请求使用了当前协议 Adapter 尚未支持的闭集能力。
    #[error("协议能力不受支持")]
    UnsupportedFeature,
    /// Normalizer 与 RouteSpec 固定的协议或请求语义不一致。
    #[error("协议规范化结果不匹配")]
    BindingMismatch,
    /// Normalizer 无法在不泄露输入的情况下继续处理。
    #[error("协议规范化已安全终止")]
    InternalFailClosed,
}

/// 封闭分类器的无敏感数据拒绝原因。
#[derive(Debug, Error)]
pub enum RouteReject {
    /// ingress receiver 拒绝了运行时、注册表或 Verified 请求。
    #[error("入站验证合同拒绝请求")]
    Ingress(#[from] IngressReject),
    /// 快照不是由当前分类器配对的 authority 签发。
    #[error("分类器快照不属于当前运行时")]
    SnapshotDomainMismatch,
    /// 分类器、快照与已接受请求的注册表摘要不一致。
    #[error("RouteSpec 注册表摘要不匹配")]
    RegistryMismatch,
    /// 快照的 Operation 或原始请求摘要不匹配。
    #[error("逐请求快照绑定不匹配")]
    SnapshotBindingMismatch,
    /// 受信协议 Normalizer 明确拒绝请求。
    #[error("协议规范化失败")]
    Normalize(#[from] ProtocolNormalizeError),
    /// Normalizer 返回的核心绑定或闭集语义与 ingress 不一致。
    #[error("协议规范化绑定不匹配")]
    NormalizedBindingMismatch,
    /// proof、授权种类、RequestKind、dispatch domain 或快照模式组合不在闭集中。
    #[error("请求分发组合未注册")]
    DispositionNotAllowed,
    /// Local Operation 的 listener、token 或授权 scope 不匹配。
    #[error("本地 Operation scope 门禁拒绝请求")]
    LocalScopeMismatch,
    /// Managed 或 GatewayOnly 的逐请求 Context 门禁拒绝请求。
    #[error("Context 逐请求门禁拒绝请求")]
    ContextGateRejected,
    /// 固定部署、账户、凭据或 Adapter 的 Bound 门禁拒绝请求。
    #[error("BoundDeployment 门禁拒绝请求")]
    BoundGateRejected,
    /// 分类器墙钟不可用或发生不可接受的回拨。
    #[error("分类器时钟不可用")]
    ClockUnavailable,
    /// proof、consent、permit 或能力授权已经过期。
    #[error("逐请求授权已过期")]
    AuthorizationExpired,
    /// authority 输入包含零标识、零摘要或其他不可能安全执行的值。
    #[error("分类器快照输入无效")]
    InvalidSnapshot,
}
