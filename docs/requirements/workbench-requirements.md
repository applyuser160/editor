# VS Code Workbench (UI Shell) 機能要件定義書 (ISO 29148 準拠)

> **文書ステータス — 将来仕様**: 本書は設計・要件上の目標を記録するものであり、記載内容が実装済みであることを示しません。現在の実装状況と制限は [プロジェクト状況](../project-status.md) を参照してください。

> 本ドキュメントは、Microsoft VS Code の Workbench UI シェル (`src/vs/workbench/`) を Rust ネイティブ環境へ移植するための機能要件定義書です。

---

## 1. 概要とスコープ

`src/vs/workbench/` は、VS Code のデスクトップ IDE 全体のウィンドウ構造、レイアウト、パート（ActivityBar, SideBar, EditorGrid, Panel, StatusBar, TitleBar）、および統合機能（ファイルツリー、検索、Git SCM、統合ターミナル、拡張機能、QuickPick）を包括する最上位 UI レイヤーです。

---

## 2. 機能要件一覧 (Functional Requirements)

### REQ-WB-001: 2次元グリッド・マルチペインエディター (EditorGroupGrid)
- **説明:** エディターを上下・左右に無制限に分割し、タブグループとして独立管理する。
- **詳細要件:**
  - 2次元グリッドレイアウト（`GridWidget`）によるリサイズ可能なスプリットビュー。
  - 各エディターグループでの複数タブ管理、タブのドラッグ&ドロップ移動・分割。
  - Diff エディター（左右 2 画面の Side-by-Side 差分比較ビュー）。

### REQ-WB-002: アクティビティバー & サイドバー (ActivityBar & SideBar)
- **説明:** ビュー切り替えアイコン列と、選択されたビューの専用コンテナ。
- **詳細要件:**
  - 📁 **Explorer:** ファイルツリー探索、仮想スクロール、インラインファイル作成・名前変更。
  - 🔍 **Search:** ripgrep 連携による高速全文検索、正規表現、除外パス、一括置換。
  - 🌿 **Source Control (SCM):** Git 変更ファイル一覧、行単位/ファイル単位のステージング・アンステージ、コミット、ブランチ切り替え。
  - 🧩 **Extensions:** マーケットプレイス検索、インストール済み拡張一覧、有効化/無効化。

### REQ-WB-003: 下部/右側パネル (Panel & Integrated Terminal)
- **説明:** ターミナル、出力ログ、問題一覧（Problems）、デバッグコンソールを収容する折りたたみパネル。
- **詳細要件:**
  - PTY（ConPTY/openpty）と連携したフル機能の仮想端末エミュレータ。
  - 複数ターミナルインスタンスのタブ管理および左右スプリット。
  - パネルの表示位置（Bottom / Right）切り替えとドラッグリサイズ。

### REQ-WB-004: クイックアクセス & コマンドパレット (QuickOpen & Command Palette)
- **説明:** キーボード操作を中心とする VS Code の最重要ナビゲーション。
- **詳細要件:**
  - `Ctrl+P` (QuickOpen): プロジェクト内全ファイルのサブシーケンス・ファジー検索と瞬間オープン。
  - `Ctrl+Shift+P` (Command Palette): 登録された全コマンドの検索と実行。
  - `Ctrl+G` (Go to Line), `Ctrl+Shift+O` (Go to Symbol in File).

### REQ-WB-005: ステータスバー & タイトルバー (StatusBar & TitleBar)
- **説明:** ウィンドウ最上部・最下部の情報表示とグローバルコントロール。
- **詳細要件:**
  - **TitleBar:** カスタムフレームレスウィンドウ、メニューバー、QuickOpen トリガー。
  - **StatusBar:** Git ブランチ、LSP エラー/警告数、行・桁番号、インデント設定、エンコーディング、言語モード。
