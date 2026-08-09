//! routing-core 的闭集入站操作规格与默认拒绝分类器。
//!
//! 本模块只根据内置 `OperationId` 查找固定 `RouteSpec`，不读取请求正文，
//! 不接受外部 Normalizer，也不负责网络、认证或 ContextPipeline。
//! ContextPipeline 的压缩、记忆和图操作不注册为 routing-core 的操作。

use super::{ModelDeploymentId, SnapshotVersion};

/// 内置操作规格的固定上限。
pub const MAX_ROUTE_SPECS: usize = 10;

/// 稳定的入站操作标识。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(u8);

impl OperationId {
    /// 本地存活探针。
    pub const LIVENESS: Self = Self(1);
    /// 本地管理状态查询。
    pub const STATUS: Self = Self(2);
    /// 本地统一模型目录查询。
    pub const UNIFIED_MODEL_CATALOG: Self = Self(3);
    /// 本地 tokenizer 精确计数。
    pub const EXACT_LOCAL_TOKEN_COUNT: Self = Self(4);
    /// 本地估算计数。
    pub const ESTIMATED_LOCAL_TOKEN_COUNT: Self = Self(5);
    /// 纯本地 Context compact handler；实现仍属于 ContextPipeline。
    pub const LOCAL_CONTEXT_COMPACT: Self = Self(6);
    /// 指定部署的模型能力探测。
    pub const DEPLOYMENT_MODEL_PROBE: Self = Self(7);
    /// 与真实请求同部署的远程精确计数，不允许 fallback。
    pub const EXACT_UPSTREAM_TOKEN_COUNT: Self = Self(8);
    /// 普通模型对话。
    pub const CONVERSATION: Self = Self(9);
    /// 独立 RoutePolicy 的辅助推理。
    pub const AUXILIARY_INFERENCE: Self = Self(10);
    /// 从原始数值构造操作标识；未知值会在分类时默认拒绝。
    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    /// 返回原始操作标识。
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// 请求的语义类别，由内置规格决定，不能由请求调用方声明。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestKind {
    /// 传输层存活探针。
    TransportControl,
    /// 本地管理面操作。
    LocalAdmin,
    /// 从本地快照合成的统一模型目录。
    UnifiedModelCatalog,
    /// 本地 tokenizer 精确计数。
    ExactLocalTokenCount,
    /// 本地估算计数。
    EstimatedLocalTokenCount,
    /// 纯本地 Context compact。
    LocalContextCompact,
    /// 固定单部署、无正文的管理模型探测。
    DeploymentModelProbe,
    /// 与真实请求同部署的远程精确计数。
    ExactUpstreamTokenCount,
    /// 普通模型推理。
    ModelInference,
    /// 独立 RoutePolicy 的辅助推理。
    AuxiliaryInference,
}

/// 请求的执行域，由内置规格决定，不能由请求调用方声明。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestDispatchDomain {
    /// 只交给本地 handler。
    Local,
    /// 绑定单一模型部署，不允许 A→B fallback。
    BoundDeployment,
    /// 使用明确的路由快照和 RoutePlan。
    RoutedPolicy,
}

/// 一个不含正文的固定路由规格。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RouteSpec {
    operation: OperationId,
    kind: RequestKind,
    domain: RequestDispatchDomain,
}

impl RouteSpec {
    const fn new(operation: OperationId, kind: RequestKind, domain: RequestDispatchDomain) -> Self {
        Self {
            operation,
            kind,
            domain,
        }
    }

    /// 返回操作标识。
    pub const fn operation(self) -> OperationId {
        self.operation
    }

    /// 返回由规格决定的请求类别。
    pub const fn kind(self) -> RequestKind {
        self.kind
    }

    /// 返回由规格决定的执行域。
    pub const fn domain(self) -> RequestDispatchDomain {
        self.domain
    }

    /// 查找内置规格；未知操作返回 `None`。
    pub const fn lookup(operation: OperationId) -> Option<Self> {
        match operation.get() {
            1 => Some(Self::new(
                OperationId::LIVENESS,
                RequestKind::TransportControl,
                RequestDispatchDomain::Local,
            )),
            2 => Some(Self::new(
                OperationId::STATUS,
                RequestKind::LocalAdmin,
                RequestDispatchDomain::Local,
            )),
            3 => Some(Self::new(
                OperationId::UNIFIED_MODEL_CATALOG,
                RequestKind::UnifiedModelCatalog,
                RequestDispatchDomain::Local,
            )),
            4 => Some(Self::new(
                OperationId::EXACT_LOCAL_TOKEN_COUNT,
                RequestKind::ExactLocalTokenCount,
                RequestDispatchDomain::Local,
            )),
            5 => Some(Self::new(
                OperationId::ESTIMATED_LOCAL_TOKEN_COUNT,
                RequestKind::EstimatedLocalTokenCount,
                RequestDispatchDomain::Local,
            )),
            6 => Some(Self::new(
                OperationId::LOCAL_CONTEXT_COMPACT,
                RequestKind::LocalContextCompact,
                RequestDispatchDomain::Local,
            )),
            7 => Some(Self::new(
                OperationId::DEPLOYMENT_MODEL_PROBE,
                RequestKind::DeploymentModelProbe,
                RequestDispatchDomain::BoundDeployment,
            )),
            8 => Some(Self::new(
                OperationId::EXACT_UPSTREAM_TOKEN_COUNT,
                RequestKind::ExactUpstreamTokenCount,
                RequestDispatchDomain::BoundDeployment,
            )),
            9 => Some(Self::new(
                OperationId::CONVERSATION,
                RequestKind::ModelInference,
                RequestDispatchDomain::RoutedPolicy,
            )),
            10 => Some(Self::new(
                OperationId::AUXILIARY_INFERENCE,
                RequestKind::AuxiliaryInference,
                RequestDispatchDomain::RoutedPolicy,
            )),
            _ => None,
        }
    }
}

