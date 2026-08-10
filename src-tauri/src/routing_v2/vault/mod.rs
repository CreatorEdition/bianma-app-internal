//! routing v2 的纯内存 AEAD 基础。
//!
//! 此模块只为后续受控 Vault 建立短生命周期根密钥与认证加密合同；它不读取真实
//! keyring、不持久化材料或密文，也不接入 SQLite、Saga、IPC、Proxy、HTTP 或网络。
//! `SealedPayload` 不是后续 S2 磁盘 envelope 协议。

mod envelope_v1;

use aes_gcm_siv::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng, Payload},
    Aes256GcmSiv, Nonce,
};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

pub(crate) use envelope_v1::{PersistentVaultContext, PersistentVaultEnvelopeV1};

const ROOT_KEY_LENGTH: usize = 32;
const NONCE_LENGTH: usize = 12;
const AEAD_TAG_LENGTH: usize = 16;
const MAX_PLAINTEXT_BYTES: usize = 16 * 1024;
const MAX_CIPHERTEXT_BYTES: usize = MAX_PLAINTEXT_BYTES + AEAD_TAG_LENGTH;
const SEALED_PAYLOAD_FORMAT_VERSION: u8 = 1;
const VAULT_AAD_DOMAIN: &[u8] = b"bianma.routing-v2.vault";
const AAD_FORMAT_VERSION: u8 = 1;
const AAD_ALGORITHM_ID_AES_256_GCM_SIV: u8 = 1;
const AAD_ROOT_KEY_SLOT: u8 = 1;
const VAULT_AAD_LENGTH: usize = VAULT_AAD_DOMAIN.len() + 4 + 16;

/// 仅包含稳定、无敏感信息的 Vault 加密失败码。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum VaultCryptoError {
    #[error("vault_unavailable")]
    VaultUnavailable,
    #[error("payload_too_large")]
    PayloadTooLarge,
    #[error("vault_seal_failed")]
    SealFailed,
    #[error("vault_open_failed")]
    OpenFailed,
    #[error("vault_envelope_malformed")]
    EnvelopeMalformed,
}

/// 短生命周期的 256-bit 根密钥句柄。
///
/// 它故意不实现 Clone、Copy、Debug、Display、serde 或字节 getter，避免根材料进入
/// 普通日志、DTO、备份或 WebView。未来受控 Port 只能加载既有材料，不能借此接口
/// 自动 provision、覆盖、轮换或恢复 root key。
pub(crate) struct RootKeyHandle {
    material: [u8; ROOT_KEY_LENGTH],
}

impl RootKeyHandle {
    /// 消费受控 loader 返回的既有根材料，立即复制到固定长度句柄并清零来源缓冲。
    ///
    /// 该入口不创建、截断、补齐或覆盖 root key；未来真实 keyring loader 缺失材料
    /// 时必须直接返回 `VaultUnavailable`，不能在此处静默 provision。
    pub(crate) fn from_loaded_material(
        mut material: Zeroizing<Vec<u8>>,
    ) -> Result<Self, VaultCryptoError> {
        if material.len() != ROOT_KEY_LENGTH {
            return Err(VaultCryptoError::VaultUnavailable);
        }

        let mut key = [0_u8; ROOT_KEY_LENGTH];
        key.copy_from_slice(&material);
        material.zeroize();
        Ok(Self { material: key })
    }

    /// 以内部 OS CSPRNG 生成一次性 nonce 后密封内存材料。
    pub(crate) fn seal(
        &self,
        aad: &VaultAad,
        plaintext: &[u8],
    ) -> Result<SealedPayload, VaultCryptoError> {
        let mut nonce_source = OsCsprngNonceSource;
        self.seal_with_nonce_source(aad, plaintext, &mut nonce_source)
    }

    /// 认证打开密文并将明文仅交给受控闭包消费。
    ///
    /// 正常 API 不返回裸 `Vec<u8>`，使后续 Vault 使用方必须显式决定明文的最小生命期。
    pub(crate) fn open<T>(
        &self,
        aad: &VaultAad,
        payload: &SealedPayload,
        consume: impl FnOnce(&[u8]) -> T,
    ) -> Result<T, VaultCryptoError> {
        if payload.format_version != SEALED_PAYLOAD_FORMAT_VERSION {
            return Err(VaultCryptoError::OpenFailed);
        }
        if payload.ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(VaultCryptoError::PayloadTooLarge);
        }
        if payload.ciphertext.len() < AEAD_TAG_LENGTH {
            return Err(VaultCryptoError::OpenFailed);
        }

