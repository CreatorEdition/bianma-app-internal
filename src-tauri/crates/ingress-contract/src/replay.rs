//! 有界、原子且 fail-closed 的一次性 nonce 消费。

use std::{collections::HashMap, sync::Mutex};

use crate::{IssuerEpoch, NonceReject, OneShotNonce};

/// 不同证明类别使用独立重放命名空间。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NonceNamespace {
    /// Managed Context Attestation nonce。
    ManagedAttestation,
    /// 固定单部署 Capability Authorization nonce。
    CapabilityAuthorization,
}

/// 原子消费一次性 nonce 的宿主 Port。
pub trait OneShotNonceStore: Send + Sync {
    /// 原子消费 nonce。
    ///
    /// 实现不得使用 `contains()` 后再 `insert()` 的非原子组合；必须把传入时间视为
    /// 不递减 high-water，已删除的过期 nonce 不得因底层时钟或存储回拨而复活。
    fn consume(
        &self,
        namespace: NonceNamespace,
        issuer_epoch: IssuerEpoch,
        nonce: OneShotNonce,
        expires_at_millis: u64,
        now_millis: u64,
    ) -> Result<(), NonceReject>;
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct NonceKey {
    namespace: NonceNamespace,
    issuer_epoch: IssuerEpoch,
    nonce: OneShotNonce,
}

/// 首个切片使用的单锁有界内存 nonce store。
///
/// 同一临界区内完成过期清理、重放检查、容量检查和插入；容量满时不淘汰仍有效条目。
pub struct MemoryNonceStore {
    capacity: usize,
    state: Mutex<NonceState>,
}

struct NonceState {
    entries: HashMap<NonceKey, u64>,
    high_water_millis: u64,
}

impl MemoryNonceStore {
    /// 构造有界 store。容量为零会立即拒绝。
    pub fn new(capacity: usize) -> Result<Self, NonceReject> {
        if capacity == 0 {
            return Err(NonceReject::CapacityExhausted);
        }
        Ok(Self {
            capacity,
            state: Mutex::new(NonceState {
                entries: HashMap::with_capacity(capacity.min(4096)),
                high_water_millis: 0,
            }),
        })
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> Result<usize, NonceReject> {
        self.state
            .lock()
            .map(|state| state.entries.len())
            .map_err(|_| NonceReject::StoreUnavailable)
    }
}

impl OneShotNonceStore for MemoryNonceStore {
    fn consume(
        &self,
        namespace: NonceNamespace,
        issuer_epoch: IssuerEpoch,
        nonce: OneShotNonce,
        expires_at_millis: u64,
        now_millis: u64,
    ) -> Result<(), NonceReject> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| NonceReject::StoreUnavailable)?;
        state.high_water_millis = state.high_water_millis.max(now_millis);
        let effective_now = state.high_water_millis;
        if expires_at_millis <= effective_now {
            return Err(NonceReject::Expired);
        }
        state
            .entries
            .retain(|_, expires_at| *expires_at > effective_now);

        let key = NonceKey {
            namespace,
            issuer_epoch,
            nonce,
        };
        if state.entries.contains_key(&key) {
            return Err(NonceReject::Replayed);
        }
        if state.entries.len() >= self.capacity {
            return Err(NonceReject::CapacityExhausted);
        }
        state.entries.insert(key, expires_at_millis);
        Ok(())
    }
}
