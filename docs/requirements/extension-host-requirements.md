# VS Code Extension Host (拡張機能基盤) 機能要件定義書 (ISO 29148 準拠)

> **文書状態: 計画（部分実装あり）。** 本ドキュメントは、VS Code の拡張機能ホスト基盤 (`src/vs/workbench/api/`) を Rust ネイティブ環境へ移植する場合の機能要件定義書です。現行実装にはVSIXの検証・展開・永続化が含まれますが、サンドボックス、拡張機能API、`vscode.*`互換は未実装です。現在提供する機能は[実装状況とロードマップ](../implementation-status.md)を参照してください。

---

## 1. 概要とスコープ

`src/vs/workbench/api/` は、VS Code の巨大な拡張機能エコシステム（Marketplace / `.vsix`）を実行可能にするためのサンドボックス基盤です。メイン UI プロセスから拡張機能を完全に分離し、UI フリーズを防ぎながら型安全な RPC プロトコルでエディタ操作や LSP・デバッガを連携させます。

---

## 2. 機能要件一覧 (Functional Requirements)

### REQ-EXT-001: プロセス分離サンドボックス (Process Isolation)
- **説明:** 拡張機能の不具合や重い処理が UI スレッドのフレームレート（120fps）に一切影響を与えない隔離環境。
- **詳細要件:**
  - 独立したワーカースレッドまたは別プロセス（WASM / QuickJS / V8）での拡張機能実行。
  - メインスレッドと Extension Host 間の双方向非同期 RPC プロトコル。
  - クラッシュ時の自動リカバリとリソース隔離。

### REQ-EXT-002: VS Code API (`vscode.*`) 互換エミュレータ
- **説明:** 公式の VS Code 拡張機能がそのまま動作するための API 実装。
- **詳細要件:**
  - `vscode.commands`: コマンド登録と実行。
  - `vscode.window`: テキストエディタ操作、メッセージ表示、QuickPick、ターミナル作成。
  - `vscode.workspace`: ドキュメント変更監視、設定読み書き、ファイルシステム操作。
  - `vscode.languages`: 補完プロバイダ、ホバープロバイダ、定義ジャンププロバイダ。

### REQ-EXT-003: VSIX パッケージ管理 (VSIX Package Loader)
- **説明:** Open VSX / Marketplace からの `.vsix` パッケージダウンロード・展開・依存関係解決。
- **詳細要件:**
  - `package.json`（Manifest）の `contributes`（themes, languages, grammars, commands）解析。
  - アクティベーションイベント（`onLanguage`, `onCommand`, `*`）に基づく遅延ロード。