        let cipher = self.cipher().map_err(|_| VaultCryptoError::OpenFailed)?;
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&payload.nonce),
                Payload {
                    msg: &payload.ciphertext,
                    aad: &aad.encode(),
                },
            )
            .map_err(|_| VaultCryptoError::OpenFailed)?;
        let plaintext = Zeroizing::new(plaintext);

        Ok(consume(plaintext.as_slice()))
    }

    /// 以受控设备本地上下文密封可持久化 envelope 的内存表示。
    ///
    /// 此入口只建立协议合同，不会执行文件、SQLite、keyring 或备份 I/O；root key 缺失、
    /// 不可用或未来无法认证的密文均必须由调用方失败关闭。
    pub(crate) fn seal_persistent(
        &self,
        context: &PersistentVaultContext,
        plaintext: &[u8],
    ) -> Result<PersistentVaultEnvelopeV1, VaultCryptoError> {
        let mut nonce_source = OsCsprngNonceSource;
        envelope_v1::seal_with_nonce_source(self, context, plaintext, &mut nonce_source)
    }

    /// 认证打开设备本地持久化 envelope，并只将明文交给受控闭包消费。
    ///
    /// 预期 record UUID 必须与 envelope 内已认证 header 相同；本入口不返回裸明文，
    /// 也不表示真实 Vault、备份或跨设备恢复已被接线。
    pub(crate) fn open_persistent<T>(
        &self,
        context: &PersistentVaultContext,
        envelope: &PersistentVaultEnvelopeV1,
        consume: impl FnOnce(&[u8]) -> T,
    ) -> Result<T, VaultCryptoError> {
        envelope_v1::open(self, context, envelope, consume)
    }

    /// 只保留在模块内，以便单测证明 nonce 由内部 source 生成。
    fn seal_with_nonce_source(
        &self,
        aad: &VaultAad,
        plaintext: &[u8],
        nonce_source: &mut impl NonceSource,
    ) -> Result<SealedPayload, VaultCryptoError> {
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(VaultCryptoError::PayloadTooLarge);
        }

        let mut nonce = [0_u8; NONCE_LENGTH];
        nonce_source.fill_nonce(&mut nonce)?;
        let cipher = self.cipher()?;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: &aad.encode(),
                },
            )
            .map_err(|_| VaultCryptoError::SealFailed)?;

        Ok(SealedPayload {
            format_version: SEALED_PAYLOAD_FORMAT_VERSION,
            nonce,
            ciphertext,
        })
    }

    fn cipher(&self) -> Result<Aes256GcmSiv, VaultCryptoError> {
        Aes256GcmSiv::new_from_slice(&self.material).map_err(|_| VaultCryptoError::SealFailed)
    }
}

impl Drop for RootKeyHandle {
    fn drop(&mut self) {
        self.material.zeroize();
    }
}

/// 固定的 AAD 用途，仅允许密封未来 Vault 中的材料。
#[derive(Clone, Copy)]
enum VaultPurpose {
    SealedMaterial = 1,
}

/// 防止密文在固定 Vault 上下文间被替换的关联数据。
///
/// 编码固定绑定 domain、AAD 版本、算法、root key slot、用途和非敏感 record UUID，
/// 不接收 schema、Provider 名称、URL 或 Secret 等可变业务数据。
#[derive(Clone, Copy)]
pub(crate) struct VaultAad {
    purpose: VaultPurpose,
    record_id: [u8; 16],
}

impl VaultAad {
    /// 为一个非敏感 record UUID 构造唯一允许的 S1 AAD。
    pub(crate) fn sealed_material(record_id: [u8; 16]) -> Self {
        Self {
            purpose: VaultPurpose::SealedMaterial,
            record_id,
        }
    }

    fn encode(&self) -> [u8; VAULT_AAD_LENGTH] {
        let mut encoded = [0_u8; VAULT_AAD_LENGTH];
        let domain_end = VAULT_AAD_DOMAIN.len();
        encoded[..domain_end].copy_from_slice(VAULT_AAD_DOMAIN);
        encoded[domain_end] = AAD_FORMAT_VERSION;
        encoded[domain_end + 1] = AAD_ALGORITHM_ID_AES_256_GCM_SIV;
        encoded[domain_end + 2] = AAD_ROOT_KEY_SLOT;
        encoded[domain_end + 3] = self.purpose as u8;
        encoded[domain_end + 4..].copy_from_slice(&self.record_id);
        encoded
    }
}

