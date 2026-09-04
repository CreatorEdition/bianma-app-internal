# bianma-app

bianma-app は Claude Code、Codex CLI、Gemini CLI、OpenCode、OpenClaw などの AI CLI を統合的に管理するコマンドラインツールで、拡張・設定・プロキシの操作を一か所で完結させます。対外的なブランドおよびドキュメントの起点はすべて bianma-app です。

> **中文主入口**：bianma-app では中文ユーザーマニュアルを公式ファーストとして扱い、English と日本語版は翻訳ミラーとして参考にしてください。

## 概要
CLI から直接複数のモデルや URI を切り替え、統一された設定体験とエクステンション管理を提供します。主線は bianma-app にあり、各種既存ツールはこの CLI の背後で動作します。

## セットアップ
1. Node.js 18+ と `pnpm` をインストールします。
2. リポジトリで `pnpm install` を実行して依存を取得します。
3. `pnpm dev` で Hot Reload 対応の開発サーバーを立ち上げます。
4. 開発用の環境変数を追加したい場合は、必要に応じてリポジトリ直下に `.env` を作成し、必要な変数を設定してください。

## ドキュメント & リソース
- [中文ユーザーマニュアル](docs/user-manual/zh/README.md)（公式主入口）
- [bianma URI プロトコル](docs/developers/bianma-uri-protocol.md)
- [更新履歴（履歴）](CHANGELOG.md)
- [セキュリティポリシー](SECURITY.md)

## ドキュメント入口（中文優先）
- [中文ユーザーマニュアル](docs/user-manual/zh/README.md)（推奨）
- [English user manual](docs/user-manual/en/README.md)（参照用）
- [日本語ユーザーマニュアル](docs/user-manual/ja/README.md)（参照用）

## 互換性メモ
- [移行互換ガイド](docs/user-manual/zh/5-faq/5.5-migration-compatibility.md)（移行・互換用途のみ）

## テスト & 品質
- `pnpm typecheck`
- `pnpm format:check`
- `pnpm test:unit`

## Contributing
Issue や Pull Request を通じて貢献できます。まず [Issue](https://github.com/CreatorEdition/bianma-app/issues) で相談し、PR を作成してください。
