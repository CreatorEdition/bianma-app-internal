//! routing v2 无 Secret 迁移恢复合同。
//!
//! 本模块只定义进程内的四阶段 Saga 代数；它不读取旧 Provider、不会构造或保存
//! Secret，也不接入 SQLite、keyring、IPC、Proxy 或网络。真实持久化和 Vault 必须在
//! 后续独立切片中实现，并且只能复用本模块的单调阶段与封闭失败码。

/// 迁移项的内部稳定标识。
///
/// 正常构建不提供构造入口，避免尚未接入持久化来源前出现可运行的生产迁移路径。
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct MigrationItemId(u64);

impl MigrationItemId {
    /// 为当前测试构造无业务含义的稳定标识。
    #[cfg(test)]
    fn for_test(value: u64) -> Self {
        Self(value)
    }
}

/// 仅能被 Saga port 传递的不透明 Vault 引用。
///
/// 它不实现 `Debug`、`Display`、serde 或原始值 getter；本模块也不提供正常构建的
/// 构造入口。该引用不是 Secret，不能据此读取或物化任何凭据。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct OpaqueSecretRef(u128);

impl OpaqueSecretRef {
    /// 在不暴露内容的前提下比较两个不透明引用。
    fn matches(&self, other: &Self) -> bool {
        self == other
    }

    /// 仅为测试构造不可观测的引用值。
    #[cfg(test)]
    fn for_test(value: u128) -> Self {
        Self(value)
    }
}

/// 迁移 checkpoint 的可观察阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationPhase {
    Discovered,
    VaultWritten,
    VaultVerified,
    MetadataCommitted,
    Quarantined,
}

/// 所有可返回给上层的封闭失败码。
///
/// 此枚举不携带 backend 错误、来源名称、Vault 引用或任何 Secret 材料。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationFailureCode {
    Interrupted,
    InvalidTransition,
    VaultWriteFailed,
    VaultVerificationFailed,
    VaultReferenceMismatch,
    MetadataCommitFailed,
    MetadataReferenceMismatch,
}

/// 单次恢复调用的非敏感结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MigrationResumeOutcome {
    Completed,
    Pending(MigrationFailureCode),
    Quarantined(MigrationFailureCode),
}

/// metadata port 对幂等提交的确认。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MetadataCommitDisposition {
    Committed,
    AlreadyCommitted,
}

/// metadata port 的封闭错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MetadataCommitError {
    Failed,
    ReferenceMismatch,
}

/// Vault 写入与读回校验的最小抽象。
///
/// 正常构建没有实现者。未来实现必须将真实材料保留在受控 Vault 边界内，只向 Saga
/// 返回不透明引用，并保证同一迁移项重复写入时幂等。
pub(crate) trait MigrationVaultPort {
    /// 为一个已发现迁移项写入材料并返回不透明引用。
    fn write_opaque_ref(
        &mut self,
        item_id: MigrationItemId,
    ) -> Result<OpaqueSecretRef, MigrationFailureCode>;

    /// 读回并校验同一迁移项的引用，返回实际被校验的引用。
    fn verify_opaque_ref(
        &mut self,
        item_id: MigrationItemId,
        secret_ref: &OpaqueSecretRef,
    ) -> Result<OpaqueSecretRef, MigrationFailureCode>;
}

/// 元数据提交的最小抽象。
///
/// port 必须按迁移项与不透明引用实现幂等语义；第二个 runner 只能收到
/// `AlreadyCommitted`，不能造成第二次逻辑提交。
pub(crate) trait MigrationMetadataPort {
    /// 提交与已验证引用关联的无 Secret 元数据。
    fn commit_metadata(
        &mut self,
        item_id: MigrationItemId,
        secret_ref: &OpaqueSecretRef,
    ) -> Result<MetadataCommitDisposition, MetadataCommitError>;
}

/// 内部 checkpoint，引用始终留在不透明类型中。
#[derive(Clone, Copy)]
enum MigrationCheckpoint {
    Discovered,
    VaultWritten(OpaqueSecretRef),
    VaultVerified(OpaqueSecretRef),
    MetadataCommitted,
    Quarantined(MigrationFailureCode),
}