/// 只在进程内存在的 S1 密封结果。
///
/// 此类型故意不实现 Clone、Debug、Display 或 serde，且不是后续持久化格式。
pub(crate) struct SealedPayload {
    format_version: u8,
    nonce: [u8; NONCE_LENGTH],
    ciphertext: Vec<u8>,
}

impl Drop for SealedPayload {
    fn drop(&mut self) {
        self.nonce.zeroize();
        self.ciphertext.zeroize();
    }
}

/// 一次性 nonce 的内部来源；生产路径只允许 OS CSPRNG 实现。
trait NonceSource {
    fn fill_nonce(&mut self, nonce: &mut [u8; NONCE_LENGTH]) -> Result<(), VaultCryptoError>;
}

struct OsCsprngNonceSource;

impl NonceSource for OsCsprngNonceSource {
    fn fill_nonce(&mut self, nonce: &mut [u8; NONCE_LENGTH]) -> Result<(), VaultCryptoError> {
        let mut os_rng = OsRng;
        os_rng
            .try_fill_bytes(nonce)
            .map_err(|_| VaultCryptoError::SealFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NonceSource, RootKeyHandle, SealedPayload, VaultAad, VaultCryptoError, AEAD_TAG_LENGTH,
        MAX_CIPHERTEXT_BYTES, MAX_PLAINTEXT_BYTES, SEALED_PAYLOAD_FORMAT_VERSION,
    };
    use zeroize::Zeroizing;

    struct FixedNonceSource {
        nonce: [u8; 12],
        calls: usize,
        fail: bool,
    }

    impl FixedNonceSource {
        fn working(nonce: [u8; 12]) -> Self {
            Self {
                nonce,
                calls: 0,
                fail: false,
            }
        }

        fn failing() -> Self {
            Self {
                nonce: [0_u8; 12],
                calls: 0,
                fail: true,
            }
        }
    }

    impl NonceSource for FixedNonceSource {
        fn fill_nonce(&mut self, nonce: &mut [u8; 12]) -> Result<(), VaultCryptoError> {
            self.calls += 1;
            if self.fail {
                return Err(VaultCryptoError::SealFailed);
            }
            *nonce = self.nonce;
            Ok(())
        }
    }

    fn root_key(fill: u8) -> RootKeyHandle {
        RootKeyHandle::from_loaded_material(Zeroizing::new(vec![fill; 32]))
            .expect("测试根密钥长度固定为 32 byte")
    }

    fn aad(record_byte: u8) -> VaultAad {
        VaultAad::sealed_material([record_byte; 16])
    }

    fn sealed(root: &RootKeyHandle, aad: &VaultAad) -> SealedPayload {
        root.seal(aad, b"routing-v2-test-material")
            .expect("测试密封应成功")
    }

    fn assert_open_failed<T>(result: Result<T, VaultCryptoError>) {
        assert!(matches!(result, Err(VaultCryptoError::OpenFailed)));
    }

    #[test]
    fn root_key_requires_exactly_32_bytes() {
        for length in [0_usize, 31, 33] {
            assert!(matches!(
                RootKeyHandle::from_loaded_material(Zeroizing::new(vec![0_u8; length])),
                Err(VaultCryptoError::VaultUnavailable)
            ));
        }

        assert!(RootKeyHandle::from_loaded_material(Zeroizing::new(vec![0_u8; 32])).is_ok());
    }

    #[test]
    fn aad_encoding_is_fixed_and_versioned() {
        let record_id = [0xA5_u8; 16];
        let encoded = VaultAad::sealed_material(record_id).encode();
        let domain_end = super::VAULT_AAD_DOMAIN.len();

        assert_eq!(&encoded[..domain_end], super::VAULT_AAD_DOMAIN);
        assert_eq!(encoded[domain_end], super::AAD_FORMAT_VERSION);
        assert_eq!(
            encoded[domain_end + 1],
            super::AAD_ALGORITHM_ID_AES_256_GCM_SIV
        );
        assert_eq!(encoded[domain_end + 2], super::AAD_ROOT_KEY_SLOT);
        assert_eq!(
            encoded[domain_end + 3],
            super::VaultPurpose::SealedMaterial as u8
        );
        assert_eq!(&encoded[domain_end + 4..], &record_id);
    }

    #[test]
    fn round_trip_only_exposes_plaintext_to_the_consumer() {
        let root = root_key(7);
        let aad = aad(1);
        let payload = sealed(&root, &aad);

        assert_eq!(
            root.open(&aad, &payload, |plaintext| plaintext.to_vec()),
            Ok(b"routing-v2-test-material".to_vec())
        );
    }

    #[test]
    fn nonce_is_generated_inside_the_api_once_per_seal() {
        let root = root_key(8);
        let aad = aad(2);
        let mut nonce_source = FixedNonceSource::working([9_u8; 12]);

        let payload = root
            .seal_with_nonce_source(&aad, b"nonce-source-test", &mut nonce_source)
            .expect("固定 nonce source 应可用于模块内测试");

        assert_eq!(nonce_source.calls, 1);
        assert_eq!(
            root.open(&aad, &payload, |plaintext| plaintext.to_vec()),
            Ok(b"nonce-source-test".to_vec())
        );
    }

    #[test]
    fn sealing_has_a_hard_16_kib_limit() {
        let root = root_key(9);
        let aad = aad(3);

        assert!(root.seal(&aad, &[1_u8; MAX_PLAINTEXT_BYTES]).is_ok());
        assert!(matches!(
            root.seal(&aad, &[1_u8; MAX_PLAINTEXT_BYTES + 1]),
            Err(VaultCryptoError::PayloadTooLarge)
        ));
    }

    #[test]
    fn tampering_nonce_ciphertext_tag_or_aad_fails_closed() {
        let root = root_key(10);
        let aad = aad(4);

        let mut nonce_tampered = sealed(&root, &aad);
        nonce_tampered.nonce[0] ^= 1;
        assert_open_failed(root.open(&aad, &nonce_tampered, |_| ()));

        let mut ciphertext_tampered = sealed(&root, &aad);
        ciphertext_tampered.ciphertext[0] ^= 1;
        assert_open_failed(root.open(&aad, &ciphertext_tampered, |_| ()));

        let mut tag_tampered = sealed(&root, &aad);
        let tag_index = tag_tampered.ciphertext.len() - 1;
        tag_tampered.ciphertext[tag_index] ^= 1;
        assert_open_failed(root.open(&aad, &tag_tampered, |_| ()));

        let payload = sealed(&root, &aad);
        let mismatched_aad = VaultAad::sealed_material([5_u8; 16]);
        assert_open_failed(root.open(&mismatched_aad, &payload, |_| ()));
    }

    #[test]
    fn wrong_root_key_and_truncated_payload_fail_closed() {
        let root = root_key(11);
        let other_root = root_key(12);
        let aad = aad(6);
        let payload = sealed(&root, &aad);
        assert_open_failed(other_root.open(&aad, &payload, |_| ()));

        let mut truncated = sealed(&root, &aad);
        truncated.ciphertext.truncate(AEAD_TAG_LENGTH - 1);
        assert_open_failed(root.open(&aad, &truncated, |_| ()));
    }

    #[test]
    fn malformed_version_and_oversized_ciphertext_fail_closed() {
        let root = root_key(13);
        let aad = aad(7);

        let mut unknown_version = sealed(&root, &aad);
        unknown_version.format_version = SEALED_PAYLOAD_FORMAT_VERSION + 1;
        assert!(matches!(
            root.open(&aad, &unknown_version, |_| ()),
            Err(VaultCryptoError::OpenFailed)
        ));

        let oversized = SealedPayload {
            format_version: SEALED_PAYLOAD_FORMAT_VERSION,
            nonce: [0_u8; 12],
            ciphertext: vec![0_u8; MAX_CIPHERTEXT_BYTES + 1],
        };
        assert!(matches!(
            root.open(&aad, &oversized, |_| ()),
            Err(VaultCryptoError::PayloadTooLarge)
        ));
    }

    #[test]
    fn nonce_source_failures_are_stable_and_fail_closed() {
        let root = root_key(14);
        let aad = aad(8);
        let mut nonce_source = FixedNonceSource::failing();

        assert!(matches!(
            root.seal_with_nonce_source(&aad, b"nonce-failure", &mut nonce_source),
            Err(VaultCryptoError::SealFailed)
        ));
        assert_eq!(nonce_source.calls, 1);
    }
}
