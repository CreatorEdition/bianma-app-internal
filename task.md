# bianma-app 单仓开源主线说明

## 当前定位

- `bianma-app` 当前是 `Bianma` 唯一正式 App 主仓。
- 默认范围包括：产品源码、README、用户手册、公开中文文案、开源协作说明、release、updater 与二进制分发。
- `bianma-app-product` 降级为历史迁移源与待归档目录，不再作为新任务或新发布入口。

## 当前状态

- ✅ 已完成（2026-04-12）：清理公开深链接测试页 `deplink.html` 的品牌口径，统一页面标题、说明文案与使用提示到 `bianma-app` / `bianma://` 主语境。
- ✅ 已完成（2026-04-12）：停用公开仓 `release.yml`，避免把公开仓误当成当前正式发布仓。
- ✅ 已完成（2026-04-12）：移除 `CODE_OF_CONDUCT.md` 中的旧个人邮箱，统一改为当前公开维护入口。
- ✅ 已完成（2026-04-12）：压缩公开用户手册主路径中的历史 `cc-switch` 命名解释，并统一回指迁移兼容说明。
- ✅ 已完成（2026-04-12）：继续压缩 FAQ / Skills / 导入说明 / 英日文索引中的历史命名提醒，减少旧标识在公开主路径的前置暴露。
- ✅ 已完成（2026-04-12）：收束开发者协议文档与 Flatpak 兼容文档中的 legacy 标识说明，统一改为兼容标识清单与迁移导向。
- ✅ 已完成（2026-07-04）：单仓完全开源收口，`bianma-app` 改为后续唯一正式开发、发布与 updater 目标仓；基础 manifest 已同步到 `bianma-app / 0.0.1 / CreatorEdition/bianma-app`。
- ✅ 已完成（2026-07-04）：切片 1 清理开源单仓发布身份与 release/updater 残留；前端 release 链接与 Rust fallback 更新入口已指向 `CreatorEdition/bianma-app`，公开 `release.yml` 已改为安全预检占位，不再声明由 `bianma-app-product` 私有仓承接。
- ✅ 已完成（2026-07-04）：切片 2 清理公开可见品牌入口；主 UI、About 面板、Windows 覆盖窗口标题与 Flatpak 用户可见元数据已统一到 `bianma.ai` / `CreatorEdition/bianma-app`。
- ✅ 已完成（2026-07-04）：切片 3 清理 i18n 应用标题；中英日 `app.title` 与 `app.description` 已统一到 `bianma.ai` 和本地 AI 编码控制面口径。
- ✅ 已完成（2026-07-05）：迁移 Provider 批量延迟测速最小切片；已补齐缓存表、DAO、Tauri 命令与前端 API 基础能力，未迁移 ProviderWorkspacePanel 大 UI。
- ✅ 已完成（2026-07-05）：补齐 Provider Workspace 未来依赖的通用模型发现 API 基础能力；新增 `fetch_provider_models` 命令、结构化错误、前端类型与 API 包装，未迁移 ProviderWorkspacePanel 大 UI。
- ✅ 已完成（2026-07-05）：迁移 Provider Workspace 前置依赖的 storageCompat 最小切片；仅补齐本地存储兼容工具与必要单测，未迁移 ProviderWorkspacePanel 大 UI。

## 维护边界

- 后续产品代码与公开文档默认都进入本仓。
- release / updater / 二进制分发默认以本仓为准。
- 内部任务拆解、灰度状态和迁移审计材料仍应优先写入 `.teamwork/` 或 `docs/`，避免污染用户文档入口。
- 从 `bianma-app-product` 合入内容前，必须先完成差异审计和敏感信息审计。

## 仍待后续复查

- ⚠️ 需要分批审计 `bianma-app-product` 与本仓差异，确认哪些代码、文档与发布配置应合入。
- ⚠️ 合入前必须复查密钥、私有 URL、签名配置、内部任务记录与未公开合作方材料。
- ⚠️ 正式公开打包发布仍需后续门禁：签名与 notarization、版本号策略、`latest.json` 生成、跨平台构建矩阵、release artifact 上传和人工发布审批。
- ⚠️ Flatpak 的 `com.ccswitch.desktop`、`com.ccswitch.desktop.desktop`、`Exec=cc-switch`、`cc-switch.deb` 与历史导出包名仍作为兼容标识保留，避免破坏既有打包链路和已安装用户迁移路径。

## 说明

- 本文件只记录 `bianma-app` 仓内主线边界；跨仓治理以根级 `docs/` 为准。
