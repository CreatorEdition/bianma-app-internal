//! Account 与 Credential 静态归属合同及有界目录。
//!
//! 本模块只验证 Account 所属站点和 Credential 所属 Account，不包含 Secret、Origin、
//! AuthScheme、URL、请求头、User-Agent、授权状态或任何运行时状态。

use super::{AccountId, CredentialId, SiteId, MAX_ROUTE_TARGETS};

/// 一个编译路由快照允许引用的最大账户数。
pub const MAX_ACCOUNTS: usize = MAX_ROUTE_TARGETS;
/// 一个编译路由快照允许引用的最大凭据数。
pub const MAX_CREDENTIALS: usize = MAX_ROUTE_TARGETS;

/// 编译静态 Account/Credential 目录所需的原始定义。
///
/// 该值只将两份借用切片成对传入编译入口，不执行校验，也不包含 Secret、Origin、
/// AuthScheme 或运行时状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountCredentialDefinitions<'a> {
    accounts: &'a [AccountDefinition],
    credentials: &'a [CredentialDefinition],
}

impl<'a> AccountCredentialDefinitions<'a> {
    /// 构造供编译快照验证的静态定义对。
    pub const fn new(
        accounts: &'a [AccountDefinition],
        credentials: &'a [CredentialDefinition],
    ) -> Self {
        Self {
            accounts,
            credentials,
        }
    }

    /// 返回待验证的账户定义。
    pub(crate) const fn accounts(&self) -> &'a [AccountDefinition] {
        self.accounts
    }

    /// 返回待验证的凭据定义。
    pub(crate) const fn credentials(&self) -> &'a [CredentialDefinition] {
        self.credentials
    }
}

/// 一个账户与其所属站点的静态身份关系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountDefinition {
    id: AccountId,
    site: SiteId,
}

impl AccountDefinition {
    /// 构造账户的静态身份关系。
    pub const fn new(id: AccountId, site: SiteId) -> Self {
        Self { id, site }
    }

    /// 返回账户稳定标识。
    pub const fn id(self) -> AccountId {
        self.id
    }

    /// 返回账户所属站点。
    pub const fn site(self) -> SiteId {
        self.site
    }
}

/// 一个凭据与其所属账户的静态身份关系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialDefinition {
    id: CredentialId,
    account: AccountId,
}

impl CredentialDefinition {
    /// 构造凭据的静态归属关系。
    pub const fn new(id: CredentialId, account: AccountId) -> Self {
        Self { id, account }
    }

    /// 返回凭据稳定标识。
    pub const fn id(self) -> CredentialId {
        self.id
    }

    /// 返回凭据所属账户。
    pub const fn account(self) -> AccountId {
        self.account
    }
}

/// 构造不可变账户目录时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountCatalogError {
    /// 目录没有任何账户定义。
    Empty,
    /// 目录超过固定上限。
    TooMany,
    /// 同一账户标识在目录中重复出现。
    DuplicateId,
}

/// 构造不可变凭据目录时的拒绝原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialCatalogError {
    /// 目录没有任何凭据定义。
    Empty,
    /// 目录超过固定上限。
    TooMany,
    /// 同一凭据标识在目录中重复出现。
    DuplicateId,
    /// 凭据引用了账户目录中不存在的 owner。
    UnknownOwnerAccount,
}

/// 与一个已编译路由快照同代的账户静态目录。
///
/// 目录只借用经过验证的静态定义，使用有界线性查找；它不包含账户授权、健康、额度、
/// 租约或实际凭据内容。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccountCatalog<'a> {
    accounts: &'a [AccountDefinition],
}

impl<'a> AccountCatalog<'a> {
    /// 验证并创建一个固定上限的账户目录。
    pub fn new(accounts: &'a [AccountDefinition]) -> Result<Self, AccountCatalogError> {
        if accounts.is_empty() {
            return Err(AccountCatalogError::Empty);
        }
        if accounts.len() > MAX_ACCOUNTS {
            return Err(AccountCatalogError::TooMany);
        }
        for (index, account) in accounts.iter().enumerate() {
            if accounts[..index]
                .iter()
                .any(|previous| previous.id == account.id)
            {
                return Err(AccountCatalogError::DuplicateId);
            }
        }
        Ok(Self { accounts })
    }

    /// 返回目录中的账户定义数量。
    pub const fn len(&self) -> usize {
        self.accounts.len()
    }

    /// 返回目录是否为空；经由 [`Self::new`] 构造的目录始终返回 `false`。
    pub const fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// 根据稳定标识有界查找账户定义。
    pub(crate) fn get(&self, id: AccountId) -> Option<&'a AccountDefinition> {
        self.accounts.iter().find(|account| account.id == id)
    }
}