/// 副作用前后的内部故障注入位置。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MigrationStep {
    BeforeVaultWrite,
    AfterVaultWrite,
    BeforeVaultVerify,
    AfterVaultVerify,
    BeforeMetadataCommit,
    AfterMetadataCommit,
}

/// 可恢复的单项迁移状态机。
///
/// 所有正常路径仅向前推进。外部副作用成功但 checkpoint 尚未更新时，下一次恢复会
/// 重试相同 port 调用；这要求 port 的写入、验证和 metadata 提交都保持幂等。
pub(crate) struct MigrationSaga {
    item_id: MigrationItemId,
    checkpoint: MigrationCheckpoint,
}

impl MigrationSaga {
    /// 以发现阶段创建新的迁移项。
    ///
    /// 调用方目前没有正常构建的 `MigrationItemId` 构造入口；这避免合同被误接入
    /// 尚不存在的生产迁移流程。
    pub(crate) fn discovered(item_id: MigrationItemId) -> Self {
        Self {
            item_id,
            checkpoint: MigrationCheckpoint::Discovered,
        }
    }

    /// 返回当前 checkpoint 阶段，不暴露不透明引用。
    pub(crate) fn phase(&self) -> MigrationPhase {
        match self.checkpoint {
            MigrationCheckpoint::Discovered => MigrationPhase::Discovered,
            MigrationCheckpoint::VaultWritten(_) => MigrationPhase::VaultWritten,
            MigrationCheckpoint::VaultVerified(_) => MigrationPhase::VaultVerified,
            MigrationCheckpoint::MetadataCommitted => MigrationPhase::MetadataCommitted,
            MigrationCheckpoint::Quarantined(_) => MigrationPhase::Quarantined,
        }
    }

    /// 返回隔离终态的封闭失败码。
    pub(crate) fn quarantine_code(&self) -> Option<MigrationFailureCode> {
        match self.checkpoint {
            MigrationCheckpoint::Quarantined(code) => Some(code),
            _ => None,
        }
    }

    /// 从最后一个内存 checkpoint 继续，直到完成、待重试或隔离。
    pub(crate) fn resume<V, M>(&mut self, vault: &mut V, metadata: &mut M) -> MigrationResumeOutcome
    where
        V: MigrationVaultPort,
        M: MigrationMetadataPort,
    {
        self.resume_with_hook(vault, metadata, |_| Ok(()))
    }

