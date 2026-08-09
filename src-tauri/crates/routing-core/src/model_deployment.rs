//! ModelDeployment 静态身份合同与有界目录。
//!
//! 本模块只验证 Deployment、Site、Endpoint 的静态关系，不包含模型能力、认证、URL、
//! User-Agent、请求头、Secret 或运行时状态。

use super::{EndpointId, ModelDeploymentId, SiteId, MAX_ROUTE_TARGETS};

/// 一个编译路由快照允许引用的最大模型部署数。
pub const MAX_MODEL_DEPLOYMENTS: usize = MAX_ROUTE_TARGETS;

/// 一个模型部署与其所属站点、端点的静态身份关系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelDeploymentDefinition {
    id: ModelDeploymentId,
    site: SiteId,
    endpoint: EndpointId,
}

impl ModelDeploymentDefinition {
    /// 构造模型部署的静态身份关系。
    pub const fn new(id: ModelDeploymentId, site: SiteId, endpoint: EndpointId) -> Self {
        Self { id, site, endpoint }
    }

    /// 返回模型部署稳定标识。
    pub const fn id(self) -> ModelDeploymentId {
        self.id
    }

    /// 返回模型部署所属站点。
    pub const fn site(self) -> SiteId {
        self.site
    }

    /// 返回模型部署所属上游端点。
    pub const fn endpoint(self) -> EndpointId {
        self.endpoint
    }
}

/// 构造不可变模型部署目录时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelDeploymentCatalogError {
    /// 目录没有任何模型部署定义。
    Empty,
    /// 目录超过固定上限。
    TooMany,
    /// 同一模型部署标识在目录中重复出现。
    DuplicateId,
}

/// 与一个已编译路由快照同代的模型部署静态目录。
///
/// 目录只借用经过验证的静态定义，使用有界线性查找；它不是模型注册中心，也不承担
/// 协议、能力、认证、健康、限流或任何运行时状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelDeploymentCatalog<'a> {
    deployments: &'a [ModelDeploymentDefinition],
}

impl<'a> ModelDeploymentCatalog<'a> {
    /// 验证并创建一个固定上限的模型部署目录。
    pub fn new(
        deployments: &'a [ModelDeploymentDefinition],
    ) -> Result<Self, ModelDeploymentCatalogError> {
        if deployments.is_empty() {
            return Err(ModelDeploymentCatalogError::Empty);
        }
        if deployments.len() > MAX_MODEL_DEPLOYMENTS {
            return Err(ModelDeploymentCatalogError::TooMany);
        }
        for (index, deployment) in deployments.iter().enumerate() {
            if deployments[..index]
                .iter()
                .any(|previous| previous.id == deployment.id)
            {
                return Err(ModelDeploymentCatalogError::DuplicateId);
            }
        }
        Ok(Self { deployments })
    }

    /// 返回目录中的模型部署定义数量。
    pub const fn len(&self) -> usize {
        self.deployments.len()
    }

    /// 返回目录是否为空；经由 [`Self::new`] 构造的目录始终返回 `false`。
    pub const fn is_empty(&self) -> bool {
        self.deployments.is_empty()
    }

    /// 根据稳定标识有界查找模型部署定义。
    pub(crate) fn get(&self, id: ModelDeploymentId) -> Option<&'a ModelDeploymentDefinition> {
        self.deployments
            .iter()
            .find(|deployment| deployment.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment(value: u64) -> ModelDeploymentDefinition {
        ModelDeploymentDefinition::new(
            ModelDeploymentId::new(value).expect("测试部署 ID 非零"),
            SiteId::new(value).expect("测试站点 ID 非零"),
            EndpointId::new(value).expect("测试端点 ID 非零"),
        )
    }

    #[test]
    fn catalog_rejects_empty_over_capacity_and_duplicate_ids() {
        let definition = deployment(1);
        let duplicate = [definition, definition];
        let too_many = [definition; MAX_MODEL_DEPLOYMENTS + 1];
        let single = [definition];

        assert_eq!(
            ModelDeploymentCatalog::new(&[]),
            Err(ModelDeploymentCatalogError::Empty)
        );
        assert_eq!(
            ModelDeploymentCatalog::new(&too_many),
            Err(ModelDeploymentCatalogError::TooMany)
        );
        assert_eq!(
            ModelDeploymentCatalog::new(&duplicate),
            Err(ModelDeploymentCatalogError::DuplicateId)
        );

        let catalog = ModelDeploymentCatalog::new(&single).expect("目录有效");
        assert_eq!(catalog.len(), 1);
        assert!(!catalog.is_empty());
        assert_eq!(
            catalog.get(ModelDeploymentId::new(1).expect("测试部署 ID 非零")),
            Some(&single[0])
        );
        assert_eq!(
            catalog.get(ModelDeploymentId::new(2).expect("测试部署 ID 非零")),
            None
        );
    }
}
