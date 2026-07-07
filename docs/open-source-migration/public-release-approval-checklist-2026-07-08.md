# 公开发布人工审批 Checklist（2026-07-08）

Status: BLOCKED
release_gate_status: blocked
allows_real_release: false
product_release_workflow_import: forbidden

本 checklist 是公开发布前的人工审批占位，不是发布授权。正式发布前必须逐项补齐证据、负责人和回读记录；在全部门禁完成前，`.github/workflows/release.yml` 仍必须保持 `Public Release Preflight`，不得创建 GitHub Release、不得上传 `latest.json`，不得引入 product 私有签名或 notarization 链路。

## 必须完成的审批项

- [ ] Version strategy approved：明确从 `0.0.1` 进入正式版本的规则，并同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 与 changelog。
- [ ] Windows signing verified：确认 Windows x64 签名方案、证书来源、安装验证和回滚记录。
- [ ] macOS signing and notarization verified：确认 macOS arm64/x64 Developer ID 签名、公证回读、Gatekeeper 验证和失败回滚策略。
- [ ] Linux packaging verified：确认 Linux x64 AppImage/deb/rpm/Flatpak 支持范围、校验和和安装验证记录。
- [ ] latest.json approval verified：确认 `latest.json` 生成来源、签名、公钥匹配、回滚策略和人工上传审批。
- [ ] Build matrix verified：确认 Windows x64、macOS arm64/x64、Linux x64 构建矩阵及首发平台范围。
- [ ] Artifact manifest verified：确认每个 release artifact 的文件名、校验和、签名状态、下载链接和安装 smoke test 记录。
- [ ] Human approval recorded：记录审批人、审批时间、release notes 审查结论和最终发布窗口。

## 解锁规则

只有当上方所有审批项都有可追溯证据，并且主线程独立复核通过后，才允许将 `Status: BLOCKED` 改为发布审批状态。任何状态变更都必须同步更新 `docs/open-source-migration/public-release-gate-runbook-2026-07-07.md` 与 `scripts/audit-public-release-preflight.mjs`，并通过 PR 复核。
