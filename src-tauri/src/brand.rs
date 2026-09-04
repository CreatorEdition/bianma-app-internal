pub const PRODUCT_DISPLAY_NAME: &str = "bianma.ai";
// 版本探测 helper 当前未被主流程直接调用，但需要集中保留 GitHub API 品牌头。
#[allow(dead_code)]
pub const GITHUB_API_USER_AGENT: &str = "bianma.ai";
pub const RELEASES_LATEST_URL: &str =
    "https://github.com/CreatorEdition/bianma-app-internal/releases/latest";
pub const PRIMARY_DEEPLINK_SCHEME: &str = "bianma";
pub const LEGACY_DEEPLINK_SCHEME: &str = "ccswitch";

pub fn is_supported_deeplink_scheme(scheme: &str) -> bool {
    matches!(scheme, PRIMARY_DEEPLINK_SCHEME | LEGACY_DEEPLINK_SCHEME)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_deeplink_scheme_accepts_primary_and_legacy_only() {
        assert!(is_supported_deeplink_scheme(PRIMARY_DEEPLINK_SCHEME));
        assert!(is_supported_deeplink_scheme(LEGACY_DEEPLINK_SCHEME));
        assert!(!is_supported_deeplink_scheme("https"));
        assert!(!is_supported_deeplink_scheme("bianma-app"));
    }
}
