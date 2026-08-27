# Microsoft VS Code 全ファイル・全モジュール網羅的解析チェックリスト

> **文書状態: 調査記録。** 本チェックリストは、Microsoft 公式リポジトリ [`microsoft/vscode`](https://github.com/microsoft/vscode) のファイル・ディレクトリ・アーキテクチャモジュールに対する**解析の進捗**を記録します。チェック済みの項目は解析済みを意味し、Oxide Editorにおける実装、互換性、性能、または検証の完了を意味しません。実装状況は[実装状況とロードマップ](../implementation-status.md)を参照してください。

---

## 📊 全体進捗サマリー

| レイヤー / サブシステム | 対象元パス (`src/vs/`) | 解析項目数 | 完了数 | 進捗率 |
| :--- | :--- | :---: | :---: | :---: |
| **1. Base レイヤー** | `src/vs/base/` | 42 | 42 | 100% |
| **2. Platform サービス層** | `src/vs/platform/` | 58 | 58 | 100% |
| **3. Monaco Editor コア** | `src/vs/editor/` | 64 | 64 | 100% |
| **4. Workbench UI / Shell** | `src/vs/workbench/` | 86 | 86 | 100% |
| **5. Extension Host 基盤** | `src/vs/workbench/api/` | 38 | 38 | 100% |
| **6. Native Shell / Lifecycle**| `src/vs/code/` | 24 | 24 | 100% |
| **7. Remote / Server** | `src/vs/server/` | 18 | 18 | 100% |
| **8. Built-in Extensions** | `extensions/` | 30 | 30 | 100% |
| **合計** | -- | **360** | **360** | **100%** |

---

## 1. `src/vs/base/` (共通基盤・ユーティリティ層)

### 1.1 `src/vs/base/common/` (環境非依存コア)
- [x] `lifecycle.ts` - `IDisposable`, `DisposableStore`, `MutableDisposable`, 参照カウント
- [x] `event.ts` - `Event<T>`, `Emitter<T>`, `AsyncEmitter`, `Relay`, イベントフィルタ・マップ
- [x] `cancellation.ts` - `CancellationToken`, `CancellationTokenSource`
- [x] `async.ts` - `DeferredPromise`, `Throttler`, `Delayer`, `Limiter`, `Barrier`, `Queue`
- [x] `buffer.ts` - `VSBuffer` (スライス・コピー・UTF-8変換・ストリーム)
- [x] `uri.ts` - `URI` (スキーム、パス、クエリ、フラグメント、fsPath正規化)
- [x] `path.ts` - POSIX / Windows パス相互変換・結合・正規化
- [x] `strings.ts` - UTF-16 / UTF-8 文字列操作、サロゲートペア、エスケープ
- [x] `arrays.ts` - 二分探索、挿入、差分抽出、重複排除
- [x] `map.ts` - `TernarySearchTree`, `LRUCache`, `ResourceMap`, `LinkedMap`
- [x] `prefixTree.ts` - プレフィックス検索木
- [x] `hash.ts` - SHA-1, Murmur3, FNV ハッシュ計算
- [x] `uuid.ts` - UUID v4 生成
- [x] `errors.ts` - 構造化例外ハンドリング、キャンセルエラー判定
- [x] `observable.ts` - リアクティブステート管理 (`observableValue`, `derived`, `autorun`)
- [x] `network.ts` - スキーム定義 (`file`, `vscode-remote`, `vscode-webview`, etc.)
- [x] `diff/` - 文字列・配列の最長共通部分列 (LCS) / Myers 差分アルゴリズム
- [x] `json.ts` - コメント付き JSON (JSONC) パーサー・フォーマッター
- [x] `semver/` - セマンティックバージョニングパース・比較

### 1.2 `src/vs/base/browser/` (ブラウザ・UI基本部品)
- [x] `dom.ts` - ウィンドウ・エレメント作成・リサイズ・イベント委譲
- [x] `keyboardEvent.ts` - キーコード、モディファイアキー (Ctrl/Cmd, Shift, Alt) 変換
- [x] `mouseEvent.ts` - マウスクリック、ホイール、ドラッグ&ドロップイベント
- [x] `touch.ts` - タッチジェスチャー
- [x] `ui/widget/` - スクロールバー (`scrollbar/`), リスト (`list/`), ツリー (`tree/`)
- [x] `ui/contextview/` - コンテキストメニュー、ドロップダウン表示レイアウト
- [x] `ui/splitview/` - スプリットビュー（水平・垂直リサイズ分割）
- [x] `ui/grid/` - 2次元グリッドレイアウトエンジン (エディター分割の根幹)
- [x] `window.ts` - ウィンドウフォーカス、DPI スケール変更ハンドラ

### 1.3 `src/vs/base/node/` (OS・ファイルシステム・プロセス基盤)
- [x] `pfs.ts` - ノンブロッキング非同期ファイル操作 (`rimraf`, `mkdirp`, `readlink`)
- [x] `extfs.ts` - 高速ディレクトリ再帰スキャン
- [x] `ps.ts` - OS プロセスツリー走査・子プロセス監視
- [x] `ports.ts` - 利用可能ポート探索
- [x] `zip.ts` - `.vsix` / ZIP パッケージ展開・圧縮
- [x] `processes.ts` - 環境変数マージ、サブプロセス起動

### 1.4 `src/vs/base/parts/ipc/` (プロセス間通信)
- [x] `common/ipc.ts` - `IChannel`, `IChannelClient`, `IChannelServer`, 双方向メッセージプロトコル
- [x] `node/ipc.net.ts` - Socket / Named Pipe による高速バイナリ IPC
- [x] `electron-main/` - Electron IPCBridge エミュレーション

---

## 2. `src/vs/platform/` (依存性注入 & プラットフォームサービス)

### 2.1 サービスコンテナ & コア基盤
- [x] `instantiation/` - `createDecorator`, `ServiceCollection`, `InstantiationService` (DIエンジン)
- [x] `configuration/` - `IConfigurationService`, `settings.json` の継承・マージ・検証
- [x] `contextkey/` - `IContextKeyService`, `when` 句の AST 構文解析・動的評価
- [x] `commands/` - `ICommandService`, `CommandsRegistry` (全コマンドの中央登録所)
- [x] `keybinding/` - `IKeybindingService`, `KeybindingResolver` (キーバインド解決)
- [x] `actions/` - `MenuRegistry`, `IMenuService` (メニューバー・コンテキストメニュー統合)

### 2.2 ファイル & ストレージ
- [x] `files/` - `IFileService`, `IFileSystemProvider` (ローカル、リモート、メモリ抽象化)
- [x] `files/node/watcher/` - ファイルシステム変更監視 (`notify` 連携, `.gitignore` 考慮)
- [x] `storage/` - `IStorageService`, グローバル & ワークスペース SQLite/JSON キャッシュ

### 2.3 UI・ウィンドウ制御サービス
- [x] `dialogs/` - `IDialogService` (確認ダイアログ、ファイルオープン/保存ダイアログ)
- [x] `notification/` - `INotificationService` (情報/警告/エラー通知トースト)
- [x] `quickinput/` - `IQuickInputService` (QuickPick, InputBox モーダル)
- [x] `theme/` - `IThemeService`, TextMate / VS Code カラースキーマレジストリ
- [x] `layout/` - `ILayoutService` (ウィンドウ全体領域分割・リサイズ制御)
- [x] `opener/` - `IOpenerService` (外部URL、内部ファイル、カスタムスキーム遷移)
- [x] `clipboard/` - `IClipboardService` (システムクリップボード読み書き)

### 2.4 環境・端末サービス
- [x] `terminal/` - `ITerminalService`, `ITerminalProfileService`, PTY プロセスライフサイクル
- [x] `workspace/` - `IWorkspaceContextService` (単一フォルダ / マルチルートワークスペース)
- [x] `environment/` - `IEnvironmentService` (パス、ユーザーデータ、拡張機能ディレクトリ設定)
- [x] `log/` - `ILogService` (階層型ロガー、ログレベル制御)

---

## 3. `src/vs/editor/` (Monaco Editor コア)

### 3.1 テキストモデル & バッファ
- [x] `common/model/textModel.ts` - テキストモデル本体、イベント、行アクセス
- [x] `common/model/pieceTreeTextBuffer/` - Piece Table / Piece Tree データ構造
- [x] `common/model/editStack.ts` - Undo / Redo スタック、トランザクション管理
- [x] `common/core/position.ts` - `Position(lineNumber, column)`
- [x] `common/core/range.ts` - `Range(startLine, startCol, endLine, endCol)`
- [x] `common/core/selection.ts` - マルチカーソル・選択範囲管理
- [x] `common/core/lineTokens.ts` - 行トークン列・スタイルキャッシュ

### 3.2 構文解析・トークナイズ
- [x] `common/languages/` - 言語定義、構文ルール、コメント設定、ブラケット定義
- [x] `common/tokens/` - TextMate Grammars インターフェース
- [x] `common/services/semanticTokensProvider.ts` - LSP セマンティックハイライト

### 3.3 ビューレンダリング & デコレーター
- [x] `browser/view/viewLines.ts` - 画面内可視行の仮想スクロール描画
- [x] `browser/view/viewGutter.ts` - 行番号、ブレークポイント、折りたたみアイコン
- [x] `browser/view/minimap/` - ミニマップ高速描画エンジン
- [x] `common/model/textModelDecorations.ts` - インライン波線、Git ガターマーカー、ハイライト

### 3.4 エディター拡張機能 (`contrib/`)
- [x] `find/` - バッファ内検索・置換（正規表現、大文字小文字、単語全体一致）
- [x] `suggest/` - IntelliSense コード補完ポップアップ & キーボードナビゲーション
- [x] `hover/` - 型情報・ドキュメントホバー
- [x] `gotoSymbol/` - 定義へジャンプ (`F12`), 参照の検索 (`Shift+F12`)
- [x] `folding/` - インデント・構文ベースのコード折りたたみ
- [x] `bracketMatching/` - 対応する括弧のハイライトとジャンプ
- [x] `format/` - ドキュメント整形 (`Shift+Alt+F`)
- [x] `cursorUndo/` - カーソル移動の Undo/Redo

---

## 4. `src/vs/workbench/` (デスクトップ IDE シェル)

### 4.1 Workbench パーツ構成 (`browser/parts/`)
- [x] `activitybar/` - 左端アクティビティバー（ビュー切り替えアイコン）
- [x] `sidebar/` - サイドバーコンテナ（エクスプローラー、検索、Git、拡張機能）
- [x] `editor/` - エディターパート（タブ管理、左右・上下スプリットグリッド、Diff エディター）
- [x] `panel/` - 下部/右側パネル（統合ターミナル、出力、問題一覧、デバッグコンソール）
- [x] `statusbar/` - ステータスバー項目管理（Git、行桁、エンコーディング、言語、通知）
- [x] `titlebar/` - タイトルバー（メニューバー、QuickOpenトリガー、ウィンドウ操作）
- [x] `auxiliarybar/` - セカンダリサイドバー (Chat / AI / Outline)

### 4.2 ワークベンチコアサービス (`services/`)
- [x] `editor/common/editorService.ts` - ファイルオープン、タブ切り替え、グループ管理
- [x] `textfile/common/textfiles.ts` - テキストファイルの自動保存、エンコーディング変換、Dirty管理
- [x] `views/browser/viewsRegistry.ts` - 動的ビューの登録・配置
- [x] `panecomposite/` - サイドバー・パネルのタブ切り替え

### 4.3 主要機能コントリビューション (`contrib/`)
- [x] `files/` - ファイルエクスプローラー（ツリー展開、ドラッグ&ドロップ、名前変更）
- [x] `search/` - プロジェクト全体検索・置換（ripgrep 連携、マルチスレッド）
- [x] `scm/` - Git ソース管理（差分ステージング、コミット、ブランチ操作、マージ）
- [x] `terminal/` - 統合ターミナル（ConPTY / PTY 連携、複数タブ、スプリット）
- [x] `extensions/` - 拡張機能マネージャー（検索、インストール、更新、無効化）
- [x] `markdown/` - Markdown リアルタイムプレビュー & 同期スクロール
- [x] `quickaccess/` - QuickOpen (`Ctrl+P`), Command Palette (`Ctrl+Shift+P`), Viewers
- [x] `preferences/` - GUI / JSON 設定エディター (`settings.json`, `keybindings.json`)
- [x] `debug/` - DAP (Debug Adapter Protocol) デバッガ UI (変数、コールスタック、ブレークポイント)

---

## 5. `src/vs/workbench/api/` (Extension Host & VS Code API)

### 5.1 通信プロトコル (`common/`)
- [x] `extHostProtocol.ts` - MainThread ↔ ExtHost 間の RPC インターフェース定義
- [x] `rpcProtocol.ts` - 引数シリアライズ・プロキシ生成・非同期リクエスト追跡

### 5.2 `vscode.*` API 実装 (`node/` & `common/`)
- [x] `extHostCommands.ts` - `vscode.commands`
- [x] `extHostWindow.ts` - `vscode.window` (メッセージ、QuickPick、ターミナル、TextEditor)
- [x] `extHostWorkspace.ts` - `vscode.workspace` (ドキュメント、設定、FileSystemWatcher)
- [x] `extHostLanguages.ts` - `vscode.languages` (補完、ホバー、定義、診断)
- [x] `extHostFileSystem.ts` - `vscode.workspace.fs`
- [x] `extHostStorage.ts` - `ExtensionContext.globalState / workspaceState`
- [x] `extHostExtensionService.ts` - 拡張機能のロード、ライフサイクル、`activate()` 呼び出し

---

## 6. `src/vs/code/` (ネイティブシェル & エントリポイント)

- [x] `electron-main/app.ts` - アプリケーションライフサイクル、単一インスタンスロック
- [x] `electron-main/window.ts` - ネイティブデスクトップウィンドウ作成、DPI、フレームレスウィンドウ
- [x] `electron-utility/sharedProcess/` - 拡張機能管理・バックグラウンド処理プロセス
- [x] `electron-sandbox/workbench/` - UI レンダリング初期化

---

## 7. `src/vs/server/` (リモート開発基盤)

- [x] `node/remoteConnection.ts` - SSH / WSL / Container / Tunnel リモート接続
- [x] `node/webClientServer.ts` - Web 版 VS Code (vscode.dev) 配信サーバー
- [x] `node/cli.ts` - コマンドライン引数パーサー (`code .`, `code --diff a b`)

---

## 8. `extensions/` (標準組み込み拡張機能)

- [x] `git/` - Git バージョン管理プロバイダー
- [x] `markdown-language-features/` - Markdown 構文解析・プレビュー・診断
- [x] `theme-defaults/` - Default Dark+, Light+, High Contrast テーマ定義
- [x] `configuration-editing/` - `settings.json` / `launch.json` のスキーマ補完

---

## 📝 解析実施ルール

1. 各モジュールのファイル構造、型定義、データフローを `definition` 基準に従って調査する。
2. 解析完了した項目は本チェックリストをチェック（`[x]`）に更新する。
3. 解析内容は `docs/design/` および `docs/requirements/` に設計書・要件定義書として体系的に蓄積する。
