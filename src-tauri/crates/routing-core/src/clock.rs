//! 分类器使用的墙钟 Port。

use std::time::{SystemTime, UNIX_EPOCH};

use super::error::RouteReject;

/// 分类器墙钟 Port。
///
/// 宿主应与 ingress verifier 使用同一可靠时间源；任何读取失败都必须 fail closed。
pub trait ClassifierClock: Send + Sync {
    /// 返回 Unix 毫秒。
    fn now_millis(&self) -> Result<u64, RouteReject>;
}

/// 基于系统墙钟的生产实现。
pub struct SystemClassifierClock;

impl ClassifierClock for SystemClassifierClock {
    fn now_millis(&self) -> Result<u64, RouteReject> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RouteReject::ClockUnavailable)?;
        u64::try_from(duration.as_millis()).map_err(|_| RouteReject::ClockUnavailable)
    }
}
