# 敏感与私有材料复查（2026-07-07）

## 当前结论

`bianma-app` 当前可以继续作为唯一开源主仓推进，但合入 `bianma-app-product` 内容时仍必须保持白名单迁移。`bianma-app-product` 中仍存在内部任务记录、私有规格、远端规则中心、Session Cloud、Risk Guard、Local Policy、Keyword Guard、多 key 池、合作方推广材料、测试运行残留与私有发布链路，不能整目录或按文件相似度直接迁入。

本轮同时清理了公开仓中少量测试夹具的 token-like 字符串：Rust 单测不再使用 `sk-*` 或 `AIza*` 形态示例，避免开源密钥扫描误报。公开仓复扫后，token-like 命中只剩 `src/types/omo.ts` 的 OMO detector 标识符误报。

已核验例外：`src-tauri/src/services/subscription.rs` 存在 `GEMINI_OAUTH_CLIENT_SECRET` 形态常量。该值已通过 GitHub 代码搜索和上游源码回读确认存在于 `google-gemini/gemini-cli` 公开仓 `packages/core/src/code_assist/oauth2.ts`（固定取证 URL：`https://github.com/google-gemini/gemini-cli/blob/15a9429b69bd4c72514678ac17c88087f7ab9d48/packages/core/src/code_assist/oauth2.ts`），上游注释说明它属于 installed application OAuth client，可随源码保存，不按服务端密钥处理。本仓保留该常量以兼容 Gemini refresh token 刷新流程，但不得把该例外扩大到 product 私有凭据、发布 secret 或服务端密钥。

## 已执行取证

主线程本地取证：

```powershell
rg -l -i --hidden -g '!**/.git/**' -g '!**/node_modules/**' -g '!**/target/**' -g '!**/dist/**' -g '!**/.codex*.json' -e data\.bianma\.ai -e docs/internal-spec -e private-uri -e providerRuleRegistry -e providerRuleCenter -e SessionCloud -e 'Risk Guard' -e 'Local Policy' -e 'Keyword Guard' -e apiKeyPool ..\bianma-app-product
rg -l -i --hidden -g '!**/.git/**' -g '!**/node_modules/**' -g '!**/target/**' -g '!**/dist/**' -g '!**/.codex*.json' -e latest\.json -e notarytool -e TAURI_SIGNING_PRIVATE_KEY -e APPLE_CERTIFICATE -e APPLE_PASSWORD -e APPLE_ID -e APPLE_TEAM_ID -e KEYCHAIN_PASSWORD -e RELEASE_REPO -e release-assets ..\bianma-app-product
rg -l -i --hidden -g '!**/.git/**' -g '!**/node_modules/**' -g '!**/target/**' -g '!**/dist/**' -g '!**/.codex*.json' -e partnerPromotion -e affiliate -e referral -e sponsor ..\bianma-app-product
rg -l -i --hidden -g '!**/.git/**' -g '!**/node_modules/**' -g '!**/target/**' -g '!**/dist/**' -e data\.bianma\.ai -e providerRuleRegistry -e providerRuleCenter -e SessionCloud -e 'Risk Guard' -e 'Local Policy' -e 'Keyword Guard' -e apiKeyPool -e docs/internal-spec -e TAURI_SIGNING_PRIVATE_KEY -e APPLE_CERTIFICATE -e APPLE_PASSWORD -e notarytool -e RELEASE_REPO .
rg -n --hidden -g '!**/.git/**' -g '!**/node_modules/**' -g '!**/target/**' -g '!**/dist/**' -e 'sk-[A-Za-z0-9]{8,}' -e 'AIza[0-9A-Za-z_-]{10,}' .
```

子代理独立复核【CodeX-Subagent】：只读扫描 `bianma-app` 与 `bianma-app-product`，补充确认 product release workflow、`.teamwork/**`、`docs/internal-spec/**`、`.codex-vitest*.json`、provider rule center、Session Cloud、Risk/Local Policy/Keyword Guard、合作方材料与 token-like 示例均不应直接迁入；同时提示公开仓 `GEMINI_OAUTH_CLIENT_SECRET` 需要单独裁决。主线程已追加上游源码核验并将其收口为 installed application OAuth client 例外。

## product 侧继续禁止迁入

