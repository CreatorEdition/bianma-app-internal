# 公开 Updater / latest.json 门禁（2026-07-08）

Status: BLOCKED
latest_json_upload_allowed: false
release_artifact_upload_allowed: false
updater_manifest_generation_allowed: false
rollback_plan_status: pending
signature_key_match_status: pending

本文件只记录公开发布前的 updater 与 `latest.json` 人工门禁，不是上传授权。当前公开仓可以保留 Tauri updater artifact 构建能力用于后续人工评审，但公开 CI / release workflow 不得生成或上传 `latest.json`，仓库不得跟踪 `latest.json` 或根级 `release-assets/**`，也不得把 product 私有 release workflow、签名 secret 或私有发布仓地址迁入公开仓。本地构建可能产生未跟踪产物；预检脚本不把本地临时输出当作仓库提交，但这些产物仍不得提交或上传。

## 必须补齐的证据

- [ ] latest.json source verified：确认 `latest.json` 的生成来源、输入 artifact、版本号和平台矩阵。
- [ ] updater signature verified：确认 Tauri updater 签名、公钥、私钥保管方式和 key rotation 策略。
- [ ] public endpoint verified：确认 endpoint 仍为 `CreatorEdition/bianma-app`，且不指向 product、private 或 `data.bianma.ai`。
- [ ] rollback plan verified：确认坏版本回滚、撤回 release、客户端失败处理和人工公告流程。
- [ ] upload approval recorded：记录人工审批人、审批时间、校验和、下载链接和最终 go/no-go 结论。

## 当前硬阻断

- `latest_json_upload_allowed` 必须保持 `false`。
- `release_artifact_upload_allowed` 必须保持 `false`。
- `updater_manifest_generation_allowed` 必须保持 `false`。
- 所有公开 workflow 不得生成或上传 `latest.json`、release artifact 或 updater 上传产物。
- 仓库不得跟踪任何目录下的 `latest.json` 或根级 `release-assets/**`。
- 公开 workflow 不得新增 `gh release upload`、`actions/upload-artifact`、`tauri-apps/tauri-action`、`contents: write` 或 product 私有 signing/notarization secret。
