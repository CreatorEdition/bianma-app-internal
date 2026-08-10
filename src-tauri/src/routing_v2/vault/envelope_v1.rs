//! routing v2 设备本地持久化 Vault envelope v1 协议合同。
//!
//! 本模块仅在内存中编码、解码和认证固定二进制帧。它不是文件格式接线、SQLite
//! 存储、真实 Vault 或加密备份实现；未来存储层必须在独立安全切片中决定原子写入、
//! 轮换、恢复与删除语义。

use super::{
    Nonce, NonceSource, RootKeyHandle, VaultCryptoError, AEAD_TAG_LENGTH, MAX_CIPHERTEXT_BYTES,
    MAX_PLAINTEXT_BYTES, NONCE_LENGTH,
};
use aes_gcm_siv::aead::{Aead, Payload};
use zeroize::{Zeroize, Zeroizing};

const MAGIC: [u8; 8] = *b"BMVAULT1";
const FORMAT_VERSION: u8 = 1;
const ALGORITHM_ID_AES_256_GCM_SIV: u8 = 1;
const ROOT_KEY_SLOT: u8 = 1;
const PURPOSE_DEVICE_LOCAL_MATERIAL: u8 = 1;
const MAGIC_OFFSET: usize = 0;
const VERSION_OFFSET: usize = MAGIC_OFFSET + MAGIC.len();
const ALGORITHM_OFFSET: usize = VERSION_OFFSET + 1;
const KEY_SLOT_OFFSET: usize = ALGORITHM_OFFSET + 1;
const PURPOSE_OFFSET: usize = KEY_SLOT_OFFSET + 1;
const RECORD_ID_OFFSET: usize = PURPOSE_OFFSET + 1;
const RECORD_ID_LENGTH: usize = 16;
const NONCE_OFFSET: usize = RECORD_ID_OFFSET + RECORD_ID_LENGTH;
const CIPHERTEXT_LENGTH_OFFSET: usize = NONCE_OFFSET + NONCE_LENGTH;
const HEADER_LENGTH: usize = CIPHERTEXT_LENGTH_OFFSET + 4;
const MIN_ENVELOPE_LENGTH: usize = HEADER_LENGTH + AEAD_TAG_LENGTH;
const MAX_ENVELOPE_LENGTH: usize = HEADER_LENGTH + MAX_CIPHERTEXT_BYTES;

/// 与设备本地持久化 envelope 绑定的非敏感记录身份。
///
/// 它不提供原始值 getter，未来持久化层只能从已验证的无 Secret 记录元数据构造。
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct PersistentVaultContext {
    record_id: [u8; RECORD_ID_LENGTH],
}

impl PersistentVaultContext {
    /// 为一个设备本地、无 Secret 的记录 UUID 建立 envelope 认证上下文。
    pub(crate) fn device_local_material(record_id: [u8; RECORD_ID_LENGTH]) -> Self {
        Self { record_id }
    }
}

/// 仅在内存中存在的持久化 envelope v1 表示。
///
/// 该类型不实现 Clone、Debug、Display、Serde 或 raw-cipher getter；调用方只能将其
/// 编码为固定二进制帧，或在匹配上下文下经认证打开。
pub(crate) struct PersistentVaultEnvelopeV1 {
    context: PersistentVaultContext,
    nonce: [u8; NONCE_LENGTH],
    ciphertext: Vec<u8>,
}

impl PersistentVaultEnvelopeV1 {
    /// 编码为固定、无可选字段的 v1 二进制帧。
    ///
    /// 输出只含 nonce 和密文，未包含 root key 或明文；它不是文件 I/O 接口。
    pub(crate) fn encode(&self) -> Result<Vec<u8>, VaultCryptoError> {
        let header = encode_header(&self.context, &self.nonce, self.ciphertext.len())?;
        let mut encoded = Vec::with_capacity(HEADER_LENGTH + self.ciphertext.len());
        encoded.extend_from_slice(&header);
        encoded.extend_from_slice(&self.ciphertext);
        Ok(encoded)
    }