/// 进入分类器的最小请求描述，不包含正文或认证材料。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IngressRequest {
    operation: OperationId,
    snapshot: Option<SnapshotVersion>,
    deployment: Option<ModelDeploymentId>,
}

impl IngressRequest {
    /// 创建不带绑定上下文的原始入站请求。
    pub const fn new(operation: OperationId) -> Self {
        Self {
            operation,
            snapshot: None,
            deployment: None,
        }
    }

    /// 创建带路由快照版本的原始请求。
    pub const fn routed(operation: OperationId, snapshot: SnapshotVersion) -> Self {
        Self {
            operation,
            snapshot: Some(snapshot),
            deployment: None,
        }
    }

    /// 创建带指定模型部署的原始请求。
    pub const fn bound_deployment(operation: OperationId, deployment: ModelDeploymentId) -> Self {
        Self {
            operation,
            snapshot: None,
            deployment: Some(deployment),
        }
    }

    /// 返回原始操作标识。
    pub const fn operation(self) -> OperationId {
        self.operation
    }
}

/// 入站分类失败原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClassifyError {
    /// 操作未在内置闭集中注册。
    UnknownOperation,
    /// 路由操作缺少快照版本。
    MissingSnapshot,
    /// 非路由操作携带了快照版本。
    UnexpectedSnapshot,
    /// 指定部署操作缺少部署标识。
    MissingDeployment,
    /// 非指定部署操作携带了部署标识。
    UnexpectedDeployment,
}

/// 已验证的本地分发结果；字段不能由外部直接构造。
pub struct VerifiedLocalDispatch {
    operation: OperationId,
    kind: RequestKind,
}

impl VerifiedLocalDispatch {
    /// 返回已验证操作。
    pub const fn operation(&self) -> OperationId {
        self.operation
    }

    /// 返回已验证类别。
    pub const fn kind(&self) -> RequestKind {
        self.kind
    }
}

/// 已验证的单部署分发结果；字段不能由外部直接构造。
pub struct VerifiedBoundDeploymentDispatch {
    operation: OperationId,
    deployment: ModelDeploymentId,
}

impl VerifiedBoundDeploymentDispatch {
    /// 返回已验证操作。
    pub const fn operation(&self) -> OperationId {
        self.operation
    }

    /// 返回已验证模型部署。
    pub const fn deployment(&self) -> ModelDeploymentId {
        self.deployment
    }
}

/// 已验证的普通路由分发结果；字段不能由外部直接构造。
pub struct VerifiedRouteDispatch {
    operation: OperationId,
    snapshot: SnapshotVersion,
}

impl VerifiedRouteDispatch {
    /// 返回已验证操作。
    pub const fn operation(&self) -> OperationId {
        self.operation
    }

    /// 返回已验证快照版本。
    pub const fn snapshot(&self) -> SnapshotVersion {
        self.snapshot
    }
}

/// 默认拒绝分类器的唯一成功结果。
///
/// 此类型只证明请求通过了闭集 RouteSpec 与绑定形状检查；并不表示已完成
/// 入站鉴权、ContextAttestation 或 Secret 授权。
pub enum VerifiedIngressDisposition {
    /// 本地 handler 分发。
    Local(VerifiedLocalDispatch),
    /// 指定模型部署分发。
    BoundDeployment(VerifiedBoundDeploymentDispatch),
    /// 普通路由计划分发。
    Routed(VerifiedRouteDispatch),
}

/// 无状态、闭集、默认拒绝的入站分类器。
pub struct IngressClassifier;

impl IngressClassifier {
    /// 创建分类器。
    pub const fn new() -> Self {
        Self
    }