/// 与一个已编译路由快照同代的凭据静态目录。
///
/// 目录只借用经过验证的静态定义，使用有界线性查找；它不包含 Secret、AuthScheme、
/// Origin、授权状态或任何实际请求认证材料。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialCatalog<'a> {
    credentials: &'a [CredentialDefinition],
}

impl<'a> CredentialCatalog<'a> {
    /// 验证并创建一个固定上限的凭据目录，并拒绝不存在的账户 owner。
    pub fn new(
        credentials: &'a [CredentialDefinition],
        accounts: &AccountCatalog<'_>,
    ) -> Result<Self, CredentialCatalogError> {
        if credentials.is_empty() {
            return Err(CredentialCatalogError::Empty);
        }
        if credentials.len() > MAX_CREDENTIALS {
            return Err(CredentialCatalogError::TooMany);
        }
        for (index, credential) in credentials.iter().enumerate() {
            if credentials[..index]
                .iter()
                .any(|previous| previous.id == credential.id)
            {
                return Err(CredentialCatalogError::DuplicateId);
            }
            if accounts.get(credential.account).is_none() {
                return Err(CredentialCatalogError::UnknownOwnerAccount);
            }
        }
        Ok(Self { credentials })
    }

    /// 返回目录中的凭据定义数量。
    pub const fn len(&self) -> usize {
        self.credentials.len()
    }

    /// 返回目录是否为空；经由 [`Self::new`] 构造的目录始终返回 `false`。
    pub const fn is_empty(&self) -> bool {
        self.credentials.is_empty()
    }

    /// 根据稳定标识有界查找凭据定义。
    pub(crate) fn get(&self, id: CredentialId) -> Option<&'a CredentialDefinition> {
        self.credentials
            .iter()
            .find(|credential| credential.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(value: u64, site_value: u64) -> AccountDefinition {
        AccountDefinition::new(
            AccountId::new(value).expect("测试账户 ID 非零"),
            SiteId::new(site_value).expect("测试站点 ID 非零"),
        )
    }

    fn credential(value: u64, account_value: u64) -> CredentialDefinition {
        CredentialDefinition::new(
            CredentialId::new(value).expect("测试凭据 ID 非零"),
            AccountId::new(account_value).expect("测试账户 ID 非零"),
        )
    }

    #[test]
    fn account_catalog_rejects_invalid_shape_and_accepts_maximum() {
        let definition = account(1, 1);
        let duplicate = [definition, definition];
        let too_many = [definition; MAX_ACCOUNTS + 1];
        let maximum: [AccountDefinition; MAX_ACCOUNTS] =
            core::array::from_fn(|index| account((index + 1) as u64, 1));

        assert_eq!(AccountCatalog::new(&[]), Err(AccountCatalogError::Empty));
        assert_eq!(
            AccountCatalog::new(&too_many),
            Err(AccountCatalogError::TooMany)
        );
        assert_eq!(
            AccountCatalog::new(&duplicate),
            Err(AccountCatalogError::DuplicateId)
        );

        let catalog = AccountCatalog::new(&maximum).expect("最大目录有效");
        assert_eq!(catalog.len(), MAX_ACCOUNTS);
        assert!(!catalog.is_empty());
        assert_eq!(
            catalog.get(AccountId::new(16).expect("测试账户 ID 非零")),
            Some(&maximum[15])
        );
    }

    #[test]
    fn credential_catalog_rejects_invalid_shape_owner_and_accepts_maximum() {
        let accounts = [account(1, 1)];
        let account_catalog = AccountCatalog::new(&accounts).expect("账户目录有效");
        let definition = credential(1, 1);
        let duplicate = [definition, definition];
        let too_many = [definition; MAX_CREDENTIALS + 1];
        let unknown_owner = [credential(1, 2)];
        let maximum: [CredentialDefinition; MAX_CREDENTIALS] =
            core::array::from_fn(|index| credential((index + 1) as u64, 1));

        assert_eq!(
            CredentialCatalog::new(&[], &account_catalog),
            Err(CredentialCatalogError::Empty)
        );
        assert_eq!(
            CredentialCatalog::new(&too_many, &account_catalog),
            Err(CredentialCatalogError::TooMany)
        );
        assert_eq!(
            CredentialCatalog::new(&duplicate, &account_catalog),
            Err(CredentialCatalogError::DuplicateId)
        );
        assert_eq!(
            CredentialCatalog::new(&unknown_owner, &account_catalog),
            Err(CredentialCatalogError::UnknownOwnerAccount)
        );

        let catalog = CredentialCatalog::new(&maximum, &account_catalog).expect("最大目录有效");
        assert_eq!(catalog.len(), MAX_CREDENTIALS);
        assert!(!catalog.is_empty());
        assert_eq!(
            catalog.get(CredentialId::new(16).expect("测试凭据 ID 非零")),
            Some(&maximum[15])
        );
    }
}