    /// 从不受信任字节解析固定 v1 帧；解析成功不表示 AEAD 认证已成功。
    pub(crate) fn decode(encoded: &[u8]) -> Result<Self, VaultCryptoError> {
        if encoded.len() > MAX_ENVELOPE_LENGTH {
            return Err(VaultCryptoError::PayloadTooLarge);
        }
        if encoded.len() < MIN_ENVELOPE_LENGTH {
            return Err(VaultCryptoError::EnvelopeMalformed);
        }

        let header = &encoded[..HEADER_LENGTH];
        validate_header_constants(header)?;
        let ciphertext_length = usize::try_from(u32::from_be_bytes(
            header[CIPHERTEXT_LENGTH_OFFSET..HEADER_LENGTH]
                .try_into()
                .map_err(|_| VaultCryptoError::EnvelopeMalformed)?,
        ))
        .map_err(|_| VaultCryptoError::EnvelopeMalformed)?;
        if !(AEAD_TAG_LENGTH..=MAX_CIPHERTEXT_BYTES).contains(&ciphertext_length) {
            return Err(VaultCryptoError::EnvelopeMalformed);
        }
        let expected_length = HEADER_LENGTH
            .checked_add(ciphertext_length)
            .ok_or(VaultCryptoError::EnvelopeMalformed)?;
        if encoded.len() != expected_length {
            return Err(VaultCryptoError::EnvelopeMalformed);
        }

        let record_id = header[RECORD_ID_OFFSET..NONCE_OFFSET]
            .try_into()
            .map_err(|_| VaultCryptoError::EnvelopeMalformed)?;
        let nonce = header[NONCE_OFFSET..CIPHERTEXT_LENGTH_OFFSET]
            .try_into()
            .map_err(|_| VaultCryptoError::EnvelopeMalformed)?;
        Ok(Self {
            context: PersistentVaultContext { record_id },
            nonce,
            ciphertext: encoded[HEADER_LENGTH..].to_vec(),
        })
    }
}

impl Drop for PersistentVaultEnvelopeV1 {
    fn drop(&mut self) {
        self.context.record_id.zeroize();
        self.nonce.zeroize();
        self.ciphertext.zeroize();
    }
}

/// 使用指定内部 nonce source 密封 v1 envelope；仅生产入口使用 OS CSPRNG。
pub(super) fn seal_with_nonce_source(
    root_key: &RootKeyHandle,
    context: &PersistentVaultContext,
    plaintext: &[u8],
    nonce_source: &mut impl NonceSource,
) -> Result<PersistentVaultEnvelopeV1, VaultCryptoError> {
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(VaultCryptoError::PayloadTooLarge);
    }

    let expected_ciphertext_length = plaintext
        .len()
        .checked_add(AEAD_TAG_LENGTH)
        .ok_or(VaultCryptoError::PayloadTooLarge)?;
    let mut nonce = [0_u8; NONCE_LENGTH];
    nonce_source.fill_nonce(&mut nonce)?;
    let header = encode_header(context, &nonce, expected_ciphertext_length)?;
    let cipher = root_key.cipher()?;
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &header,
            },
        )
        .map_err(|_| VaultCryptoError::SealFailed)?;
    if ciphertext.len() != expected_ciphertext_length {
        return Err(VaultCryptoError::SealFailed);
    }

    Ok(PersistentVaultEnvelopeV1 {
        context: *context,
        nonce,
        ciphertext,
    })
}

/// 认证打开 v1 envelope；任何上下文、格式或认证错配都不得交付明文。
pub(super) fn open<T>(
    root_key: &RootKeyHandle,
    expected_context: &PersistentVaultContext,
    envelope: &PersistentVaultEnvelopeV1,
    consume: impl FnOnce(&[u8]) -> T,
) -> Result<T, VaultCryptoError> {
    if envelope.context != *expected_context {
        return Err(VaultCryptoError::OpenFailed);
    }
    if !(AEAD_TAG_LENGTH..=MAX_CIPHERTEXT_BYTES).contains(&envelope.ciphertext.len()) {
        return Err(VaultCryptoError::OpenFailed);
    }

    let header = encode_header(expected_context, &envelope.nonce, envelope.ciphertext.len())?;
    let cipher = root_key
        .cipher()
        .map_err(|_| VaultCryptoError::OpenFailed)?;
    let plaintext = cipher
        .decrypt(
            Nonce::from_slice(&envelope.nonce),
            Payload {
                msg: &envelope.ciphertext,
                aad: &header,
            },
        )
        .map_err(|_| VaultCryptoError::OpenFailed)?;
    let plaintext = Zeroizing::new(plaintext);
    Ok(consume(plaintext.as_slice()))
}

fn encode_header(
    context: &PersistentVaultContext,
    nonce: &[u8; NONCE_LENGTH],
    ciphertext_length: usize,
) -> Result<[u8; HEADER_LENGTH], VaultCryptoError> {
    if !(AEAD_TAG_LENGTH..=MAX_CIPHERTEXT_BYTES).contains(&ciphertext_length) {
        return Err(VaultCryptoError::EnvelopeMalformed);
    }
    let ciphertext_length =
        u32::try_from(ciphertext_length).map_err(|_| VaultCryptoError::EnvelopeMalformed)?;
    let mut header = [0_u8; HEADER_LENGTH];
    header[MAGIC_OFFSET..VERSION_OFFSET].copy_from_slice(&MAGIC);
    header[VERSION_OFFSET] = FORMAT_VERSION;
    header[ALGORITHM_OFFSET] = ALGORITHM_ID_AES_256_GCM_SIV;
    header[KEY_SLOT_OFFSET] = ROOT_KEY_SLOT;
    header[PURPOSE_OFFSET] = PURPOSE_DEVICE_LOCAL_MATERIAL;
    header[RECORD_ID_OFFSET..NONCE_OFFSET].copy_from_slice(&context.record_id);
    header[NONCE_OFFSET..CIPHERTEXT_LENGTH_OFFSET].copy_from_slice(nonce);
    header[CIPHERTEXT_LENGTH_OFFSET..HEADER_LENGTH]
        .copy_from_slice(&ciphertext_length.to_be_bytes());
    Ok(header)
}

