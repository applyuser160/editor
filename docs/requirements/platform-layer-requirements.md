# VS Code Platform サービス層 機能要件定義書 (ISO 29148 準拠)

> **文書状態: 計画（未実装の要件を含む）。** 本ドキュメントは、Microsoft VS Code のプラットフォームサービス層 (`src/vs/platform/`) を Rust ネイティブ環境へ移植する場合の機能要件定義書です。要件の記載は実装完了を意味しません。現在提供する機能は[実装状況とロードマップ](../implementation-status.md)を参照してください。

---

## 1. 概要とスコープ

`src/vs/platform/` は、VS Code のすべての機能（エディタ、サイドバー、ターミナル、プラグイン等）が利用する共通サービス基盤（DI コンテナ、設定管理、コンテキストキー、ファイルシステム、コマンドレジストリ、キーバインド解決、ストレージ、テーマ）を提供します。

---

## 2. 機能要件一覧 (Functional Requirements)

### REQ-PLAT-001: 依存性注入コンテナ (Dependency Injection / Instantiation)
- **説明:** 疎結合なモジュール設計とサービス依存の自動解決を実現する DI サービス。
- **詳細要件:**
  - `ServiceCollection` によるシングルトンサービスの登録。
  - `IInstantiationService` によるサービスの遅延インスタンス化と依存グラフの自動解決。
  - 循環依存の検出とエラーハンドリング。

### REQ-PLAT-002: 設定エンジン (Configuration Service & settings.json)
- **説明:** ユーザー設定、ワークスペース設定、フォルダ設定を階層的に管理・マージする。
- **詳細要件:**
  - JSON スキーマに基づく `settings.json` の型安全なバリデーション。
  - 設定変更イベント (`onDidChangeConfiguration`) のリアルタイム通知。
  - デフォルト設定、グローバル設定、ワークスペース設定の階層的オーバーライド。

### REQ-PLAT-003: コンテキストキー評価エンジン (ContextKey & When Clauses)
- **説明:** VS Code のすべての UI 要素（メニュー、ボタン、キーバインド）の有効・無効を動的に制御する `when` 句評価器。
- **詳細要件:**
  - `when` 式の AST 構文解析（`editorTextFocus && !editorReadonly || resourceExt == '.rs'`）。
  - 変数比較演算子（`==`, `!=`, `=~`, `in`）の完全サポート。
  - コンテキスト状態変更時の高速再評価（キャッシュインデックス活用）。

### REQ-PLAT-004: コマンド & キーバインド中央管理 (Commands & Keybindings)
- **説明:** 全コマンドとキーボードショートカットの中央集中型ディスパッチシステム。
- **詳細要件:**
  - `CommandsRegistry`: 一意のコマンド ID に対するハンドラ登録とメタデータ（タイトル、カテゴリ）管理。
  - `KeybindingResolver`: OS（Windows / macOS / Linux）ごとのキーマップ解決と `when` 句による競合調停。
  - ユーザーカスタムキーバインド (`keybindings.json`) によるオーバーライド。

### REQ-PLAT-005: 統一ファイルサービス (Unified File Service)
- **説明:** ローカルファイル、リモートファイル、インメモリファイルを透過的に扱う抽象化ファイルシステム。
- **詳細要件:**
  - `IFileService` および `IFileSystemProvider` による CRUD 操作（Read, Write, Delete, Rename, Watch）。
  - ノンブロッキング非同期 I/O (`tokio::fs`)。
  - バックグラウンドファイル変更監視とエディタバッファへの通知連携。

### REQ-PLAT-006: 状態ストレージサービス (Storage & Memento)
- **説明:** ウィンドウ状態、直近開いたファイル、UI レイアウト状態の永続化。
- **詳細要件:**
  - アプリケーション全体（Global）およびワークスペース単位（Workspace）のキー・バリューストア。
  - 高速 SQLite / バイナリインデックスによるクラッシュ耐性と即時復元。
