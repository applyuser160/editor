# Microsoft VS Code アーキテクチャと Rust 移植マッピング仕様書

> 本ドキュメントは、Microsoft 公式リポジトリ [`microsoft/vscode`](https://github.com/microsoft/vscode) の内部ディレクトリ構成、レイヤー設計、主要サービスを Rust による高パフォーマンス・省メモリ実装へマッピングするための技術仕様書です。

---

## メタ情報

| 項目 | 値 |
|------|-----|
| プロジェクト名 | Oxide (VS Code Rust Port) |
| 対象元リポジトリ | [microsoft/vscode](https://github.com/microsoft/vscode) |
| バージョン | 1.0.0 |
| 作成日 | 2026-08-22 |
| 作成者 | Syun |
| ステータス | Approved |

---

## 1. VS Code レイヤー構造と Rust クレート対応表

```
VS Code Source Tree (TypeScript)                     Oxide Workspace (Rust)
├── src/vs/base/                                 ==> crates/editor-core / oxide-base
│   ├── common/ (lifecycle, event, cancellation)  ├── lifecycle (Disposable, Emitter, Event)
│   ├── browser/ (dom, keyboard, mouse)           ├── event (KeyBindings, MouseEvents)
│   └── node/ (pfs, extfs, processes)             └── io (Non-blocking FileSystem, PTY)
│
├── src/vs/platform/                             ==> crates/editor-workspace / oxide-platform
│   ├── instantiation/ (DI Service Container)    ├── di (ServiceCollection, InstantiationService)
│   ├── configuration/ (settings.json engine)     ├── configuration (SettingsEngine, schema)
│   ├── contextkey/ (when clauses evaluation)    ├── contextkey (ContextKeyEvaluator)
│   ├── files/ (IFileService, watcher)           ├── files (FileService, NotifyWatcher)
│   └── storage/ (memento, SQLite/JSON cache)    └── storage (StorageService, StateStore)
│
├── src/vs/editor/ (Monaco Core)                 ==> crates/editor-core / editor-syntax
│   ├── common/ (model, textModel, pieceTree)    ├── model (TextModel, RopeBuffer, Position)
│   ├── common/tokens/ (LineTokens, grammars)    ├── tokens (SyntaxTokens, TreeSitterAST)
│   ├── browser/view/ (viewLines, minimap)       ├── view (ViewLines, MinimapRenderer, Gutter)
│   └── contrib/ (suggest, find, gotoSymbol)     └── contrib (Completion, SearchReplace, QuickOutline)
│
├── src/vs/workbench/                            ==> crates/editor-ui / editor-app
│   ├── browser/parts/ (activitybar, sidebar)    ├── parts (ActivityBar, SideBar, EditorGroupGrid)
│   ├── browser/parts/panel/ (terminal, output)  ├── panel (TerminalPanel, OutputPanel, Problems)
│   └── services/ (editorService, viewletService)└── services (WorkbenchLayout, ThemeManager)
│
└── src/vs/workbench/api/ (Extension Host)       ==> crates/editor-plugin
    ├── common/ (extHostProtocol, rpcProtocol)   ├── protocol (ExtHostRpc, MessagePort)
    └── node/ (vscode.* API Implementations)     └── api (VsCodeApiBridge, VsixLoader)
```

---

## 2. コアレイヤー別詳細設計

### 2.1 Base & Platform (基盤・サービス層)
- **Lifecycle (Disposable Pattern):**
  - リソースの確実な解放とリーク防止を Rust の `Drop` トレイトおよび `DisposableStore` で実現。
- **Context Key Engine:**
  - VS Code の `when` 句（例: `editorTextFocus && !editorReadonly`）を評価する AST 式パーサーとコンテキスト評価エンジン。
- **Configuration Service:**
  - `settings.json` の階層的マージ（Default -> User Settings -> Workspace Settings）とスキーマバリデーション。

### 2.2 Monaco Editor Core (エディターコア)
- **TextModel & Buffer:**
  - `Rope` データ構造による O(log N) 編集とイミュータブルスナップショット。
- **Decorations & ViewLineTokens:**
  - Git の追加/変更/削除ガターマーカー、LSP の診断波線（エラー・警告）、検索ハイライトの統合デコレーション管理。
- **Minimap (ミニマップ):**
  - バッファ全体のテキストグリフを 1/10 スケールでピクセル描画し、現在のビューポート位置をハイライト。

### 2.3 Workbench (UI シェル)
- **QuickPick & Command Palette (`Ctrl+P` / `Ctrl+Shift+P`):**
  - 入力ファジーマッチング、コマンド一覧、ファイル一覧、シンボル一覧の高速表示。
- **Grid Layout System:**
  - エディターペインの縦横自由な分割（Split Horizontal / Vertical）とリサイズ。
- **Integrated Terminal:**
  - PTY 経由でのシェル対話、複数ターミナルインスタンスの管理。

### 2.4 Extension Host (拡張機能システム)
- **プロセス分離アーキテクチャ:**
  - UI スレッドを絶対にブロックしないよう、拡張機能は別スレッド/別プロセスで実行。
- **VS Code API (`vscode.*`) 互換:**
  - `vscode.commands.registerCommand`
  - `vscode.window.showInformationMessage`
  - `vscode.workspace.onDidChangeTextDocument`
  - `vscode.languages.registerCompletionItemProvider`

---

## 3. 性能・メモリ目標値 (VS Code との比較)

| 指標 | Microsoft VS Code | Oxide (Rust Port) | 改善率 |
| :--- | :---: | :---: | :---: |
| **起動時間 (Cold Start)** | 1.5s 〜 3.0s | **50ms 〜 100ms** | **20倍〜30倍高速** |
| **常駐メモリ消費 (Idle)** | 350MB 〜 800MB | **30MB 〜 60MB** | **1/10 以下** |
| **100MB ファイル読込** | 1.2s (フリーズ発生) | **80ms (完全ノンブロッキング)** | **15倍高速** |
| **レンダリング描画** | 60fps (DOMベース) | **120fps+ (GPU ネイティブ)** | **低遅延・高フレームレート** |
