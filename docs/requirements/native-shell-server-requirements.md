# VS Code Native Shell & Server レイヤー 機能要件定義書 (ISO 29148 準拠)

> **文書状態: 計画（部分実装あり）。** 本ドキュメントは、Microsoft VS Code のネイティブシェル (`src/vs/code/`)、リモートサーバー (`src/vs/server/`)、および組み込み拡張機能 (`extensions/`) を Rust ネイティブ環境へ移植する場合の機能要件定義書です。現行実装にはTauriデスクトップシェルが含まれますが、リモート開発とVS Code組み込み拡張機能の互換性は未実装です。現在提供する機能は[実装状況とロードマップ](../implementation-status.md)を参照してください。

---

## 1. 概要とスコープ

`src/vs/code/` は Electron の Main プロセス（シングルインスタンス制御、ウィンドウ生成、OS ネイティブメニュー、プロトコルハンドラ）、`src/vs/server/` は SSH/WSL/Container/Tunnel によるリモート開発基盤、`extensions/` は標準提供される Git、Markdown、テーマ、言語定義を提供します。

---

## 2. 機能要件一覧 (Functional Requirements)

### REQ-SHELL-001: ネイティブウィンドウ & ライフサイクル (Native Shell & Lifecycle)
- **説明:** OS ネイティブのデスクトップウィンドウライフサイクルと高速起動。
- **詳細要件:**
  - 単一インスタンスロック（Single Instance Lock）と既存ウィンドウへの引数ルーティング (`code path/to/file`)。
  - OS ネイティブメニューバー（Windows / macOS / Linux）との完全統合。
  - クラッシュレポートおよびセッション自動復旧。

### REQ-SHELL-002: リモート開発 & トンネリング (Remote Development & Server)
- **説明:** リモートサーバー、WSL、コンテナ、SSH 上でのヘッドレス実行とローカル UI クライアントの接続。
- **詳細要件:**
  - ヘッドレス Oxide Server によるリモートファイル I/O、PTY 端末、LSP 実行。
  - WebSocket / TCP ストリームによるセキュアな双方向多重化プロトコル。
  - ポートフォワーディングおよび自動再接続。

### REQ-SHELL-003: 標準組み込み言語・テーマ機能 (Built-in Extensions)
- **説明:** インストール直後から主要言語の構文ハイライトと標準テーマが動作する。
- **詳細要件:**
  - TextMate 文法定義（`.tmLanguage.json`）による主要言語（Rust, TypeScript, Python, C/C++, HTML, JSON 等）の標準ハイライト。
  - VS Code 標準カラーテーマ（Dark+, Light+, Monokai, Solarized Dark, High Contrast）の同梱。
