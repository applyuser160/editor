# 実装状況とロードマップ

> **最終確認:** 2026-08-27、`a228692` 時点。
> この文書は、リポジトリ中のソースコード、依存関係、およびテストを基に確認できる範囲を示します。要件定義書・設計書・ADRに記載された目標は、個別に「実装済み」と明記されない限り、将来の設計または計画です。

## 表記

| 状態 | 意味 |
|---|---|
| **実装済み** | リポジトリの現行コードと依存関係から機能の実装を確認できる。すべての利用環境での動作保証や性能保証を意味しない。 |
| **部分実装** | 基盤または限定された操作は実装済みだが、受け入れ条件の全ては満たしていない。 |
| **計画** | 要件または設計として記録済みだが、現行コードでは実装を確認できない。 |
| **未検証** | 実装の有無にかかわらず、再現可能な測定・E2E検証・セキュリティ検証を確認できない。 |

## 現在実装されている範囲

| 領域 | 状態 | 確認できる範囲 | 主な実装箇所 |
|---|---|---|---|
| デスクトップシェル | **実装済み** | Tauri v2 と WebView 上の TypeScript UI を起動する。 | `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml` |
| テキスト編集 | **実装済み** | Monaco Editor による編集と、ネイティブコマンド経由のワークスペース内ファイル CRUD を提供する。 | `src/main.ts`, `src-tauri/src/commands.rs` |
| ワークスペース | **部分実装** | 単一ルートの選択、最近使ったワークスペース、ルート境界の検証を提供する。複数ルートと信頼モデルは未実装である。 | `src-tauri/src/workspace.rs`, [Issue #62](https://github.com/applyuser160/editor/issues/62) |
| 検索・ファイル監視 | **部分実装** | ワークスペース内検索と、ローカルファイル変更の監視を提供する。除外規則と大規模検索の性能保証は未実装・未検証である。 | `src-tauri/src/commands.rs`, `src-tauri/src/file_watcher.rs` |
| 統合ターミナル | **部分実装** | `portable-pty` と xterm.js を利用した PTY の生成、入出力、リサイズを提供する。名前付きタスク実行基盤は別途対応する。 | `src-tauri/src/pty_manager.rs`, [Issue #40](https://github.com/applyuser160/editor/issues/40) |
| Git 操作 | **部分実装** | ステータス、ブランチ、ステージング、コミット、プッシュ、プルのコマンドを提供する。高度な差分・マージ体験は未実装である。 | `src-tauri/src/commands.rs`, [Issue #47](https://github.com/applyuser160/editor/issues/47) |
| LSP 連携 | **部分実装** | 言語サーバーの起動・停止と JSON-RPC の通知・リクエストを提供する。ライフサイクル、診断、全言語互換性は継続対応である。 | `src-tauri/src/lsp_client.rs` |
| VSIX 管理 | **部分実装** | VSIX のサイズ・パス・マニフェスト検証、展開、永続化、有効・無効、削除を提供する。拡張機能 API の実行は有効化していない。 | `src-tauri/src/extension_host.rs`, [Issue #38](https://github.com/applyuser160/editor/issues/38) |
| 設定・キーバインド | **部分実装** | テーマ、フォントサイズ、タブ幅、ミニマップ、キーバインド、プロファイルを `localStorage` に保存する。外部編集可能な JSON ファイル永続化と資格情報ストアは未実装である。 | `src/settings.ts`, [Issue #63](https://github.com/applyuser160/editor/issues/63) |
| リリース | **部分実装** | タグ向けの Linux リリースワークフローが存在する。各 OS の署名、更新、配布の完全なパイプラインは未実装である。 | `.github/workflows/` , [Issue #48](https://github.com/applyuser160/editor/issues/48) |

## 現在の対象外または未実装の設計

| 設計・主張 | 状態 | 補足 |
|---|---|---|
| WGPU / Vello によるネイティブ GPU レンダリング | **計画** | 現行 UI は Tauri WebView と Monaco Editor を使用する。`wgpu` および `vello` は依存関係に含まれない。 |
| `ropey` によるネイティブテキストバッファ | **計画** | 現行の編集コアは Monaco Editor であり、`ropey` は依存関係に含まれない。 |
| Tree-sitter による構文解析 | **計画** | `tree-sitter` および言語グラマーは依存関係に含まれない。 |
| WASM / WASI 拡張機能サンドボックス | **計画** | Wasmtime / Extism 等のランタイムは依存関係に含まれず、拡張機能 API 実行は有効化していない。 |
| VS Code 拡張機能 API との完全互換 | **計画** | VSIX の取得・管理と、`vscode.*` API の実行・互換性は別の課題である。 |
| VS Code 設定の完全インポート互換 | **計画** | 現行設定形式は Oxide Editor 独自の `localStorage` 形式である。 |

## 検証状況

性能、メモリ使用量、起動時間、対応 OS、長時間安定性、拡張機能隔離、および VS Code 互換性について、リポジトリには受け入れ判断に足る再現可能なベンチマークまたは E2E 検証手順がまだないため、**未検証**です。数値や互換性を外部に示す前に、測定環境、入力データ、実行手順、結果、および測定対象のコミットを公開してください。

## 追跡

実装の拡張・品質保証は、[オープンIssue一覧](https://github.com/applyuser160/editor/issues)で追跡します。特に、設定永続化は [#63](https://github.com/applyuser160/editor/issues/63)、テストと PR 品質ゲートは [#64](https://github.com/applyuser160/editor/issues/64)、拡張機能実行基盤は [#38](https://github.com/applyuser160/editor/issues/38) を参照してください。

## 検証の実行

現行のローカル検証は、次のコマンドを基準とします。

```bash
npm run build
cargo test
```

これらは型検査、フロントエンドのプロダクションビルド、および Rust の単体テストを実行します。フロントエンドの専用テストランナーと PR ごとの品質ゲートは、[Issue #64](https://github.com/applyuser160/editor/issues/64) で追跡しています。

## 管理対象外ファイル

リポジトリ直下の `sample.py`、`scratch_patch.py`、`welcome.rs` は、製品コード、テスト、または文書から参照されない作業用ファイルです。本変更で削除します。利用者向けの動作例は、再現可能なテストまたは `examples/` 以下に用途・実行方法を明記して追加してください。