    /// 在副作用边界执行恢复；hook 仅服务于进程内故障建模。
    fn resume_with_hook<V, M, F>(
        &mut self,
        vault: &mut V,
        metadata: &mut M,
        mut hook: F,
    ) -> MigrationResumeOutcome
    where
        V: MigrationVaultPort,
        M: MigrationMetadataPort,
        F: FnMut(MigrationStep) -> Result<(), MigrationFailureCode>,
    {
        loop {
            match self.checkpoint {
                MigrationCheckpoint::Discovered => {
                    if let Err(code) = hook(MigrationStep::BeforeVaultWrite) {
                        return MigrationResumeOutcome::Pending(code);
                    }
                    let secret_ref = match vault.write_opaque_ref(self.item_id) {
                        Ok(secret_ref) => secret_ref,
                        Err(_) => {
                            return MigrationResumeOutcome::Pending(
                                MigrationFailureCode::VaultWriteFailed,
                            );
                        }
                    };
                    if let Err(code) = hook(MigrationStep::AfterVaultWrite) {
                        return MigrationResumeOutcome::Pending(code);
                    }
                    if let Err(code) = self.record_vault_written(secret_ref) {
                        return self.quarantine(code);
                    }
                }
                MigrationCheckpoint::VaultWritten(secret_ref) => {
                    if let Err(code) = hook(MigrationStep::BeforeVaultVerify) {
                        return MigrationResumeOutcome::Pending(code);
                    }
                    let verified_ref = match vault.verify_opaque_ref(self.item_id, &secret_ref) {
                        Ok(verified_ref) => verified_ref,
                        Err(_) => {
                            return self.quarantine(MigrationFailureCode::VaultVerificationFailed)
                        }
                    };
                    if !secret_ref.matches(&verified_ref) {
                        return self.quarantine(MigrationFailureCode::VaultReferenceMismatch);
                    }
                    if let Err(code) = hook(MigrationStep::AfterVaultVerify) {
                        return MigrationResumeOutcome::Pending(code);
                    }
                    if let Err(code) = self.record_vault_verified(verified_ref) {
                        return self.quarantine(code);
                    }
                }
                MigrationCheckpoint::VaultVerified(secret_ref) => {
                    if let Err(code) = hook(MigrationStep::BeforeMetadataCommit) {
                        return MigrationResumeOutcome::Pending(code);
                    }
                    match metadata.commit_metadata(self.item_id, &secret_ref) {
                        Ok(
                            MetadataCommitDisposition::Committed
                            | MetadataCommitDisposition::AlreadyCommitted,
                        ) => {}
                        Err(MetadataCommitError::Failed) => {
                            return MigrationResumeOutcome::Pending(
                                MigrationFailureCode::MetadataCommitFailed,
                            );
                        }
                        Err(MetadataCommitError::ReferenceMismatch) => {
                            return self
                                .quarantine(MigrationFailureCode::MetadataReferenceMismatch);
                        }
                    }
                    if let Err(code) = hook(MigrationStep::AfterMetadataCommit) {
                        return MigrationResumeOutcome::Pending(code);
                    }
                    if let Err(code) = self.record_metadata_committed() {
                        return self.quarantine(code);
                    }
                }
                MigrationCheckpoint::MetadataCommitted => return MigrationResumeOutcome::Completed,
                MigrationCheckpoint::Quarantined(code) => {
                    return MigrationResumeOutcome::Quarantined(code);
                }
            }
        }
    }

    /// 仅允许 discovered 记录写入不透明引用。
    fn record_vault_written(
        &mut self,
        secret_ref: OpaqueSecretRef,
    ) -> Result<(), MigrationFailureCode> {
        if !matches!(self.checkpoint, MigrationCheckpoint::Discovered) {
            return Err(MigrationFailureCode::InvalidTransition);
        }
        self.checkpoint = MigrationCheckpoint::VaultWritten(secret_ref);
        Ok(())
    }

    /// 仅允许与已写引用完全相同的验证结果进入 verified。
    fn record_vault_verified(
        &mut self,
        verified_ref: OpaqueSecretRef,
    ) -> Result<(), MigrationFailureCode> {
        let MigrationCheckpoint::VaultWritten(written_ref) = self.checkpoint else {
            return Err(MigrationFailureCode::InvalidTransition);
        };
        if !written_ref.matches(&verified_ref) {
            return Err(MigrationFailureCode::VaultReferenceMismatch);
        }
        self.checkpoint = MigrationCheckpoint::VaultVerified(verified_ref);
        Ok(())
    }

    /// 仅允许 verified checkpoint 在 metadata port 明确确认后完成。
    fn record_metadata_committed(&mut self) -> Result<(), MigrationFailureCode> {
        if !matches!(self.checkpoint, MigrationCheckpoint::VaultVerified(_)) {
            return Err(MigrationFailureCode::InvalidTransition);
        }
        self.checkpoint = MigrationCheckpoint::MetadataCommitted;
        Ok(())
    }

    /// 记录不可恢复的封闭隔离终态。
    fn quarantine(&mut self, code: MigrationFailureCode) -> MigrationResumeOutcome {
        self.checkpoint = MigrationCheckpoint::Quarantined(code);
        MigrationResumeOutcome::Quarantined(code)
    }

