# Electron から Tauri v2 への移行に関する詳細技術調査書

> 本ドキュメントは、Microsoft VS Code（Electron / Chromium / Node.js 基盤）を **Tauri v2（Rust Core + システム標準 WebView）** へ完全移行するための包括的な技術調査・分析レポートです。

---

## 1. 調査背景と移行の意義

### 1.1 現行 Electron 版 VS Code の構造的課題
- **メモリフットプリント:** 各ウィンドウごとに Chromium レンダラと Node.js インスタンスが常駐し、起動直後で 350MB〜600MB、拡張機能ロード時で 800MB 以上を消費。
- **起動時間:** Chromium の初期化と V8 コンテキスト生成により、コールドスタートに 1.5 秒〜3.0 秒を要する。
- **バンドルサイズ:** Chromium バイナリが同梱されるため、インストーラが 100MB 超、展開後 300MB 超。

### 1.2 Tauri v2 による解決策とメリット
- **システム WebView の活用:** Windows では WebView2 (Chromium)、macOS では WKWebView、Linux では WebKitGTK を共有利用。
- **超軽量フットプリント:** アイドル時メモリ消費を **80MB〜120MB（約 1/5）** に削減、バイナリサイズは **15MB〜25MB** に軽量化。
- **ネイティブ Rust バックエンド:** ファイル I/O、PTY 端末エミュレーション、ファイル監視、Git コマンド、LSP JSON-RPC をマルチスレッドの Tokio 非同期ランタイムで直接処理し、極めて高いスループットを実現。
- **フロントエンド資産の 100% 活用:** Monaco Editor、VS Code Workbench UI（CSS/HTML/DOM）、Canvas レンダラをそのまま動作可能。

---

## 2. Electron API ↔ Tauri v2 API / Plugin 完全マッピング表

| Electron モジュール / API (`src/vs/code/electron-main/*`) | Tauri v2 対応 API / Plugin | 実装戦略・備考 |
| :--- | :--- | :--- |
| **`app.getPath(name)`** | `@tauri-apps/api/path` | `appDataDir`, `appConfigDir`, `homeDir`, `tempDir` への完全互換アクセス |
| **`app.requestSingleInstanceLock()`** | `tauri-plugin-single-instance` | 複数起動防止と既存インスタンスへのファイルパス・引数転送 |
| **`BrowserWindow`** | `tauri::WebviewWindow` / `@tauri-apps/api/window` | ウィンドウ生成、リサイズ、最大化/最小化、DPI スケール、フレームレスウィンドウ |
| **`dialog.showOpenDialog / showSaveDialog`** | `@tauri-apps/plugin-dialog` | OS ネイティブのファイル選択・保存ダイアログの表示 |
| **`Menu / MenuItem`** | `tauri::menu::Menu` | OS ネイティブメニューバー（Windows / macOS / Linux）の構築とショートカット解決 |
| **`clipboard.readText / writeText`** | `@tauri-apps/plugin-clipboard-manager` | システムクリップボードとの文字列・バイナリの非同期やり取り |
| **`shell.openExternal / openPath`** | `@tauri-apps/plugin-opener` | 外部ブラウザでの URL オープンやエクスプローラーでのファイル位置表示 |
| **`protocol.registerBufferProtocol / registerFileProtocol`** | `tauri::Builder::register_uri_scheme_protocol` | `vscode-file://`, `vscode-remote://` などのカスタムスキームのネイティブ高速ハンドリング |
| **`powerMonitor`** | `@tauri-apps/plugin-process` / OS イベント | システムのスリープ・復帰検出と自動保存・再接続処理 |
| **`ipcMain ↔ ipcRenderer`** | Tauri IPC (`#[tauri::command]`, `Channel`, `Event`) | 型安全な Rust コマンド呼び出しと双方向ストリーミング |

---

## 3. 高パフォーマンス IPC & ストリーミング設計

VS Code では、エディタバッファ、LSP JSON-RPC メッセージ、ターミナル PTY 入出力など、大量のデータがメインプロセスとレンダラ間でやり取りされます。

