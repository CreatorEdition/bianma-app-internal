//! 既有设备 Root Key v1 的只读加载器。
//!
//! 本模块只从固定的系统凭据库 identity 读取已存在的 32-byte 根材料，并立即交给
//! `RootKeyHandle`。它不创建、写入、删除、轮换或恢复 root key，也不接入任何
//! 文件、SQLite、备份、Saga、Provider、IPC、Proxy、HTTP 或网络路径。

use crate::routing_v2::vault::{RootKeyHandle, VaultCryptoError};
use keyring::Entry;
use zeroize::Zeroizing;

const DEVICE_ROOT_KEY_V1_SERVICE: &str = "com.creatoredition.bianma.routing-v2.vault";
const DEVICE_ROOT_KEY_V1_ACCOUNT: &str = "device-root-key-v1";

/// 读取既有根材料的私有端口。
///
/// 返回值在离开具体后端的同一表达式中即包入 `Zeroizing`，避免普通 `Vec<u8>`
/// 进入 loader 后续路径。端口没有任何写入、删除、创建或轮换能力。
trait ExistingDeviceRootKeyReadPort {
    fn read_existing_device_root_key_v1(&mut self) -> Result<Zeroizing<Vec<u8>>, ()>;
}

/// 使用固定 identity 读取系统凭据库中已经存在的设备根材料。
struct NativeExistingDeviceRootKeyReadPort {
    entry: Entry,
}

impl NativeExistingDeviceRootKeyReadPort {
    fn open() -> Result<Self, VaultCryptoError> {
        Entry::new(DEVICE_ROOT_KEY_V1_SERVICE, DEVICE_ROOT_KEY_V1_ACCOUNT)
            .map(|entry| Self { entry })
            .map_err(|_| VaultCryptoError::VaultUnavailable)
    }
}

impl ExistingDeviceRootKeyReadPort for NativeExistingDeviceRootKeyReadPort {
    fn read_existing_device_root_key_v1(&mut self) -> Result<Zeroizing<Vec<u8>>, ()> {
        self.entry.get_secret().map(Zeroizing::new).map_err(|_| ())
    }
}

/// 只读加载固定 v1 slot 的既有设备 root key。
///
/// 缺失、锁定、权限拒绝、后端异常和长度异常全部收敛为 `vault_unavailable`。此入口
/// 不会 provision、覆盖、删除、轮换或恢复任何 root key；首次安装没有既有材料时也
/// 必须失败关闭。
pub(super) fn load_existing_device_root_key_v1() -> Result<RootKeyHandle, VaultCryptoError> {
    let mut reader = NativeExistingDeviceRootKeyReadPort::open()?;
    load_from_existing_device_root_key_reader(&mut reader)
}

fn load_from_existing_device_root_key_reader(
    reader: &mut impl ExistingDeviceRootKeyReadPort,
) -> Result<RootKeyHandle, VaultCryptoError> {
    let material = reader
        .read_existing_device_root_key_v1()
        .map_err(|_| VaultCryptoError::VaultUnavailable)?;

    RootKeyHandle::from_loaded_material(material).map_err(|_| VaultCryptoError::VaultUnavailable)
}

#[cfg(test)]
mod tests {
    use super::{
        load_from_existing_device_root_key_reader, ExistingDeviceRootKeyReadPort,
        DEVICE_ROOT_KEY_V1_ACCOUNT, DEVICE_ROOT_KEY_V1_SERVICE,
    };
    use crate::routing_v2::vault::{VaultAad, VaultCryptoError};
    use zeroize::Zeroizing;

    struct FakeExistingDeviceRootKeyReadPort {
        material: Option<Vec<u8>>,
        fail_read: bool,
        read_count: usize,
    }

    impl FakeExistingDeviceRootKeyReadPort {
        fn available(material: Vec<u8>) -> Self {
            Self {
                material: Some(material),
                fail_read: false,
                read_count: 0,
            }
        }

        fn unavailable() -> Self {
            Self {
                material: None,
                fail_read: true,
                read_count: 0,
            }
        }
    }

    impl ExistingDeviceRootKeyReadPort for FakeExistingDeviceRootKeyReadPort {
        fn read_existing_device_root_key_v1(&mut self) -> Result<Zeroizing<Vec<u8>>, ()> {
            self.read_count += 1;
            if self.fail_read {
                return Err(());
            }

            self.material.take().map(Zeroizing::new).ok_or(())
        }
    }

    #[test]
    fn v1_identity_is_separate_from_the_capability_poc() {
        assert_eq!(
            DEVICE_ROOT_KEY_V1_SERVICE,
            "com.creatoredition.bianma.routing-v2.vault"
        );
        assert_eq!(DEVICE_ROOT_KEY_V1_ACCOUNT, "device-root-key-v1");
        assert_ne!(
            DEVICE_ROOT_KEY_V1_SERVICE,
            "com.creatoredition.bianma.routing-v2.vault-poc"
        );
    }

    #[test]
    fn read_failure_is_stably_mapped_to_vault_unavailable() {
        let mut reader = FakeExistingDeviceRootKeyReadPort::unavailable();

        assert!(matches!(
            load_from_existing_device_root_key_reader(&mut reader),
            Err(VaultCryptoError::VaultUnavailable)
        ));
        assert_eq!(reader.read_count, 1);
    }

    #[test]
    fn only_exactly_32_bytes_can_become_a_root_key_handle() {
        for length in [0_usize, 31, 33] {
            let mut reader = FakeExistingDeviceRootKeyReadPort::available(vec![7_u8; length]);

            assert!(matches!(
                load_from_existing_device_root_key_reader(&mut reader),
                Err(VaultCryptoError::VaultUnavailable)
            ));
            assert_eq!(reader.read_count, 1);
        }
    }

    #[test]
    fn exact_32_byte_material_creates_a_usable_opaque_handle() {
        let mut reader = FakeExistingDeviceRootKeyReadPort::available(vec![9_u8; 32]);
        let root = load_from_existing_device_root_key_reader(&mut reader)
            .expect("精确 32-byte 已有材料应可建立根密钥句柄");
        let aad = VaultAad::sealed_material([4_u8; 16]);
        let payload = root
            .seal(&aad, b"root-key-loader-round-trip")
            .expect("既有根密钥应可密封测试材料");

        assert_eq!(reader.read_count, 1);
        assert_eq!(
            root.open(&aad, &payload, |plaintext| plaintext.to_vec()),
            Ok(b"root-key-loader-round-trip".to_vec())
        );
    }
}
