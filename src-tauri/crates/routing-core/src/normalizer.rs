//! 受信客户端协议 Normalizer 合同。

use ingress_contract::verified::ReceiverAcceptedIngressRequest;
use ingress_contract::{
    HttpMethod, IngressProtocol, OperationId, RequestDigest, RequestDispatchDomain, RequestKind,
    VerifiedHeaderRef,
};

use super::error::ProtocolNormalizeError;

/// Normalizer 对请求结构确认后的语义闭集。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NormalizedRequestSemantic {
    /// 已注册的本地 Operation。
    LocalOperation,
    /// 无用户正文、固定单部署的管理模型探测。
    DeploymentModelProbe,
    /// 与真实请求同部署、禁止 fallback 的远程精确 token 计数。
    ExactUpstreamTokenCount,
    /// 普通模型推理。
    ModelInference,
    /// 使用独立 RoutePolicy 的辅助推理。
    AuxiliaryInference,
}

/// 只在 classifier 内构造、供受信 Normalizer 同步读取的请求视图。
///
/// 它只能借用 receiver 已接受的 typestate，无法接收尚未通过 receiver 的入站请求。
pub struct NormalizerInput<'a> {
    accepted: &'a ReceiverAcceptedIngressRequest,
}

impl<'a> NormalizerInput<'a> {
    pub(crate) const fn new(accepted: &'a ReceiverAcceptedIngressRequest) -> Self {
        Self { accepted }
    }

    /// 返回已匹配的 Operation。
    pub const fn operation(&self) -> OperationId {
        self.accepted.operation()
    }

    /// 返回 RouteSpec 固定的请求语义。
    pub const fn request_kind(&self) -> RequestKind {
        self.accepted.request_kind()
    }

    /// 返回 RouteSpec 固定的分发域。
    pub const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.accepted.dispatch_domain()
    }

    /// 返回 RouteSpec 固定的客户端协议。
    pub const fn ingress_protocol(&self) -> IngressProtocol {
        self.accepted.ingress_protocol()
    }

    /// 返回证明绑定的原始请求摘要。
    pub fn request_digest(&self) -> RequestDigest {
        self.accepted.request_digest()
    }

    /// 返回清理后的 HTTP 方法。
    pub const fn method(&self) -> HttpMethod {
        self.accepted.method()
    }

    /// 返回已验证的原始 request target。
    pub fn target(&self) -> &[u8] {
        self.accepted.target()
    }

    /// 返回原始正文的只读借用。
    ///
    /// 正文只提供给与 receiver 同 crate 编译的受信 Normalizer 实现；公开 trait 不是第三方
    /// 正文插件面，避免在 Local/Context/Bound gate 完成前出现外部复制或外传能力。
    #[allow(dead_code)] // 当前切片尚未接入具体协议 Adapter；为同 crate 的后续实现保留。
    pub(crate) fn body(&self) -> &[u8] {
        self.accepted.body()
    }

    /// 遍历已清理的 Header。
    pub fn headers(&self) -> impl Iterator<Item = VerifiedHeaderRef<'_>> {
        self.accepted.headers()
    }
}

/// Normalizer 已确认的协议结果。
///
/// 核心绑定只能从 [`NormalizerInput`] 复制；字段私有，且本类型不实现 `Clone`、`Debug`、
/// `Default` 或任何 Serde trait。classifier 会再次与 receiver typestate 逐字段比较。
pub struct ProtocolNormalizedRequest {
    operation: OperationId,
    request_kind: RequestKind,
    dispatch_domain: RequestDispatchDomain,
    ingress_protocol: IngressProtocol,
    request_digest: RequestDigest,
    semantic: NormalizedRequestSemantic,
}

impl ProtocolNormalizedRequest {
    /// 从当前 receiver-accepted 输入确认协议语义。
    ///
    /// 该函数不能替换 Operation、协议、分发域或摘要，只允许受信 Normalizer 表达其已完成
    /// 对正文/frame 的结构检查。
    pub fn accept(input: &NormalizerInput<'_>, semantic: NormalizedRequestSemantic) -> Self {
        Self {
            operation: input.operation(),
            request_kind: input.request_kind(),
            dispatch_domain: input.dispatch_domain(),
            ingress_protocol: input.ingress_protocol(),
            request_digest: input.request_digest(),
            semantic,
        }
    }

    pub(crate) const fn operation(&self) -> OperationId {
        self.operation
    }

    pub(crate) const fn request_kind(&self) -> RequestKind {
        self.request_kind
    }

    pub(crate) const fn dispatch_domain(&self) -> RequestDispatchDomain {
        self.dispatch_domain
    }

    pub(crate) const fn ingress_protocol(&self) -> IngressProtocol {
        self.ingress_protocol
    }

    pub(crate) const fn request_digest(&self) -> RequestDigest {
        self.request_digest
    }

    pub(crate) const fn semantic(&self) -> NormalizedRequestSemantic {
        self.semantic
    }
}

/// 受信客户端协议 Normalizer。
///
/// 实现必须是同步、纯计算且无外部副作用；当前切片只定义合同，不提供任何 Claude、Codex、
/// OpenAI 或 Gemini 的生产 Adapter。
pub trait ClientProtocolNormalizer: Send + Sync {
    /// 验证协议正文/frame，并返回绑定当前 accepted 请求的闭集语义。
    fn normalize(
        &self,
        input: NormalizerInput<'_>,
    ) -> Result<ProtocolNormalizedRequest, ProtocolNormalizeError>;
}
