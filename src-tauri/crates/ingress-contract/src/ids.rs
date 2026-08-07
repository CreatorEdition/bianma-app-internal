//! 领域标识、修订号与不可日志化摘要。

macro_rules! numeric_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            /// 从宿主已经验证的数值构造标识。
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            /// 返回非敏感数值标识。
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

numeric_id!(OperationId, "已注册入站 Operation 的稳定标识。");
numeric_id!(ListenerId, "监听器实例的稳定标识。");
numeric_id!(IngressTokenScopeId, "入站 Token scope 的稳定标识。");
numeric_id!(AudienceId, "证明 audience 的稳定标识。");
numeric_id!(IssuerEpoch, "进程启动期证明签发 epoch。");
numeric_id!(RoutePolicyRevision, "RoutePolicy 快照修订号。");
numeric_id!(ConsentRevision, "本机用户 consent 修订号。");
numeric_id!(ClientFamilyId, "受管理客户端家族标识。");
numeric_id!(ClientVersion, "受管理客户端版本标识。");
numeric_id!(AdapterVersion, "客户端 Adapter 版本标识。");
numeric_id!(IngressSchemaVersion, "入站 schema 版本标识。");
numeric_id!(ContextPolicyVersion, "ContextPolicy 版本标识。");
numeric_id!(TransformOwnerId, "有损变换 owner 标识。");
numeric_id!(TransformOwnerVersion, "有损变换 owner 版本。");
numeric_id!(SiteId, "站点逻辑标识。");
numeric_id!(ModelDeploymentId, "模型部署逻辑标识。");
numeric_id!(EndpointId, "Endpoint 逻辑标识。");
numeric_id!(AccountSelectorId, "账户选择器逻辑标识。");
numeric_id!(AccountId, "站点账户逻辑标识。");
numeric_id!(CredentialId, "凭据逻辑标识；不包含 Secret。");
numeric_id!(AdapterContractRevision, "上游 Adapter 合同修订号。");
numeric_id!(CapabilityManagementScopeId, "管理能力 scope 标识。");
numeric_id!(ProtocolFrameRevision, "协议 frame schema 修订号。");
numeric_id!(ToolSchemaRevision, "工具 schema 修订号。");
numeric_id!(RetrievalSchemaRevision, "检索 schema 修订号。");
numeric_id!(HandleEpoch, "本地检索 handle epoch。");

macro_rules! digest_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Eq, Hash, PartialEq)]
        pub struct $name([u8; 32]);

        impl $name {
            /// 从固定长度摘要字节构造。调用方仍不能借此构造 Verified 类型。
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            /// 只读借用摘要；禁止将其直接写入普通日志或遥测。
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }
    };
}

digest_type!(RegistryDigest, "RouteSpec 注册表的版本化摘要。");
digest_type!(RequestDigest, "原始入站请求的域分离摘要。");
digest_type!(BodyDigest, "原始正文逐字节域分离摘要。");
digest_type!(SemanticHeadersDigest, "已允许语义 Header 的有序摘要。");
digest_type!(EnvelopeDigest, "ContextEnvelope 的域分离摘要。");
digest_type!(AuthorizationBundleDigest, "完整授权 bundle 的域分离摘要。");

/// 128-bit 一次性 nonce；全零值始终非法。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OneShotNonce([u8; 16]);

impl OneShotNonce {
    /// 构造一次性 nonce。
    pub fn from_bytes(bytes: [u8; 16]) -> Result<Self, crate::IngressReject> {
        if bytes == [0; 16] {
            return Err(crate::IngressReject::InvalidNonce);
        }
        Ok(Self(bytes))
    }

    /// 只读借用 nonce 字节。
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
