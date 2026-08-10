# bianma-app

bianma-app 是 Claude Code、Codex CLI、Gemini CLI、OpenCode 与 OpenClaw 等多种 AI 编码工具的统一命令行入口，集中管理扩展、代理与配置。所有对外推广、文档与开发体验都以 bianma-app 为品牌锚点。

## 仓库状态

`bianma-app` 现在是 Bianma 的唯一正式开源主仓。后续源码、公开协作、Release、Updater 与二进制分发默认都以本仓为准；历史 `bianma-app-product` 仅作为迁移源与待归档目录。

## 快速开始

1. 安装 [Node.js 18+](https://nodejs.org/) 与 `pnpm`。
2. 进入仓库后运行 `pnpm install` 以拉取依赖。
3. 使用 `pnpm dev` 启动本地开发服务器并热更新 UI。
4. 若需自定义开发环境变量，可按需在仓库根目录创建 `.env` 文件并自行添加所需变量。

## 资源导航

- [中文用户手册](docs/user-manual/zh/README.md)
- [bianma URI 协议文档](docs/developers/bianma-uri-protocol.md)
- [routing-core v2 与 ContextPipeline 架构及迁移规格](docs/developers/routing-core-v2-architecture.md)（提案，作为后续路由核心重建、发送前上下文治理与 A/B 验收的事实源）
- [更新日志（历史记录）](CHANGELOG.md)
- [安全政策](SECURITY.md)

## 文档入口（中文优先）

- [中文用户手册](docs/user-manual/zh/README.md)（首选入口，涵盖最新界面与使用流程）
- [English user manual](docs/user-manual/en/README.md)（英文镜像，供术语比对）
- [日本語ユーザーマニュアル](docs/user-manual/ja/README.md)（日文镜像，供日语读者参考）

## 兼容说明

仅保留[迁移兼容说明](docs/user-manual/zh/5-faq/5.5-migration-compatibility.md)作为历史兼容文档，用于帮助还在使用旧方案的团队过渡；其他场景均以 bianma-app 为主，新的体验与 URI 规范已经完全统一。

## 测试与质量

声明参与贡献前先在本地运行以下命令确认状态：

- `pnpm typecheck`
- `pnpm format:check`
- `pnpm test:unit`
- `pnpm audit:product-migration`（检查后续 product 迁移切片是否命中禁迁边界）
- `cargo test --manifest-path src-tauri/Cargo.toml -p routing-core --all-targets --locked`
- `cargo clippy --manifest-path src-tauri/Cargo.toml -p routing-core --all-targets --locked -- -D warnings`
- `cargo bench --manifest-path src-tauri/Cargo.toml -p routing-core --bench route_planner --locked`

`routing_v2::vault_adapter` 目前只是系统凭据库 capability PoC，不是生产 Vault，也不读取或迁移任何 Provider Secret。受控设备可显式运行其 ignored 测试；测试只写入随机 canary、读取验证后立即删除，不得在 CI 或非受控设备上自动执行：

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked -p bianma-app routing_v2::vault_adapter::tests::native_keyring_round_trip_writes_no_persistent_test_credential --lib -- --ignored
```

`routing_v2::vault` 是与 capability PoC 分离的纯内存加密基础：宿主 crate 显式锁定 `aes-gcm-siv 0.11.1`、`aes 0.8.4` 的 `zeroize` feature 与 `zeroize 1.8.2`，只验证既有 32-byte root key 的短生命周期 AEAD 使用。它不执行 keyring I/O、不自动创建或覆盖 root key、不持久化密文，也不接入 SQLite、迁移 Saga、Proxy、Tauri IPC 或任何 Provider Secret；密文格式不是后续持久化 Vault 协议。

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --locked -p bianma-app routing_v2::vault::tests --lib
```

`routing-core` 当前仍是未接入生产转发链的纯 Rust 领域切片：仅基于内存快照做闭集入站分类、有界路由计划与保守重试决策，不读取数据库/文件、不启动后台线程、不执行网络请求，也不承载 ContextPipeline；本地 compact 可被分类为本地 handler，但压缩、记忆和图的实现不会进入模型 RoutePlan 或该核心。
