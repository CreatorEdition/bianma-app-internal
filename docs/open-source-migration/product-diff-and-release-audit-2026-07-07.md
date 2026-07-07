# bianma-app-product 差异与发布风险审计（2026-07-07）

## 审计结论

`bianma-app-product` 仍只能作为迁移源，不应作为整仓覆盖来源。当前取证显示：

- `bianma-app-product` 相对 `bianma-app` 仍有 202 个 product-only 路径。
- `bianma-app` 相对 `bianma-app-product` 有 39 个 app-only 路径，其中包括公开仓新增的 `flatpak/**`、`SECURITY.md`、`SUPPORT.md`、公开迁移文档与部分订阅/兼容切片。
- product-only 路径主要集中在 `.teamwork/**`、`docs/internal-spec/**`、规则中心、Session Cloud、Risk Guard、Local Policy、Keyword Guard、策略链、私有发布工作流与扩展语言包。
- 当前公开仓已经完成多轮白名单迁移，后续不能按文件名相似度直接复制 product 内容，必须继续按切片审计、脱敏、测试和主线程复核。

## 已执行取证

```powershell
git -C C:\code\bianma.ai\bianma-app status --short --branch
git -C C:\code\bianma.ai\bianma-app-product status --short --branch
git -C C:\code\bianma.ai\bianma-app ls-files
git -C C:\code\bianma.ai\bianma-app-product ls-files
rg -n "providerRuleRegistry|providerRuleCenter|data\.bianma\.ai|apiKeyPool|SessionCloud|Risk Guard|Local Policy|Keyword Guard|load-balanc|strategy|affiliate|referral|sponsor|partnerPromotion|TAURI_SIGNING_PRIVATE_KEY|APPLE_CERTIFICATE|APPLE_PASSWORD|notar|latest\.json|private key|BEGIN .*PRIVATE KEY|docs/internal-spec|subscription|quota" src src-tauri docs .github flatpak tests package.json task.md
```

当前仓库状态：

- `bianma-app`：`codex/open-source-consolidation-20260705...origin/codex/open-source-consolidation-20260705`，取证时工作树干净。
- `bianma-app-product`：`fix/test-env-lock-baseline`，取证时工作树未显示未提交文件。

## 明确不可直接迁移

以下 product-only 区域必须继续排除，除非后续有单独设计、脱敏和测试任务：

| 区域 | product 路径证据 | 处理结论 |
| --- | --- | --- |
| 多 Agent 内部记录 | `.teamwork/**` | 不迁入公开仓用户路径；仅可作为审计输入。 |
| 内部规格与私有规划 | `docs/internal-spec/**` | 不整目录迁入；只允许抽取公开中性结论后重写。 |
| 规则中心与远端规则 | `src/lib/providerRuleCenter.ts`、`src/components/providers/forms/providerRuleRegistry.ts`、`tests/utils/providerRuleCenter.test.ts` | 不迁入；包含 `data.bianma.ai`、远端规则和合作方元数据边界。 |
| 多 key 池 | `src/components/providers/forms/providerApiKeyPoolUtils.ts`、`ProviderForm.tsx` 中 `apiKeyPool` 路径 | 不迁入；涉及密钥池、选择模式和配置持久化边界。 |
| Session Cloud | `src/components/sessions/SessionCloudPanel.tsx`、`SessionCloudSnapshotDialog.tsx`、`src-tauri/src/services/session_cloud.rs` | 不迁入；当前公开仓保留本地会话能力，云端恢复链路需重新设计。 |
| Local Policy / Keyword Guard | `src/components/settings/KeywordGuardSettings.tsx`、`LocalPolicyRecentActivity.tsx`、`src-tauri/src/proxy/keyword_guard.rs` | 不迁入；属于本地策略/审计链路，需独立威胁模型。 |
| Risk Guard | `src/lib/risk/**`、`src-tauri/src/commands/risk.rs`、`src-tauri/src/services/risk/**` | 不迁入；涉及审批流、风险预览和策略执行边界。 |
| 后端策略链 | `src-tauri/src/proxy/strategy/**`、`src-tauri/src/database/dao/strategy.rs`、`docs/provider-strategy-chain.md` | 不迁入；公开仓当前不承接完整策略/load-balancing/failover 后端。 |
| 合作方/推广材料 | `partnerPromotionKey`、`affiliate/referral/sponsor`、合作方 release notes | 不新增运行时展示；历史归档仅保留事实，不做推广入口。 |
| 扩展语言包 | `src/i18n/locales/de.json`、`ko.json`、`pt-BR.json`、`zh-Hant.json` | 暂不迁入；公开仓当前只维护 zh/en/ja。 |
| 私有发布流水线 | product `.github/workflows/release.yml` | 不直接迁入；含签名、notarization、私有 release/latest.json 上传逻辑。 |

