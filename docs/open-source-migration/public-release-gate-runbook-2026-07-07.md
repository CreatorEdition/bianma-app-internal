# 公开发布门禁 Runbook（2026-07-07）

## 当前结论

`bianma-app` 已经是唯一正式开源主仓，但正式公开打包发布仍处于 **blocked** 状态。当前 `.github/workflows/release.yml` 必须保持 `Public Release Preflight`，只允许手动预检，不得创建 GitHub Release、不得上传 `latest.json`、不得引入 product 私有签名和 notarization 链路。

## 必须补齐的人工门禁

1. **版本策略**
   - 明确从 `0.0.1` 进入正式版本的规则。
   - 同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 与 changelog。

2. **签名与 notarization**
   - Windows、macOS、Linux 分别定义签名要求。
   - macOS 需要独立确认 Developer ID、notarytool、公证回读和 Gatekeeper 验证。
   - 不能直接复制 `bianma-app-product` 的 secret 名称和私有 workflow。

3. **Updater 与 `latest.json`**
   - `src-tauri/tauri.conf.json` endpoint 当前指向 `CreatorEdition/bianma-app`。
   - 正式上传前必须先验证 `latest.json` 生成来源、签名、公钥匹配和回滚策略。

4. **跨平台构建矩阵**
   - 至少明确 Windows x64、macOS arm64/x64、Linux x64 的构建与验收方式。
   - Linux ARM、Flatpak、deb/rpm 是否首发支持必须单独裁决。

5. **Release artifact 与人工审批**
   - 每个 artifact 必须有校验和。
   - 发布前必须有人审查 release notes、下载链接、updater endpoint、签名状态和安装验证记录。

## 当前已自动化的版本门禁

正式版本策略获批前，公开仓保持 `0.0.1` 占位版本。`scripts/audit-public-release-preflight.mjs` 会同时检查：

- `package.json`、`src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json` 的版本号必须一致。
- 当前版本必须继续是 `0.0.1`，避免在签名、notarization、`latest.json`、构建矩阵和人工审批缺失时误发正式版本。
- 后续升级到正式版本时，必须先更新本 runbook、changelog 与人工发布审批记录，再调整该预检门禁。

## 当前已自动化的审批门禁

公开发布人工审批清单见 `docs/open-source-migration/public-release-approval-checklist-2026-07-08.md`。在真实发布链路落地前，预检脚本会强制：

- checklist 文件必须存在。
- checklist 必须保持 `Status: BLOCKED`。
- checklist 中不得出现已勾选审批项。
- checklist 必须覆盖版本策略、Windows 签名、macOS 签名与 notarization、Linux 打包、`latest.json`、构建矩阵、artifact manifest 和人工审批记录。

## 当前允许的自动检查

运行：

```powershell
node scripts/audit-public-release-preflight.mjs
```

该脚本只证明当前公开仓仍处于安全预检状态，不证明已经可以正式发布。它会检查：

- `.github/workflows/release.yml` 仍是 `Public Release Preflight`。
- workflow 显式声明 `permissions: contents: read`。
- workflow 只保留 `workflow_dispatch`，没有 tag/push、`workflow_run`、`schedule` 或 `release` 发布触发。
- workflow 没有 `contents: write`、`id-token: write`、`gh release upload/create/edit`、GitHub Release action、`tauri-apps/tauri-action` 或 `actions/upload-artifact`。
- workflow 没有 `TAURI_SIGNING_PRIVATE_KEY`、`APPLE_CERTIFICATE`、`APPLE_PASSWORD`、`APPLE_ID`、`APPLE_TEAM_ID`、`KEYCHAIN_PASSWORD`、`notarytool`、`GH_TOKEN`、`GITHUB_TOKEN`、`RELEASE_REPO` 等 product 私有发布链路。
- Tauri updater endpoint 仍指向 `CreatorEdition/bianma-app`，且不指向 `bianma-app-product`、`data.bianma.ai`、internal 或 private 通道。
- `package.json`、`src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json` 的版本号一致，并继续保持 `0.0.1` 占位版本。
- 公开发布人工审批 checklist 存在，且继续保持 blocked / 未勾选状态。
- `ccswitch` legacy deep link scheme 仍保留。

## 禁止事项

- 禁止把 `bianma-app-product/.github/workflows/release.yml` 直接复制到公开仓。
- 禁止在公开 workflow 中写入真实 secret、证书、私钥、token 或私有仓地址。
- 禁止在未完成人工门禁前上传 `latest.json`。
- 禁止移除 Flatpak legacy ID、`Exec=cc-switch`、`cc-switch.deb` 或 `ccswitch` scheme。
