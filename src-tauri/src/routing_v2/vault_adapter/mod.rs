//! routing v2 的系统凭据库能力 PoC。
//!
//! 此模块只验证操作系统凭据库能否承载未来 Vault 的根材料；它不读取旧 Provider
//! 配置，不写 SQLite，不暴露 Tauri 命令，也不解析、迁移或发送任何用户 Secret。
//! 真正的 Vault、SecretResolver 和迁移 Saga 必须在本 PoC 通过独立审计后另行实现。

use keyring::Entry;
use thiserror::Error;

const VAULT_CAPABILITY_POC_SERVICE: &str = "com.creatoredition.bianma.routing-v2.vault-poc";

/// 系统凭据库能力探测的封闭失败码。
///
/// 故意不保存或展示底层错误文本，以免未来 Secret 操作将 OS 错误、账户标识或
/// 凭据材料带入日志、Tauri 返回值或崩溃报告。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum VaultCapabilityError {
    #[error("vault_unavailable")]
    Unavailable,
    #[error("vault_round_trip_failed")]
    RoundTripFailed,
    #[error("vault_cleanup_failed")]
    CleanupFailed,
}

/// 返回稳定失败码，而不泄漏系统凭据库的原始错误。
fn map_backend_result<T>(result: keyring::Result<T>) -> Result<T, VaultCapabilityError> {
    result.map_err(|_| VaultCapabilityError::Unavailable)
}

/// 仅探测本机系统凭据库是否能创建一个未持久化的 entry handle。
///
/// 此函数不写入任何凭据，也不提供读取、写入或删除真实 Secret 的生产 API。
pub(crate) fn check_platform_capability() -> Result<(), VaultCapabilityError> {
    map_backend_result(Entry::new(VAULT_CAPABILITY_POC_SERVICE, "capability-check")).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{check_platform_capability, VaultCapabilityError, VAULT_CAPABILITY_POC_SERVICE};
    use keyring::Entry;
    use uuid::Uuid;

    trait TestCredentialEntry {
        fn write_canary(&mut self, canary: &[u8]) -> Result<(), VaultCapabilityError>;
        fn read_canary(&mut self) -> Result<Vec<u8>, VaultCapabilityError>;
        fn delete_canary(&mut self) -> Result<(), VaultCapabilityError>;
    }

    /// 验证二进制 canary 的创建、读取与删除顺序；任何底层异常都收敛为固定码。
    fn verify_round_trip(
        entry: &mut impl TestCredentialEntry,
        canary: &[u8],
    ) -> Result<(), VaultCapabilityError> {
        entry.write_canary(canary)?;

        let read_result = entry.read_canary();
        let cleanup_result = entry.delete_canary();
        if cleanup_result.is_err() {
            return Err(VaultCapabilityError::CleanupFailed);
        }

        match read_result {
            Ok(value) if value == canary => Ok(()),
            _ => Err(VaultCapabilityError::RoundTripFailed),
        }
    }

    struct TestCanary(Vec<u8>);

    impl TestCanary {
        fn random() -> Self {
            Self(Uuid::new_v4().as_bytes().to_vec())
        }

        fn as_bytes(&self) -> &[u8] {
            &self.0
        }
    }

    impl Drop for TestCanary {
        fn drop(&mut self) {
            self.0.fill(0);
        }
    }

    struct NativeTestCredentialEntry {
        entry: Entry,
    }

    impl NativeTestCredentialEntry {
        fn new(account: &str) -> Result<Self, VaultCapabilityError> {
            Entry::new(VAULT_CAPABILITY_POC_SERVICE, account)
                .map(|entry| Self { entry })
                .map_err(|_| VaultCapabilityError::Unavailable)
        }
    }

    impl TestCredentialEntry for NativeTestCredentialEntry {
        fn write_canary(&mut self, canary: &[u8]) -> Result<(), VaultCapabilityError> {
            self.entry
                .set_secret(canary)
                .map_err(|_| VaultCapabilityError::Unavailable)
        }

        fn read_canary(&mut self) -> Result<Vec<u8>, VaultCapabilityError> {
            self.entry
                .get_secret()
                .map_err(|_| VaultCapabilityError::Unavailable)
        }

        fn delete_canary(&mut self) -> Result<(), VaultCapabilityError> {
            self.entry
                .delete_credential()
                .map_err(|_| VaultCapabilityError::CleanupFailed)
        }
    }

    impl Drop for NativeTestCredentialEntry {
        fn drop(&mut self) {
            let _ = self.entry.delete_credential();
        }
    }

    #[derive(Default)]
    struct FakeCredentialEntry {
        stored: Option<Vec<u8>>,
        fail_read: bool,
        fail_cleanup: bool,
        delete_count: usize,
    }

    impl TestCredentialEntry for FakeCredentialEntry {
        fn write_canary(&mut self, canary: &[u8]) -> Result<(), VaultCapabilityError> {
            self.stored = Some(canary.to_vec());
            Ok(())
        }

        fn read_canary(&mut self) -> Result<Vec<u8>, VaultCapabilityError> {
            if self.fail_read {
                return Err(VaultCapabilityError::Unavailable);
            }
            self.stored
                .clone()
                .ok_or(VaultCapabilityError::RoundTripFailed)
        }

        fn delete_canary(&mut self) -> Result<(), VaultCapabilityError> {
            self.delete_count += 1;
            if self.fail_cleanup {
                return Err(VaultCapabilityError::CleanupFailed);
            }
            self.stored = None;
            Ok(())
        }
    }

    #[test]
    fn capability_check_only_returns_a_stable_error_code() {
        match check_platform_capability() {
            Ok(()) | Err(VaultCapabilityError::Unavailable) => {}
            Err(other) => panic!("能力检查只能返回 vault_unavailable，实际为 {other}"),
        }
    }

    #[test]
    fn round_trip_requires_exact_binary_match_and_cleans_up() {
        let canary = TestCanary::random();
        let mut entry = FakeCredentialEntry::default();

        assert!(verify_round_trip(&mut entry, canary.as_bytes()).is_ok());
        assert!(entry.stored.is_none());
        assert_eq!(entry.delete_count, 1);
    }

    #[test]
    fn read_failure_is_fail_closed_but_still_cleans_up() {
        let canary = TestCanary::random();
        let mut entry = FakeCredentialEntry {
            fail_read: true,
            ..Default::default()
        };

        assert_eq!(
            verify_round_trip(&mut entry, canary.as_bytes()),
            Err(VaultCapabilityError::RoundTripFailed)
        );
        assert!(entry.stored.is_none());
        assert_eq!(entry.delete_count, 1);
    }

    #[test]
    fn cleanup_failure_overrides_read_result() {
        let canary = TestCanary::random();
        let mut entry = FakeCredentialEntry {
            fail_cleanup: true,
            ..Default::default()
        };

        assert_eq!(
            verify_round_trip(&mut entry, canary.as_bytes()),
            Err(VaultCapabilityError::CleanupFailed)
        );
        assert_eq!(entry.delete_count, 1);
    }

    #[test]
    #[ignore = "需要写入并立即删除本机随机凭据库 canary；请在受控设备上显式运行"]
    fn native_keyring_round_trip_writes_no_persistent_test_credential(
    ) -> Result<(), VaultCapabilityError> {
        let account = format!("poc-{}", Uuid::new_v4());
        let canary = TestCanary::random();
        let mut entry = NativeTestCredentialEntry::new(&account)?;

        verify_round_trip(&mut entry, canary.as_bytes())
    }
}