## 发布与 updater 差异

公开仓当前 `.github/workflows/release.yml` 是公开发布预检占位，只说明：

- 不创建 release。
- 不上传 updater artifact。
- 正式打包发布仍需签名、版本号、`latest.json`、跨平台构建与人工门禁。

product 的 `.github/workflows/release.yml` 是完整私有发布流水线，包含：

- tag 触发的 Windows / Linux / Linux ARM / macOS 构建矩阵。
- `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 处理。
- `APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_PASSWORD`、notarytool notarization。
- DMG notarization 与签名校验。
- private `latest.json` 生成和上传。

结论：公开仓只能基于 product workflow 重新设计公开发布门禁，不能直接复制该 workflow。

## 后续可考虑白名单切片

以下仅是后续候选，不代表已批准迁移：

1. **公开发布 runbook 切片**
   - 候选路径：新增公开文档或重写 `.github/workflows/release.yml` 的非发布预检逻辑。
   - 价值：把当前“发布仍需人工门禁”变成可执行清单。
   - 风险：不能引入 product secrets、私有 release repo 或自动上传。
   - 验证：`git diff --check`、workflow 文本扫描 `TAURI_SIGNING_PRIVATE_KEY|APPLE_CERTIFICATE|APPLE_PASSWORD|private release`。

2. **App shell 只读布局评估切片**
   - 候选路径：product `src/components/layout/AppShellLayout.tsx`、`AppWorkspaceContent.tsx`、`workspaceRenderers.tsx`、相关测试。
   - 价值：可评估是否有低风险 UI 结构复用价值。
   - 风险：product layout 与规则中心、策略页、Session Cloud、订阅入口耦合，不能整包迁移。
   - 验证：`tsc --noEmit`、相关组件测试、敏感词扫描 `providerRule|SessionCloud|Risk|Local Policy|subscription`。

3. **会话列表视觉拆分切片**
   - 候选路径：product `src/components/sessions/SessionListCard.tsx` 与相关测试。
   - 价值：可能改善公开仓本地会话列表可维护性。
   - 风险：必须剥离 Session Cloud、远端恢复和 Claude replay 入口。
   - 验证：`vitest run tests/components/SessionManagerPage.test.tsx`、`tsc --noEmit`。

4. **Provider 表单测试补强切片**
   - 候选路径：product 中不依赖规则中心/多 key 池的表单测试。
   - 价值：公开仓已有 Provider 表单工具抽取，可继续补基础测试。
   - 风险：不得迁入 `apiKeyPool`、`providerRuleRegistry`、合作方推广或远端规则断言。
   - 验证：定向 Vitest、`tsc --noEmit`、新增 diff 敏感词扫描。

5. **公开协议文档对齐切片**
   - 候选路径：product `docs/integrations/bianma-uri-protocol.md` 与公开仓 `docs/developers/bianma-uri-protocol.md` 的公开字段差异。
   - 价值：补齐公开 URI 使用说明。
   - 风险：不能迁入 `docs/internal-spec/api/bianma-private-uri-protocol-spec.md` 中的私有协议。
   - 验证：Markdown diff 人审、敏感词扫描 `private|internal|sign|token|secret`。

## 后续门禁

每个后续切片必须同时满足：

- 先读 `README.md` 与 `task.md`。
- 明确说明是否来自 product，列出排除路径。
- 主线程独立复核子代理结论。
- 不使用 `git add .`。
- 至少运行 `tsc --noEmit` 或与切片等价的强验证。
- 对新增 diff 扫描：

```powershell
git diff --cached --name-only
git diff --cached --check
git diff --cached | Select-String -Pattern 'TAURI_SIGNING_PRIVATE_KEY|BEGIN .*PRIVATE KEY|APPLE_CERTIFICATE|APPLE_PASSWORD|GITHUB_TOKEN|GH_TOKEN|providerRuleRegistry|providerRuleCenter|data\.bianma\.ai|apiKeyPool|SessionCloud|Risk Guard|Local Policy|Keyword Guard|affiliate|referral|sponsor|docs/internal-spec|\.teamwork'
```