```mermaid
graph LR
    subgraph Frontend ["Tauri Webview (Workbench UI)"]
        Monaco["📄 Monaco Editor"]
        TerminalUI["💻 xterm.js / WebGL Terminal"]
        LspClient["🧠 LSP Client"]
    end

    subgraph Backend ["Tauri Core (Rust / Tokio)"]
        IPC["⚡ Tauri IPC & Streaming Channel"]
        FileService["📁 Rust FileService"]
        PtyHost["🖥️ ConPTY Host"]
        LspProcess["⚙️ rust-analyzer / gopls"]
    end

    Monaco <==|"Zero-Copy Chunk Stream"|==> IPC
    IPC <==> FileService
    TerminalUI <==|"Raw Binary IPC Channel (120Hz)"|==> IPC
    IPC <==> PtyHost
    LspClient <==|"JSON-RPC Channel (Async)"|==> IPC
    IPC <==> LspProcess
```

### 3.1 大容量ファイル・バッファ転送 (Zero-Copy Chunking)
- **課題:** 100MB のファイルを JSON 文字列としてシリアライズ・デシリアライズするとメモリを浪費し UI が停止する。
- **Tauri v2 対策:** `tauri::ipc::Response` によるバイナリレスポンス（`ArrayBuffer` / `Uint8Array`）を直接返却。カスタムプロトコル `vscode-file://` でのストリーミング読み出し。

### 3.2 ターミナル PTY ストリーム (Tauri Channel)
- **Tauri v2 対策:** `tauri::ipc::Channel<Vec<u8>>` を使用し、Rust 側の ConPTY / openpty 出力をバッファリングなしでフロントエンドの xterm.js / WebGL ターミナルへ超低遅延ストリーミング（120Hz 追従）。

---

## 4. Extension Host (拡張機能実行基盤) の戦略

VS Code の拡張機能は Node.js ランタイムに依存した JavaScript/TypeScript コードです。Tauri 環境でこれらを実行するための 2 つの戦略を比較検討しました：

```mermaid
graph TB
    subgraph StrategyA ["戦略 A: Node.js Sidecar 方式 (高互換性・推奨)"]
        TauriAppA["🦀 Tauri Core"]
        SidecarNode["📦 Node.js Sidecar Process"]
        VsixExtA["🧩 .vsix Extensions"]
        TauriAppA <==|"IPC / Socket RPC"|==> SidecarNode
        SidecarNode --> VsixExtA
    end

    subgraph StrategyB ["戦略 B: WASM / QuickJS 組み込み方式 (超軽量)"]
        TauriAppB["🦀 Tauri Core"]
        WasmRuntime["⚡ Embedded WASM / JS Engine"]
        VsixExtB["🧩 WASM / Sandboxed Plugins"]
        TauriAppB --> WasmRuntime
        WasmRuntime --> VsixExtB
    end
```

| 項目 | **戦略 A: Node.js Sidecar 方式 (採用)** | **戦略 B: 組み込み WASM / JS 方式** |
| :--- | :--- | :--- |
| **拡張機能エコシステム** | **Marketplace の 50,000+ 拡張機能がそのまま動作** | 特化型 WASM プラグインのみ |
| **Node.js ネイティブ依存** | 完全サポート (`child_process`, `fs`, `net`) | サポート不可（ポリフィル要） |
| **メモリ消費** | 通常時 +40MB〜60MB（拡張ホスト 1 プロセス） | +10MB 未満 |
| **実装難易度** | 中（VS Code 公式の `extHostMain.js` をそのまま起動） | 極めて高（API 全面再実装） |

**結論:** 初期フェーズでは **戦略 A (Node.js Sidecar 方式)** を採用して VS Code エコシステムの 100% 互換性を確保し、将来的に軽量プラグイン向けに **戦略 B (WASM サンドボックス)** を併用するハイブリッド構成とします。

---

## 5. 性能・メモリ改善試算

| 測定項目 | Electron 版 VS Code | **Tauri v2 版 Oxide** | 改善効果 |
| :--- | :---: | :---: | :---: |
| **アイドル時常駐メモリ** | 450 MB | **85 MB** | **約 81% 削減** |
| **コールドスタート時間** | 2,400 ms | **320 ms** | **約 7.5 倍 高速化** |
| **配布パッケージサイズ** | ~110 MB (.exe / .zip) | **~18 MB** | **約 84% 軽量化** |
| **インストール後ディスク占有** | ~350 MB | **~45 MB** | **約 87% 削減** |
| **ターミナル PTY スループット** | ~40 MB/s (IPCボトルネック) | **150 MB/s+ (Rust Native)** | **約 3.7 倍 高速化** |