fn validate_header_constants(header: &[u8]) -> Result<(), VaultCryptoError> {
    if header.len() != HEADER_LENGTH
        || header[MAGIC_OFFSET..VERSION_OFFSET] != MAGIC
        || header[VERSION_OFFSET] != FORMAT_VERSION
        || header[ALGORITHM_OFFSET] != ALGORITHM_ID_AES_256_GCM_SIV
        || header[KEY_SLOT_OFFSET] != ROOT_KEY_SLOT
        || header[PURPOSE_OFFSET] != PURPOSE_DEVICE_LOCAL_MATERIAL
    {
        return Err(VaultCryptoError::EnvelopeMalformed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        seal_with_nonce_source, PersistentVaultContext, PersistentVaultEnvelopeV1, AEAD_TAG_LENGTH,
        ALGORITHM_OFFSET, CIPHERTEXT_LENGTH_OFFSET, FORMAT_VERSION, HEADER_LENGTH, KEY_SLOT_OFFSET,
        MAGIC_OFFSET, MAX_ENVELOPE_LENGTH, MAX_PLAINTEXT_BYTES, NONCE_OFFSET, PURPOSE_OFFSET,
        RECORD_ID_OFFSET, VERSION_OFFSET,
    };
    use crate::routing_v2::vault::{NonceSource, RootKeyHandle, VaultCryptoError};
    use zeroize::Zeroizing;

    struct FixedNonceSource {
        nonce: [u8; 12],
        calls: usize,
    }

    impl FixedNonceSource {
        fn new(nonce: [u8; 12]) -> Self {
            Self { nonce, calls: 0 }
        }
    }

    impl NonceSource for FixedNonceSource {
        fn fill_nonce(&mut self, nonce: &mut [u8; 12]) -> Result<(), VaultCryptoError> {
            self.calls += 1;
            *nonce = self.nonce;
            Ok(())
        }
    }

    fn root_key(fill: u8) -> RootKeyHandle {
        RootKeyHandle::from_loaded_material(Zeroizing::new(vec![fill; 32]))
            .expect("测试根密钥长度固定为 32 byte")
    }

    fn context(fill: u8) -> PersistentVaultContext {
        PersistentVaultContext::device_local_material([fill; 16])
    }

    fn sealed(root: &RootKeyHandle, context: &PersistentVaultContext) -> PersistentVaultEnvelopeV1 {
        root.seal_persistent(context, b"routing-v2-persistent-envelope-test")
            .expect("测试密封应成功")
    }

    fn assert_open_failed<T>(result: Result<T, VaultCryptoError>) {
        assert!(matches!(result, Err(VaultCryptoError::OpenFailed)));
    }

    #[test]
    fn canonical_encoding_round_trips_only_with_matching_context() {
        let root = root_key(1);
        let record_context = context(2);
        let mut source = FixedNonceSource::new([3; 12]);
        let envelope =
            seal_with_nonce_source(&root, &record_context, b"envelope-round-trip", &mut source)
                .expect("固定 nonce 的 envelope 应成功密封");
        let encoded = envelope.encode().expect("有效 envelope 应可编码");

        assert_eq!(source.calls, 1);
        assert_eq!(
            encoded.len(),
            HEADER_LENGTH + b"envelope-round-trip".len() + AEAD_TAG_LENGTH
        );
        assert_eq!(&encoded[MAGIC_OFFSET..VERSION_OFFSET], b"BMVAULT1");
        assert_eq!(encoded[VERSION_OFFSET], FORMAT_VERSION);
        assert_eq!(
            encoded[CIPHERTEXT_LENGTH_OFFSET..HEADER_LENGTH],
            u32::try_from(b"envelope-round-trip".len() + AEAD_TAG_LENGTH)
                .expect("测试 ciphertext 长度可表示为 u32")
                .to_be_bytes()
        );

        let decoded = PersistentVaultEnvelopeV1::decode(&encoded).expect("canonical 帧应可解析");
        assert_eq!(
            root.open_persistent(&record_context, &decoded, |plaintext| plaintext.to_vec()),
            Ok(b"envelope-round-trip".to_vec())
        );
        assert_open_failed(root.open_persistent(&context(4), &decoded, |_| ()));
    }

    #[test]
    fn fixed_header_fields_and_payload_tampering_fail_closed() {
        let root = root_key(5);
        let context = context(6);
        let encoded = sealed(&root, &context)
            .encode()
            .expect("有效 envelope 应可编码");

        for index in [
            MAGIC_OFFSET,
            VERSION_OFFSET,
            ALGORITHM_OFFSET,
            KEY_SLOT_OFFSET,
            PURPOSE_OFFSET,
        ] {
            let mut tampered = encoded.clone();
            tampered[index] ^= 1;
            assert!(matches!(
                PersistentVaultEnvelopeV1::decode(&tampered),
                Err(VaultCryptoError::EnvelopeMalformed)
            ));
        }

        for index in [
            RECORD_ID_OFFSET,
            NONCE_OFFSET,
            HEADER_LENGTH,
            encoded.len() - 1,
        ] {
            let mut tampered = encoded.clone();
            tampered[index] ^= 1;
            let decoded = PersistentVaultEnvelopeV1::decode(&tampered)
                .expect("结构仍完整的篡改帧应可到达认证边界");
            assert_open_failed(root.open_persistent(&context, &decoded, |_| ()));
        }
    }

    #[test]
    fn record_replacement_cannot_be_rebound_to_a_different_context() {
        let root = root_key(14);
        let original_context = context(15);
        let replacement_context = context(16);
        let mut encoded = sealed(&root, &original_context)
            .encode()
            .expect("有效 envelope 应可编码");

        encoded[RECORD_ID_OFFSET..NONCE_OFFSET].copy_from_slice(&[16; 16]);
        let decoded =
            PersistentVaultEnvelopeV1::decode(&encoded).expect("record 篡改不应破坏固定帧结构");
        assert_open_failed(root.open_persistent(&replacement_context, &decoded, |_| ()));
    }

    #[test]
    fn malformed_lengths_truncation_and_trailing_bytes_fail_closed() {
        let root = root_key(7);
        let context = context(8);
        let encoded = sealed(&root, &context)
            .encode()
            .expect("有效 envelope 应可编码");

        for length in [
            0,
            HEADER_LENGTH - 1,
            HEADER_LENGTH + AEAD_TAG_LENGTH - 1,
            encoded.len() - 1,
        ] {
            assert!(matches!(
                PersistentVaultEnvelopeV1::decode(&encoded[..length]),
                Err(VaultCryptoError::EnvelopeMalformed)
            ));
        }

        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(matches!(
            PersistentVaultEnvelopeV1::decode(&trailing),
            Err(VaultCryptoError::EnvelopeMalformed)
        ));

        for length in [
            0_u32,
            (AEAD_TAG_LENGTH - 1) as u32,
            (super::MAX_CIPHERTEXT_BYTES + 1) as u32,
        ] {
            let mut malformed = encoded.clone();
            malformed[CIPHERTEXT_LENGTH_OFFSET..HEADER_LENGTH]
                .copy_from_slice(&length.to_be_bytes());
            assert!(matches!(
                PersistentVaultEnvelopeV1::decode(&malformed),
                Err(VaultCryptoError::EnvelopeMalformed)
            ));
        }

        assert!(matches!(
            PersistentVaultEnvelopeV1::decode(&vec![0; MAX_ENVELOPE_LENGTH + 1]),
            Err(VaultCryptoError::PayloadTooLarge)
        ));
    }

    #[test]
    fn wrong_key_and_consumer_suppression_fail_closed() {
        let root = root_key(9);
        let other_root = root_key(10);
        let context = context(11);
        let envelope = sealed(&root, &context);
        let mut consumer_called = false;

        assert_open_failed(other_root.open_persistent(&context, &envelope, |_| {
            consumer_called = true;
        }));
        assert!(!consumer_called);
    }

    #[test]
    fn plaintext_limit_and_production_nonce_contract_hold() {
        let root = root_key(12);
        let context = context(13);
        let maximum = vec![0xA5; MAX_PLAINTEXT_BYTES];
        let first = root
            .seal_persistent(&context, &maximum)
            .expect("16 KiB 应允许密封");
        let second = root
            .seal_persistent(&context, &maximum)
            .expect("16 KiB 应允许重复密封");
        assert_ne!(
            first.encode().expect("有效 envelope 应可编码"),
            second.encode().expect("有效 envelope 应可编码")
        );
        assert!(matches!(
            root.seal_persistent(&context, &vec![0; MAX_PLAINTEXT_BYTES + 1]),
            Err(VaultCryptoError::PayloadTooLarge)
        ));
    }
}
