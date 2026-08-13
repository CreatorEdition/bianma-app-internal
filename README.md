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
- [更新日志（历史记录）](CHANGELOG.md)
- [安全政策](SECURITY.md)

## 文档入口（中文优先）

- [中文用户手册](docs/user-manual/zh/README.md)（首选入口，涵盖最新界面与使用流程）
- [English user manual](docs/user-manual/en/README.md)（英文镜像，供术语比对）
- [日本語ユーザーマニュアル](docs/user-manual/ja/README.md)（日文镜像，供日语读者参考）

## 兼容说明

仅保留[迁移兼容说明](docs/user-manual/zh/5-faq/5.5-migration-compatibility.md)作为历史兼容文档，用于帮助还在使用旧方案的团队过渡；其他场景均以 bianma-app 为主，新的体验与 URI 规范已经完全统一。

本地代理只允许绑定 `127.0.0.1`、`localhost` 或 `::1`；模型路由会拒绝带 `Origin` 的浏览器请求。该边界用于避免设备保存的上游凭据被局域网设备或网页借用。

## 统一路由核心

默认产品路径是一套本地路由中心：用户只需录入 API 地址、API Key 和默认模型，Claude Code、Codex、Gemini 等客户端共用同一份规范化模型路由。客户端专属模型映射、Header/User-Agent、同阶段均衡与多账户选择属于高级配置，不作为首次接入步骤。

`src-tauri/crates/routing-core` 是独立的纯 Rust Stage-first 规划器。它只使用不可变内存快照生成固定容量的 `A -> B -> C` 计划，热路径不访问数据库、文件或网络，不启动后台线程，也不持有账户或凭据。HTTP、429/重试、健康状态、Secret 和 ContextPipeline 将在后续独立切片接入。

## 测试与质量

声明参与贡献前先在本地运行以下命令确认状态：

- `pnpm typecheck`
- `pnpm format:check`
- `pnpm test:unit`
- `pnpm audit:product-migration`（检查后续 product 迁移切片是否命中禁迁边界）
