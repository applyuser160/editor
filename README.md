# Oxide Editor

Oxide Editor は、Rust/Tauri バックエンドと TypeScript/Monaco フロントエンドで構成されたデスクトップ IDE の試作です。現在の実装は、ローカルの単一ルートワークスペース、ファイル編集・検索、統合ターミナル、基本的な Git 操作、Open VSX の VSIX 管理を中心としています。VS Code の完全な移植や API 互換を提供するものではありません。

> **実装状況の読み方**: この README は現在提供される範囲を示します。設計書、要件書、調査書に記載する将来仕様および技術候補は、実装済み機能ではありません。機能ごとの根拠、未検証事項、ロードマップは [プロジェクト状況](docs/project-status.md) を参照してください。

## 現在の実装

| 領域 | 現在利用できる範囲 | 主な制限 |
|---|---|---|
| エディター UI | Monaco を利用した編集、タブ、基本的な 2 ペイン表示、クイックアクセス | 高度な Diff/Merge、Peek、Minimap などは未完了 |
| ローカルワークスペース | 1 つのローカルフォルダーの選択、最近使ったワークスペース、パス境界検証、ファイル操作・検索 | 信頼モデル、ユーザー定義除外、複数ルートは未実装 |
| 統合ターミナルと Git | PTY ベースのターミナル、Git 状態表示、ステージング、コミット、pull/push、ブランチ操作 | タスク実行とテストエクスプローラーは未実装 |
| 言語支援 | LSP 通信の基盤と Monaco の言語機能 | DAP デバッグ、複数ファイル WorkspaceEdit の安全な適用は未完了 |
| 拡張機能 | Open VSX の検索、VSIX の検証・展開・永続化、有効・無効化、削除 | Extension Host、activation event、任意拡張コードの実行、VS Code API 互換は提供しない |
| 設定 | UI 上の設定、キーバインド、プロファイルとワークスペースセッションの保存 | 現在はブラウザの localStorage を利用しており、`settings.json` / `keybindings.json` のファイル互換はない |

## 未検証の項目

起動時間、メモリ使用量、入力遅延、フレームレート、大容量ファイル性能、検索性能、およびクロスプラットフォーム対応状況について、再現可能なベンチマーク結果は公開していません。これらの数値を製品特性として扱わないでください。測定を導入する際の前提条件と公開基準は [プロジェクト状況の検証方針](docs/project-status.md#性能互換性セキュリティの検証方針) に記載しています。

## ロードマップ

次の Issue は、現在の制約を解消するための追跡先です。Issue の受け入れ条件を満たす変更がマージされるまで、対応機能が利用可能であるとは扱いません。

| 領域 | 追跡 Issue |
|---|---|
| 設定ファイルと資格情報ストア | [#63](https://github.com/applyuser160/editor/issues/63) |
| ワークスペース信頼、除外規則、複数ルート | [#62](https://github.com/applyuser160/editor/issues/62) |
| フロントエンドテストと PR 品質ゲート | [#64](https://github.com/applyuser160/editor/issues/64) |
| テストエクスプローラー | [#44](https://github.com/applyuser160/editor/issues/44) |
| タスク実行 | [#40](https://github.com/applyuser160/editor/issues/40) |
| デバッグ基盤 | [#39](https://github.com/applyuser160/editor/issues/39) |
| 拡張機能の実行基盤 | [#38](https://github.com/applyuser160/editor/issues/38) |
| 高度なエディター体験 | [#47](https://github.com/applyuser160/editor/issues/47) |

## ドキュメント

| 文書 | 位置付け |
|---|---|
| [プロジェクト状況](docs/project-status.md) | 実装状況、既知の制限、ロードマップ、検証方針の基準文書 |
| [ワークスペースモデル](docs/workspace-model.md) | 現在の単一ルートモデルと将来予約している形式 |
| [拡張機能ライフサイクル](docs/extension-lifecycle.md) | VSIX 管理の現行実装と、実行しない機能の境界 |
| [ネイティブ機能の信頼境界](docs/security/native-security-boundaries.md) | 現在のセキュリティ境界と既知の制限 |
| [ADR 一覧](docs/adr/README.md) | 採用済み・延期・提案中のアーキテクチャ判断 |
| [要件書](docs/requirements/) と [設計書](docs/design/) | 将来仕様。実装済みを示すものではない |
| [調査書](docs/research/) | 技術選定時の調査記録。現行ベンチマークや実装状況を示すものではない |
| [チェックリスト](docs/checklist/) | 文書監査・検証の追跡表。実装機能の一覧ではない |

## 開発

```bash
npm ci
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

テストランナーと PR 向け品質ゲートは未導入です。導入状況は [#64](https://github.com/applyuser160/editor/issues/64) で追跡しています。

## 追跡対象ファイル

`welcome.rs` は、起動時にエディターへ表示する Rust の歓迎用サンプルです。アプリケーションコードではなく、初期編集体験のためのフィクスチャとしてリポジトリ直下に置いています。用途のない `sample.py` および `scratch_patch.py` は追跡対象から除外しました。

## ライセンス

リポジトリには現在ライセンス本文がありません。配布または外部利用の前に、適用するライセンスを明示してください。
