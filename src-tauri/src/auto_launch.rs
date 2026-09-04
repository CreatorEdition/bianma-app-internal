use crate::brand::PRODUCT_DISPLAY_NAME;
use crate::error::AppError;
use auto_launch::{AutoLaunch, AutoLaunchBuilder};

const LEGACY_PRODUCT_DISPLAY_NAME: &str = "CC Switch";

/// 获取 macOS 上的 .app bundle 路径
/// 将 `/path/to/bianma.ai.app/Contents/MacOS/bianma.ai` 转换为 `/path/to/bianma.ai.app`
#[cfg(target_os = "macos")]
fn get_macos_app_bundle_path(exe_path: &std::path::Path) -> Option<std::path::PathBuf> {
    let path_str = exe_path.to_string_lossy();
    // 查找 .app/Contents/MacOS/ 模式
    if let Some(app_pos) = path_str.find(".app/Contents/MacOS/") {
        let app_bundle_end = app_pos + 4; // ".app" 的结束位置
        Some(std::path::PathBuf::from(&path_str[..app_bundle_end]))
    } else {
        None
    }
}

/// 初始化 AutoLaunch 实例
fn get_auto_launch_for_name(app_name: &str) -> Result<AutoLaunch, AppError> {
    let exe_path =
        std::env::current_exe().map_err(|e| AppError::Message(format!("无法获取应用路径: {e}")))?;

    // macOS 需要使用 .app bundle 路径，否则 AppleScript login item 会打开终端
    #[cfg(target_os = "macos")]
    let app_path = get_macos_app_bundle_path(&exe_path).unwrap_or(exe_path);

    #[cfg(not(target_os = "macos"))]
    let app_path = exe_path;

    // 使用 AutoLaunchBuilder 消除平台差异
    // macOS: 使用 AppleScript 方式（默认），需要 .app bundle 路径
    // Windows/Linux: 使用注册表/XDG autostart
    let auto_launch = AutoLaunchBuilder::new()
        .set_app_name(app_name)
        .set_app_path(&app_path.to_string_lossy())
        .build()
        .map_err(|e| AppError::Message(format!("创建 AutoLaunch 失败: {e}")))?;

    Ok(auto_launch)
}

fn get_auto_launch() -> Result<AutoLaunch, AppError> {
    get_auto_launch_for_name(PRODUCT_DISPLAY_NAME)
}

fn is_auto_launch_enabled_for_name(app_name: &str) -> Result<bool, AppError> {
    get_auto_launch_for_name(app_name)?
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))
}

fn disable_auto_launch_for_name(app_name: &str) -> Result<bool, AppError> {
    let auto_launch = get_auto_launch_for_name(app_name)?;
    if !auto_launch
        .is_enabled()
        .map_err(|e| AppError::Message(format!("检查开机自启状态失败: {e}")))?
    {
        return Ok(false);
    }

    auto_launch
        .disable()
        .map_err(|e| AppError::Message(format!("禁用开机自启失败: {e}")))?;
    Ok(true)
}

/// 启用开机自启
pub fn enable_auto_launch() -> Result<(), AppError> {
    if let Err(error) = disable_auto_launch_for_name(LEGACY_PRODUCT_DISPLAY_NAME) {
        log::warn!("清理历史开机自启项失败: {error}");
    }

    let auto_launch = get_auto_launch()?;
    auto_launch
        .enable()
        .map_err(|e| AppError::Message(format!("启用开机自启失败: {e}")))?;
    log::info!("已启用开机自启");
    Ok(())
}

/// 禁用开机自启
pub fn disable_auto_launch() -> Result<(), AppError> {
    let current_result = disable_auto_launch_for_name(PRODUCT_DISPLAY_NAME);
    let legacy_result = disable_auto_launch_for_name(LEGACY_PRODUCT_DISPLAY_NAME);

    if let Err(error) = &legacy_result {
        log::warn!("禁用历史开机自启项失败: {error}");
    }

    current_result?;

    log::info!("已禁用开机自启");
    Ok(())
}

/// 检查是否已启用开机自启
pub fn is_auto_launch_enabled() -> Result<bool, AppError> {
    if is_auto_launch_enabled_for_name(PRODUCT_DISPLAY_NAME)? {
        return Ok(true);
    }

    match is_auto_launch_enabled_for_name(LEGACY_PRODUCT_DISPLAY_NAME) {
        Ok(enabled) => Ok(enabled),
        Err(error) => {
            log::warn!("检查历史开机自启项失败: {error}");
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::get_macos_app_bundle_path;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_valid() {
        let exe_path = std::path::Path::new("/Applications/bianma.ai.app/Contents/MacOS/bianma.ai");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from("/Applications/bianma.ai.app"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_with_spaces() {
        let exe_path =
            std::path::Path::new("/Users/test/My Apps/bianma.ai.app/Contents/MacOS/bianma.ai");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(
            result,
            Some(std::path::PathBuf::from(
                "/Users/test/My Apps/bianma.ai.app"
            ))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_not_in_bundle() {
        let exe_path = std::path::Path::new("/usr/local/bin/bianma");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(result, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_app_bundle_path_dev_build() {
        // 开发环境下的路径通常不在 .app bundle 内
        let exe_path = std::path::Path::new("/Users/dev/project/target/debug/bianma-app");
        let result = get_macos_app_bundle_path(exe_path);
        assert_eq!(result, None);
    }
}
