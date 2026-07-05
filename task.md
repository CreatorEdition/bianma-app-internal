# bianma-app 单仓开源主线说明

## 当前定位

- `bianma-app` 当前是 `Bianma` 唯一正式 App 主仓。
- 默认范围包括：产品源码、README、用户手册、公开中文文案、开源协作说明、release、updater 与二进制分发。
- `bianma-app-product` 降级为历史迁移源与待归档目录，不再作为新任务或新发布入口。

## 当前状态

- ✅ 已完成（2026-07-05）：迁移 App startup checks 抽取最小切片；仅抽取启动环境变量冲突检查、配置迁移 toast、skills 迁移 toast/invalidate 与 activeApp 切换冲突合并逻辑，新增公开仓 useAppStartupChecks 与定向 hook 测试。
- ✅ 已完成（2026-07-05）：迁移 useEnvBannerActions 最小切片；仅抽取 EnvWarningBanner dismiss/deleted 动作与对应定向测试，App.tsx 仅替换 EnvWarningBanner 的 dismiss/deleted 回调，未迁移 product 的 App 大结构。
- ✅ 已完成（2026-07-05）：收口 useEnvBannerActions 测试隔离与 App 集成测试冷启动稳定化；定向组合测试不再因模块级 mock 污染或 App 动态导入耗时超时失败。
- ✅ 已完成（2026-07-05）：迁移 App 事件订阅抽取最小切片；仅抽取 provider-switched、universal-provider-synced 与 webdav-sync-status-updated 三段订阅逻辑，新增公开仓 useAppEventSubscriptions 与定向 hook 测试。
- ✅ 已完成（2026-07-05）：补齐 Tauri 事件测试隔离 helper；全局测试清理会重置 mock event listeners，降低事件订阅类测试的跨用例污染风险。
- ✅ 已完成（2026-07-05）：设置元数据错误路径测试噪声收口小切片；同步 product 参考的 console.error spy，仅屏蔽 `[useSettingsMetadata]` 预期错误日志并在 afterEach 恢复，避免污染其他测试。
- ✅ 已完成（2026-07-05）：SQL 导出头品牌收口小切片；新导出的 SQL 备份 header 已统一为 `-- bianma.ai SQLite 导出`，导入继续兼容历史 `-- CC Switch SQLite 导出` 文件并补充定向 Rust 单测。
- ✅ 已完成（2026-07-05）：迁移主题 localStorage 主键兼容小切片；ThemeProvider 默认主键切换到 `bianma-theme`，兼容迁移并清理旧 `cc-switch-theme`，补充新旧键优先级定向单测。
- ✅ 已完成（2026-07-05）：同步用户手册导入提示默认导出文件名品牌；中英文数据库备份导入提示已统一为 `bianma-export-{时间戳/timestamp}.sql`。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 useImportExport 默认导出文件名品牌收口；默认 SQL 导出文件名已从 `cc-switch-export-*` 改为 `bianma-export-*`，并补充 saveFileDialog 默认文件名断言。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 Failover 前端 tooltip 资源化与组件单测；仅移除 FailoverToggle 与 FailoverPriorityBadge 内 tooltip 中文 fallback，并补充资源化 tooltip 与切换 action 参数定向单测。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 ProxyToggle tooltip 资源化与组件单测；移除组件内 tooltip 中文 fallback，确认中英日已有资源 key，并补充 inactive/active/broken 与切换动作定向单测。
- ✅ 已完成（2026-07-05）：迁移会话删除纯工具最小切片；新增 deleteUtils 复用删除目标过滤、删除参数映射与批量结果汇总逻辑，SessionManagerPage 已替换内联实现并补充定向单测。
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
- ✅ 已完成（2026-07-05）：应用级 localStorage 主键切换兼容小切片；`last-app`、`last-view` 与更新提醒关闭版本已切换到 `bianma-*` 主键，并保留旧键自动迁移清理。
- ✅ 已完成（2026-07-05）：ProviderWorkspacePanel 前置依赖切片 1，补齐公开仓所需的 ProviderMeta 收藏/模型发现协议字段与 providerConfigUtils 最小连接信息导出。
- ✅ 已完成（2026-07-05）：ProviderWorkspacePanel 前置依赖切片 2，给 ProviderList 增加最小 displayMode 支持；single 模式禁用搜索浮层快捷键与拖拽上下文，仅渲染传入供应商卡片。
- ✅ 已完成（2026-07-05）：迁移 ProviderWorkspacePanel 主 UI 切片；默认 providers 分支已接入工作台面板，保留搜索、收藏、键盘导航、模型发现、测速与单卡详情能力，未迁移合作方权重排序和内部 shell 变量。
- ✅ 已完成（2026-07-05）：修复 ProviderWorkspacePanel 审核问题；收口模型发现协议切换后的自动重入，确保旧请求不覆盖新协议结果且同 provider/protocol 不重复自动发现。
- ✅ 已完成（2026-07-05）：迁移剪贴板兼容兜底最小切片；`copy_text_to_clipboard` 保留 `arboard` 主路径，并在失败时使用系统命令写入剪贴板。
- ✅ 已完成（2026-07-05）：迁移 Provider 表单 key 输入字段最小切片；新增 providerKeyUtils 与 ProviderKeyField，OpenCode/OpenClaw 供应商标识输入已复用共享字段并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 Provider 预设列表工具最小切片；新增 providerPresetUtils，ProviderForm 预设条目构造、分组、分类 key 与标签已改为复用共享工具并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 Provider 预设选择应用工具最小切片；新增 providerPresetApplyUtils，ProviderForm 预设选择分支已复用 custom 重置计划与选择结果解析工具并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 Provider 提交校验/配置解析工具最小切片；新增 providerSubmitUtils，提交前供应商标识、非官方凭据与 Codex/Gemini/OMO settingsConfig 解析已改为共享工具并补充最小单测。
- ✅ 已完成（2026-07-05）：迁移 BasicFormFields 图标选择器可访问性与交互最小切片；已按流程先记录进行中，完成后对齐 Dialog 标题/描述、测试标识、选中即关闭与移除独立完成按钮，并补充定向单测。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 ProviderActions 按钮文案资源化与组件单测；仅移除 6 个指定按钮文案中文 fallback，并补充资源化来源断言。
- ✅ 已完成（2026-07-05）：按白名单最小切片迁移 ProviderKeyField 编辑态锁定/加载提示行为；仅调整共享字段锁定判定、表单传参与组件单测，未迁移任何明确排除的非目标能力。

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
