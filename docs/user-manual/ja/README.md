# bianma-app ユーザーマニュアル

> bianma-app 公開ドキュメントの日本語ミラーです。対象ツールは Claude Code / Codex / Gemini CLI / OpenCode / OpenClaw です。

この日本語版は、中国語ドキュメントを正本とする翻訳ミラーです。

## 推奨の読み順

1. [中文ユーザーマニュアル](../zh/README.md) を公式主入口として確認
2. [1.1 ソフトウェア紹介](./1-getting-started/1.1-introduction.md)
3. [1.2 インストールガイド](./1-getting-started/1.2-installation.md)
4. [1.4 クイックスタート](./1-getting-started/1.4-quickstart.md)
5. [5.3 ディープリンクプロトコル（`bianma://`）](./5-faq/5.3-deeplink.md)
6. [移行互換ガイド（ZH）](../zh/5-faq/5.5-migration-compatibility.md)

## 公開ドキュメントの位置づけ

この公開リポジトリは、bianma-app のオープンソースコア、コミュニティ層、互換層、公開連携面を説明するためのものです。

- 公開ブランド: `bianma-app`
- 公開主プロトコル: `bianma://`
- 移行互換の詳細: [移行互換ガイド（ZH）](../zh/5-faq/5.5-migration-compatibility.md) を参照

## 主要リンク

- [中文ユーザーマニュアル](../zh/README.md)
- [bianma URI プロトコル](../../developers/bianma-uri-protocol.md)
- [移行互換ガイド（ZH）](../zh/5-faq/5.5-migration-compatibility.md)
- [CHANGELOG](../../../CHANGELOG.md)

`docs/release-notes/` 配下の過去リリースノートはアーカイブ用途であり、現行の主ドキュメント入口ではありません。

## ファイル一覧

### 1. はじめに

| ファイル | 内容 |
|------|------|
| [1.1-introduction.md](./1-getting-started/1.1-introduction.md) | ソフトウェア紹介、公開位置づけ、対応ツール |
| [1.2-installation.md](./1-getting-started/1.2-installation.md) | インストールとローカル開発セットアップ |
| [1.3-interface.md](./1-getting-started/1.3-interface.md) | インターフェースレイアウト、ナビゲーションバー、プロバイダーカードの説明 |
| [1.4-quickstart.md](./1-getting-started/1.4-quickstart.md) | 5 分でできるクイックスタートチュートリアル |
| [1.5-settings.md](./1-getting-started/1.5-settings.md) | 言語、テーマ、ディレクトリ、クラウド同期の設定 |

### 2. プロバイダー管理

| ファイル | 内容 |
|------|------|
| [2.1-add.md](./2-providers/2.1-add.md) | プリセットの使用、カスタム設定、統一プロバイダー |
| [2.2-switch.md](./2-providers/2.2-switch.md) | メイン画面での切り替え、トレイでの切り替え、反映方法 |
| [2.3-edit.md](./2-providers/2.3-edit.md) | 設定の編集、API Key の変更、バックフィル機能 |
| [2.4-sort-duplicate.md](./2-providers/2.4-sort-duplicate.md) | ドラッグで並べ替え、プロバイダーの複製、削除 |
| [2.5-usage-query.md](./2-providers/2.5-usage-query.md) | 使用量クエリ、残額表示、複数プラン表示 |

### 3. 拡張機能

| ファイル | 内容 |
|------|------|
| [3.1-mcp.md](./3-extensions/3.1-mcp.md) | MCP プロトコル、サーバーの追加、アプリバインド |
| [3.2-prompts.md](./3-extensions/3.2-prompts.md) | プリセットの作成、有効化の切り替え、スマートバックフィル |
| [3.3-skills.md](./3-extensions/3.3-skills.md) | スキルの発見、インストール・アンインストール、リポジトリ管理 |
| [3.4-sessions.md](./3-extensions/3.4-sessions.md) | セッションマネージャー：閲覧、検索、再開、削除 |
| [3.5-workspace.md](./3-extensions/3.5-workspace.md) | ワークスペースファイルとデイリーメモリー（OpenClaw） |

### 4. プロキシと高可用性

| ファイル | 内容 |
|------|------|
| [4.1-service.md](./4-proxy/4.1-service.md) | プロキシの起動、設定項目、実行状態 |
| [4.2-takeover.md](./4-proxy/4.2-takeover.md) | アプリケーション接管、設定変更、ステータス表示 |
| [4.3-failover.md](./4-proxy/4.3-failover.md) | フェイルオーバーキュー、サーキットブレーカー、ヘルスステータス |
| [4.4-usage.md](./4-proxy/4.4-usage.md) | 使用量統計、トレンドグラフ、料金設定 |
| [4.5-model-test.md](./4-proxy/4.5-model-test.md) | モデルテスト、ヘルスチェック、レイテンシテスト |

### 5. FAQ / プロトコル / 移行

| ファイル | 内容 |
|------|------|
| [5.1-config-files.md](./5-faq/5.1-config-files.md) | bianma-app のストレージ、CLI 設定ファイル形式 |
| [5.2-questions.md](./5-faq/5.2-questions.md) | よくある質問と回答 |
| [5.3-deeplink.md](./5-faq/5.3-deeplink.md) | 公開ディープリンクプロトコルと使い方 |
| [5.4-env-conflict.md](./5-faq/5.4-env-conflict.md) | 環境変数の競合検出と対処 |
| [../zh/5-faq/5.5-migration-compatibility.md](../zh/5-faq/5.5-migration-compatibility.md) | 移行互換ガイド（中国語主版） |

## コントリビュート

- [GitHub Issues](https://github.com/CreatorEdition/bianma-app-internal/issues)
- [GitHub Repository](https://github.com/CreatorEdition/bianma-app-internal)