    /// 按内置规格分类请求；未知操作或错误绑定都会拒绝。
    pub fn classify(
        &self,
        request: IngressRequest,
    ) -> Result<VerifiedIngressDisposition, ClassifyError> {
        let Some(spec) = RouteSpec::lookup(request.operation) else {
            return Err(ClassifyError::UnknownOperation);
        };

        match spec.domain {
            RequestDispatchDomain::Local => {
                if request.snapshot.is_some() {
                    return Err(ClassifyError::UnexpectedSnapshot);
                }
                if request.deployment.is_some() {
                    return Err(ClassifyError::UnexpectedDeployment);
                }
                Ok(VerifiedIngressDisposition::Local(VerifiedLocalDispatch {
                    operation: request.operation,
                    kind: spec.kind,
                }))
            }
            RequestDispatchDomain::BoundDeployment => {
                if request.snapshot.is_some() {
                    return Err(ClassifyError::UnexpectedSnapshot);
                }
                let Some(deployment) = request.deployment else {
                    return Err(ClassifyError::MissingDeployment);
                };
                Ok(VerifiedIngressDisposition::BoundDeployment(
                    VerifiedBoundDeploymentDispatch {
                        operation: request.operation,
                        deployment,
                    },
                ))
            }
            RequestDispatchDomain::RoutedPolicy => {
                if request.deployment.is_some() {
                    return Err(ClassifyError::UnexpectedDeployment);
                }
                let Some(snapshot) = request.snapshot else {
                    return Err(ClassifyError::MissingSnapshot);
                };
                Ok(VerifiedIngressDisposition::Routed(VerifiedRouteDispatch {
                    operation: request.operation,
                    snapshot,
                }))
            }
        }
    }
}

impl Default for IngressClassifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> SnapshotVersion {
        SnapshotVersion::new(1).expect("测试快照非零")
    }

    fn deployment() -> ModelDeploymentId {
        ModelDeploymentId::new(2).expect("测试部署非零")
    }

    #[test]
    fn builtin_specs_are_closed_and_explicit() {
        let spec = RouteSpec::lookup(OperationId::CONVERSATION).expect("对话已注册");
        assert_eq!(spec.kind(), RequestKind::ModelInference);
        assert_eq!(spec.domain(), RequestDispatchDomain::RoutedPolicy);
        let exact =
            RouteSpec::lookup(OperationId::EXACT_UPSTREAM_TOKEN_COUNT).expect("远程精确计数已注册");
        assert_eq!(exact.kind(), RequestKind::ExactUpstreamTokenCount);
        assert_eq!(exact.domain(), RequestDispatchDomain::BoundDeployment);
        assert_eq!(MAX_ROUTE_SPECS, 10);
        assert!(RouteSpec::lookup(OperationId::new(99)).is_none());
    }

    #[test]
    fn local_operation_cannot_carry_route_binding() {
        let classifier = IngressClassifier::new();
        assert!(matches!(
            classifier.classify(IngressRequest::new(OperationId::STATUS)),
            Ok(VerifiedIngressDisposition::Local(_))
        ));
        assert!(matches!(
            classifier.classify(IngressRequest::routed(OperationId::STATUS, snapshot())),
            Err(ClassifyError::UnexpectedSnapshot)
        ));
    }

    #[test]
    fn routed_operation_requires_snapshot_and_rejects_deployment_binding() {
        let classifier = IngressClassifier::new();
        assert!(matches!(
            classifier.classify(IngressRequest::new(OperationId::CONVERSATION)),
            Err(ClassifyError::MissingSnapshot)
        ));
        assert!(matches!(
            classifier.classify(IngressRequest::bound_deployment(
                OperationId::CONVERSATION,
                deployment()
            )),
            Err(ClassifyError::UnexpectedDeployment)
        ));
        assert!(matches!(
            classifier.classify(IngressRequest::routed(
                OperationId::CONVERSATION,
                snapshot()
            )),
            Ok(VerifiedIngressDisposition::Routed(_))
        ));
    }

    #[test]
    fn bound_deployment_operation_requires_exact_binding() {
        let classifier = IngressClassifier::new();
        assert!(matches!(
            classifier.classify(IngressRequest::new(OperationId::DEPLOYMENT_MODEL_PROBE)),
            Err(ClassifyError::MissingDeployment)
        ));
        assert!(matches!(
            classifier.classify(IngressRequest::bound_deployment(
                OperationId::DEPLOYMENT_MODEL_PROBE,
                deployment()
            )),
            Ok(VerifiedIngressDisposition::BoundDeployment(_))
        ));
        assert!(matches!(
            classifier.classify(IngressRequest::bound_deployment(
                OperationId::EXACT_UPSTREAM_TOKEN_COUNT,
                deployment()
            )),
            Ok(VerifiedIngressDisposition::BoundDeployment(_))
        ));
    }

    #[test]
    fn runtime_source_stays_bounded() {
        let runtime = include_str!("ingress.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("存在运行时代码");
        let code_lines = runtime
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            })
            .count();
        assert!(
            code_lines <= 280,
            "入站分类器运行时代码过大: {code_lines} 行"
        );
    }

    #[test]
    fn unknown_operation_is_default_denied() {
        let classifier = IngressClassifier::new();
        assert!(matches!(
            classifier.classify(IngressRequest::new(OperationId::new(255))),
            Err(ClassifyError::UnknownOperation)
        ));
        assert!(RouteSpec::lookup(OperationId::new(11)).is_none());
    }
}