    /// 为测试构造任意已持久化的内存 checkpoint。
    #[cfg(test)]
    fn with_test_checkpoint(item_id: MigrationItemId, checkpoint: MigrationCheckpoint) -> Self {
        Self {
            item_id,
            checkpoint,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MetadataCommitDisposition, MetadataCommitError, MigrationCheckpoint, MigrationFailureCode,
        MigrationItemId, MigrationMetadataPort, MigrationPhase, MigrationResumeOutcome,
        MigrationSaga, MigrationStep, MigrationVaultPort, OpaqueSecretRef,
    };
    use std::collections::BTreeMap;

    /// 无 Secret 的测试 Vault：只记录不透明引用及副作用次数。
    #[derive(Default)]
    struct FakeVault {
        refs: BTreeMap<u64, OpaqueSecretRef>,
        write_effects: usize,
        verify_effects: usize,
        fail_next_write: bool,
        fail_next_verify: bool,
        return_mismatched_ref: bool,
    }

    impl FakeVault {
        /// 为恢复测试预置一个已存在的不透明引用。
        fn seed(&mut self, item_id: MigrationItemId, secret_ref: OpaqueSecretRef) {
            self.refs.insert(item_id.0, secret_ref);
        }
    }

    impl MigrationVaultPort for FakeVault {
        fn write_opaque_ref(
            &mut self,
            item_id: MigrationItemId,
        ) -> Result<OpaqueSecretRef, MigrationFailureCode> {
            if self.fail_next_write {
                self.fail_next_write = false;
                return Err(MigrationFailureCode::VaultWriteFailed);
            }
            if let Some(secret_ref) = self.refs.get(&item_id.0).copied() {
                return Ok(secret_ref);
            }
            self.write_effects += 1;
            let secret_ref = OpaqueSecretRef::for_test(u128::from(item_id.0) + 1);
            self.refs.insert(item_id.0, secret_ref);
            Ok(secret_ref)
        }

        fn verify_opaque_ref(
            &mut self,
            item_id: MigrationItemId,
            secret_ref: &OpaqueSecretRef,
        ) -> Result<OpaqueSecretRef, MigrationFailureCode> {
            if self.fail_next_verify {
                self.fail_next_verify = false;
                return Err(MigrationFailureCode::VaultVerificationFailed);
            }
            self.verify_effects += 1;
            if self.return_mismatched_ref {
                return Ok(OpaqueSecretRef::for_test(u128::from(item_id.0) + 1000));
            }
            let stored_ref = self
                .refs
                .get(&item_id.0)
                .copied()
                .ok_or(MigrationFailureCode::VaultVerificationFailed)?;
            if !stored_ref.matches(secret_ref) {
                return Ok(OpaqueSecretRef::for_test(u128::from(item_id.0) + 2000));
            }
            Ok(stored_ref)
        }
    }

    /// 无 Secret 的测试 metadata port：同一 item/ref 只产生一次逻辑提交。
    #[derive(Default)]
    struct FakeMetadata {
        committed: BTreeMap<u64, OpaqueSecretRef>,
        commit_effects: usize,
        fail_next_commit: bool,
        reject_ref: bool,
    }

    impl MigrationMetadataPort for FakeMetadata {
        fn commit_metadata(
            &mut self,
            item_id: MigrationItemId,
            secret_ref: &OpaqueSecretRef,
        ) -> Result<MetadataCommitDisposition, MetadataCommitError> {
            if self.fail_next_commit {
                self.fail_next_commit = false;
                return Err(MetadataCommitError::Failed);
            }
            if self.reject_ref {
                return Err(MetadataCommitError::ReferenceMismatch);
            }
            match self.committed.get(&item_id.0) {
                Some(existing_ref) if existing_ref.matches(secret_ref) => {
                    Ok(MetadataCommitDisposition::AlreadyCommitted)
                }
                Some(_) => Err(MetadataCommitError::ReferenceMismatch),
                None => {
                    self.committed.insert(item_id.0, *secret_ref);
                    self.commit_effects += 1;
                    Ok(MetadataCommitDisposition::Committed)
                }
            }
        }
    }

    /// 取得只含测试标识的初始 Saga。
    fn discovered_saga(value: u64) -> MigrationSaga {
        MigrationSaga::discovered(MigrationItemId::for_test(value))
    }

    /// 取得需要继续验证的 checkpoint 与匹配的 fake Vault。
    fn written_saga(value: u64) -> (MigrationSaga, FakeVault) {
        let item_id = MigrationItemId::for_test(value);
        let secret_ref = OpaqueSecretRef::for_test(u128::from(value) + 1);
        let mut vault = FakeVault::default();
        vault.seed(item_id, secret_ref);
        (
            MigrationSaga::with_test_checkpoint(
                item_id,
                MigrationCheckpoint::VaultWritten(secret_ref),
            ),
            vault,
        )
    }

    /// 取得需要继续提交 metadata 的 checkpoint 与匹配的 fake Vault。
    fn verified_saga(value: u64) -> (MigrationSaga, FakeVault) {
        let item_id = MigrationItemId::for_test(value);
        let secret_ref = OpaqueSecretRef::for_test(u128::from(value) + 1);
        let mut vault = FakeVault::default();
        vault.seed(item_id, secret_ref);
        (
            MigrationSaga::with_test_checkpoint(
                item_id,
                MigrationCheckpoint::VaultVerified(secret_ref),
            ),
            vault,
        )
    }

    #[test]
    fn valid_migration_advances_monotonically_and_commits_once() {
        let mut saga = discovered_saga(1);
        let mut vault = FakeVault::default();
        let mut metadata = FakeMetadata::default();

        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Completed
        );
        assert_eq!(saga.phase(), MigrationPhase::MetadataCommitted);
        assert_eq!(vault.write_effects, 1);
        assert_eq!(vault.verify_effects, 1);
        assert_eq!(metadata.commit_effects, 1);

        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Completed
        );
        assert_eq!(metadata.commit_effects, 1);
    }

    #[test]
    fn invalid_transition_cannot_skip_to_metadata_commit() {
        let mut saga = discovered_saga(2);

        assert_eq!(
            saga.record_metadata_committed(),
            Err(MigrationFailureCode::InvalidTransition)
        );
        assert_eq!(saga.phase(), MigrationPhase::Discovered);
    }

    #[test]
    fn verification_failure_quarantines_and_never_retries_side_effects() {
        let mut saga = discovered_saga(3);
        let mut vault = FakeVault {
            fail_next_verify: true,
            ..Default::default()
        };
        let mut metadata = FakeMetadata::default();

        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Quarantined(MigrationFailureCode::VaultVerificationFailed)
        );
        assert_eq!(saga.phase(), MigrationPhase::Quarantined);
        assert_eq!(
            saga.quarantine_code(),
            Some(MigrationFailureCode::VaultVerificationFailed)
        );
        assert_eq!(metadata.commit_effects, 0);
        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Quarantined(MigrationFailureCode::VaultVerificationFailed)
        );
        assert_eq!(metadata.commit_effects, 0);
    }

    #[test]
    fn mismatched_verified_ref_quarantines_before_metadata_commit() {
        let mut saga = discovered_saga(4);
        let mut vault = FakeVault {
            return_mismatched_ref: true,
            ..Default::default()
        };
        let mut metadata = FakeMetadata::default();

        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Quarantined(MigrationFailureCode::VaultReferenceMismatch)
        );
        assert_eq!(saga.phase(), MigrationPhase::Quarantined);
        assert_eq!(metadata.commit_effects, 0);
    }

    #[test]
    fn metadata_ref_rejection_quarantines_after_verified_checkpoint() {
        let (mut saga, mut vault) = verified_saga(5);
        let mut metadata = FakeMetadata {
            reject_ref: true,
            ..Default::default()
        };

        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Quarantined(MigrationFailureCode::MetadataReferenceMismatch)
        );
        assert_eq!(saga.phase(), MigrationPhase::Quarantined);
    }

    #[test]
    fn failures_before_each_side_effect_resume_from_the_last_checkpoint() {
        let cases = [
            (MigrationStep::BeforeVaultWrite, MigrationPhase::Discovered),
            (
                MigrationStep::BeforeVaultVerify,
                MigrationPhase::VaultWritten,
            ),
            (
                MigrationStep::BeforeMetadataCommit,
                MigrationPhase::VaultVerified,
            ),
        ];

        for (index, (fault, expected_phase)) in cases.into_iter().enumerate() {
            let value = 100 + index as u64;
            let (mut saga, mut vault) = match fault {
                MigrationStep::BeforeVaultWrite => (discovered_saga(value), FakeVault::default()),
                MigrationStep::BeforeVaultVerify => written_saga(value),
                MigrationStep::BeforeMetadataCommit => verified_saga(value),
                _ => unreachable!("测试表只包含副作用前故障点"),
            };
            let mut metadata = FakeMetadata::default();

            assert_eq!(
                saga.resume_with_hook(&mut vault, &mut metadata, |point| {
                    if point == fault {
                        Err(MigrationFailureCode::Interrupted)
                    } else {
                        Ok(())
                    }
                }),
                MigrationResumeOutcome::Pending(MigrationFailureCode::Interrupted)
            );
            assert_eq!(saga.phase(), expected_phase);
            assert_eq!(
                saga.resume(&mut vault, &mut metadata),
                MigrationResumeOutcome::Completed
            );
            assert_eq!(metadata.commit_effects, 1);
        }
    }

    #[test]
    fn failures_after_each_side_effect_resume_idempotently() {
        let cases = [
            (MigrationStep::AfterVaultWrite, MigrationPhase::Discovered),
            (
                MigrationStep::AfterVaultVerify,
                MigrationPhase::VaultWritten,
            ),
            (
                MigrationStep::AfterMetadataCommit,
                MigrationPhase::VaultVerified,
            ),
        ];

        for (index, (fault, expected_phase)) in cases.into_iter().enumerate() {
            let value = 200 + index as u64;
            let (mut saga, mut vault) = match fault {
                MigrationStep::AfterVaultWrite => (discovered_saga(value), FakeVault::default()),
                MigrationStep::AfterVaultVerify => written_saga(value),
                MigrationStep::AfterMetadataCommit => verified_saga(value),
                _ => unreachable!("测试表只包含副作用后故障点"),
            };
            let mut metadata = FakeMetadata::default();

            assert_eq!(
                saga.resume_with_hook(&mut vault, &mut metadata, |point| {
                    if point == fault {
                        Err(MigrationFailureCode::Interrupted)
                    } else {
                        Ok(())
                    }
                }),
                MigrationResumeOutcome::Pending(MigrationFailureCode::Interrupted)
            );
            assert_eq!(saga.phase(), expected_phase);
            assert_eq!(
                saga.resume(&mut vault, &mut metadata),
                MigrationResumeOutcome::Completed
            );
            assert_eq!(metadata.commit_effects, 1);
            if fault == MigrationStep::AfterVaultWrite {
                assert_eq!(vault.write_effects, 1, "写入重试必须由 port 幂等收敛");
            }
        }
    }

    #[test]
    fn metadata_commit_failure_keeps_verified_checkpoint_for_retry() {
        let (mut saga, mut vault) = verified_saga(300);
        let mut metadata = FakeMetadata {
            fail_next_commit: true,
            ..Default::default()
        };

        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Pending(MigrationFailureCode::MetadataCommitFailed)
        );
        assert_eq!(saga.phase(), MigrationPhase::VaultVerified);
        assert_eq!(
            saga.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Completed
        );
        assert_eq!(metadata.commit_effects, 1);
    }

    #[test]
    fn two_runners_share_one_logical_metadata_commit() {
        let item_id = MigrationItemId::for_test(400);
        let secret_ref = OpaqueSecretRef::for_test(401);
        let mut first = MigrationSaga::with_test_checkpoint(
            item_id,
            MigrationCheckpoint::VaultVerified(secret_ref),
        );
        let mut second = MigrationSaga::with_test_checkpoint(
            item_id,
            MigrationCheckpoint::VaultVerified(secret_ref),
        );
        let mut vault = FakeVault::default();
        vault.seed(item_id, secret_ref);
        let mut metadata = FakeMetadata::default();

        assert_eq!(
            first.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Completed
        );
        assert_eq!(
            second.resume(&mut vault, &mut metadata),
            MigrationResumeOutcome::Completed
        );
        assert_eq!(metadata.commit_effects, 1);
        assert_eq!(first.phase(), MigrationPhase::MetadataCommitted);
        assert_eq!(second.phase(), MigrationPhase::MetadataCommitted);
    }
}
