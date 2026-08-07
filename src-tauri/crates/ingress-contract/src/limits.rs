//! 入站 wire 与验证窗口的不可放宽硬上限。

/// 单个原始请求正文的绝对上限；各 Operation 还必须声明更小或相等的上限。
pub const MAX_RAW_BODY_BYTES: usize = 16 * 1024 * 1024;
/// 单个 origin-form target 的最大长度。
pub const MAX_REQUEST_TARGET_BYTES: usize = 4 * 1024;
/// 单个请求允许的 Header 数量。
pub const MAX_HEADER_COUNT: usize = 128;
/// Header 名称最大长度。
pub const MAX_HEADER_NAME_BYTES: usize = 128;
/// 单个 Header 值最大长度。
pub const MAX_HEADER_VALUE_BYTES: usize = 16 * 1024;
/// Attestation wire 最大长度。
pub const MAX_ATTESTATION_BYTES: usize = 32 * 1024;
/// Authorization bundle wire 最大长度。
pub const MAX_AUTHORIZATION_BUNDLE_BYTES: usize = 64 * 1024;
/// Capability authorization wire 最大长度。
pub const MAX_CAPABILITY_AUTHORIZATION_BYTES: usize = 16 * 1024;
/// 一个 Managed Permit 最多授权的出站目标数。
pub const MAX_ALLOWED_EGRESS_TARGETS: usize = 64;
/// Canonical Origin 最大长度。
pub const MAX_CANONICAL_ORIGIN_BYTES: usize = 512;
/// Attestation 允许的最大生存期。
pub const MAX_ATTESTATION_TTL_MILLIS: u64 = 120_000;
/// Capability Authorization 允许的最大生存期。
pub const MAX_CAPABILITY_TTL_MILLIS: u64 = 120_000;
/// 允许签发时钟相对验证时钟的正向偏差。
pub const MAX_CLOCK_SKEW_MILLIS: u64 = 5_000;