| 风险类别               | 证据路径                                                                                                                              | 处理结论                                                                                                                                   |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| 内部任务与交接记录     | `bianma-app-product/.teamwork/**`                                                                                                     | 不迁入公开仓；只可作为本地审计输入。                                                                                                       |
| 私有规格与协议         | `bianma-app-product/docs/internal-spec/**`                                                                                            | 不整目录迁入；公开文档只能重写中性结论。                                                                                                   |
| 测试运行残留           | `bianma-app-product/.codex-vitest*.json`                                                                                              | 不迁入；属于已跟踪测试运行结果和内部测试上下文。                                                                                           |
| 私有发布链路           | `bianma-app-product/.github/workflows/release.yml`                                                                                    | 不复制；该 workflow 使用 tag push、`contents: write`、Tauri signing、Apple certificate、notarization、`RELEASE_REPO` 与 release 上传链路。 |
| updater / release 规划 | `docs/internal-spec/product/repo-boundary-and-release-channel.md`、`release-distribution-autoupdate-layering-plan.md`                 | 不迁入；公开仓只保留独立 runbook 与预检脚本。                                                                                              |
| 远端规则中心           | `src/lib/providerRuleCenter.ts`、`src/components/providers/forms/providerRuleRegistry.ts`、`tests/utils/providerRuleCenter.test.ts`   | 不迁入；包含 `data.bianma.ai`、partners index 与远端规则口径。                                                                             |
| Session Cloud          | `src/components/sessions/SessionCloudPanel.tsx`、`SessionCloudSnapshotDialog.tsx`、`src-tauri/src/services/session_cloud.rs`          | 不迁入；公开仓继续只承接本地会话能力。                                                                                                     |
| 本地策略与风险能力     | `KeywordGuardSettings.tsx`、`LocalPolicyRecentActivity.tsx`、`src-tauri/src/services/risk/**`、`src-tauri/src/proxy/keyword_guard.rs` | 不迁入；需要独立威胁模型和权限边界。                                                                                                       |
| 多 key 池              | `providerApiKeyPoolUtils.ts`、`ProviderForm.tsx` 相关路径                                                                             | 不迁入；涉及密钥选择、持久化与展示边界。                                                                                                   |
| 合作方/推广材料        | provider presets、`partnerPromotionKey`、README、release notes、affiliate/referral/sponsor 命中                                       | 不新增运行时推广展示；历史 release notes 仅作为归档事实保留。                                                                              |
| token-like 示例        | product `deplink.html`、Rust/前端测试中的 API key 形态字符串                                                                          | 不直接迁入；如需参考测试，必须改写为非密钥形态。                                                                                           |

## 公开仓当前例外与待裁决

以下命中不是泄露，不应误删：

- `src-tauri/src/usage_script.rs` 的 `is_private_ip` 是 SSRF/私网访问防护逻辑。
- `src-tauri/src/proxy/body_filter.rs` 的 `filter_private_params` 是请求体敏感字段过滤逻辑。
- `src-tauri/src/proxy/providers/copilot_auth.rs` 的 `copilot_internal` 是 GitHub Copilot API 路径片段，不是 Bianma 内部域名。
- Flatpak legacy ID、`Exec=cc-switch`、`cc-switch.deb` 与 `ccswitch` scheme 仍需保留，避免破坏已安装用户迁移。
- 公开仓现有 `partnerPromotionKey` 只作为 provider preset 元数据和历史兼容输入存在；不得据此恢复 API Key 区域促销展示或合作方星标入口。

已核验例外：

- `src-tauri/src/services/subscription.rs` 的 `GEMINI_OAUTH_CLIENT_SECRET` 来自 Gemini CLI 公开源码中的 installed application OAuth client。保留理由仅限兼容 Gemini CLI refresh token 刷新流程；新增 OAuth client、服务端 secret 或 product 私有凭据不得引用此例外。

## 本轮公开仓修正

- `src-tauri/src/proxy/providers/auth.rs`：`test_masked_key_long` 改用非密钥形态占位字符串。
- `src-tauri/src/proxy/providers/gemini.rs`：Gemini API key 提取测试改用非 `AIza*` 形态占位字符串。

## 后续门禁

后续任何 product 切片进入公开仓前，必须同时满足：

1. 先跑敏感词扫描，至少覆盖 private/internal URL、发布 secret、token-like 示例、合作方材料、规则中心、Session Cloud、Risk Guard、Local Policy、Keyword Guard 与多 key 池。
2. 对命中的 product 文件逐项标注“禁止迁入 / 可脱敏重写 / 可保留兼容”。
3. 新增测试夹具不得使用真实供应商密钥前缀形态。
4. 发布 workflow 只能从公开门禁重新设计，不能复制 product 私有发布流水线。
5. 主线程必须独立复核子代理结论，不能把子代理输出当作完成证据。
