# VS Code on Tauri v2 統合システム設計書 (C4 Model 準拠)

> **文書ステータス — 将来仕様**: 本書は設計・要件上の目標を記録するものであり、記載内容が実装済みであることを示しません。現在の実装状況と制限は [プロジェクト状況](../project-status.md) を参照してください。

> 本ドキュメントは、Microsoft VS Code のフロントエンド（Workbench UI / Monaco Editor）と Rust バックエンド（Tauri v2 / Tokio）を統合するアーキテクチャ設計書です。

---

## 1. C4 Container 構成図 (C4 Container Diagram)

```mermaid
graph TB
    User["👤 開発者 (Developer)"]

    subgraph DesktopApp ["Oxide IDE (Tauri v2 Desktop Application)"]
        subgraph WebviewContainer ["Webview Frontend (OS System Webview)"]
            WorkbenchUI["🎨 VS Code Workbench UI (HTML / CSS / TS)"]
            MonacoCore["📄 Monaco Editor (TextModel / PieceTree)"]
            XtermUI["💻 xterm.js / WebGL Terminal"]
            TauriBridge["🔌 Tauri IPC Bridge (@tauri-apps/api)"]
        end

        subgraph RustCoreContainer ["Tauri Core Backend (Rust / Tokio)"]
            IpcCommands["⚡ Tauri Command Registry & IPC Handler"]
            FileService["📁 Native FileService (tokio::fs + notify)"]
            PtyManager["🖥️ Native PTY Manager (ConPTY / openpty)"]
            GitService["🌿 Native Git Engine (git2 / CLI)"]
            LspHost["🧠 Native LSP Client (JSON-RPC 2.0)"]
            ConfigStore["⚙️ Configuration & Keybinding Store"]
        end

        subgraph SidecarContainer ["Extension Host Sidecar"]
            NodeHost["📦 Node.js Sidecar (extHostMain.js)"]
            VsixPlugins["🧩 VS Code Extensions (.vsix)"]
        end
    end

    User -->|"操作 (キーボード / マウス)"| WorkbenchUI
    WorkbenchUI --> MonacoCore
    WorkbenchUI --> XtermUI
    WorkbenchUI --> TauriBridge

    TauriBridge <==|"Tauri IPC (Commands / Events / Channel)"|==> IpcCommands
    IpcCommands --> FileService
    IpcCommands --> PtyManager
    IpcCommands --> GitService
    IpcCommands --> LspHost
    IpcCommands --> ConfigStore

    IpcCommands <==|"Local Socket RPC (Named Pipe / TCP)"|==> NodeHost
    NodeHost --> VsixPlugins
```

---

## 2. Tauri IPC コマンド & イベント仕様

### 2.1 ファイルサービス (`file_service`)
- `#[tauri::command] async fn read_file(path: PathBuf) -> Result<Vec<u8>, String>`
- `#[tauri::command] async fn write_file(path: PathBuf, contents: Vec<u8>) -> Result<(), String>`
- `#[tauri::command] async fn list_directory(path: PathBuf) -> Result<Vec<FileEntry>, String>`
- `#[tauri::command] async fn watch_directory(path: PathBuf, on_change: tauri::ipc::Channel<FsEvent>) -> Result<u32, String>`

### 2.2 ターミナル PTY サービス (`pty_service`)
- `#[tauri::command] async fn spawn_pty(cols: u16, rows: u16, shell: Option<String>, on_data: tauri::ipc::Channel<Vec<u8>>) -> Result<u32, String>`
- `#[tauri::command] async fn write_pty(id: u32, data: Vec<u8>) -> Result<(), String>`
- `#[tauri::command] async fn resize_pty(id: u32, cols: u16, rows: u16) -> Result<(), String>`
- `#[tauri::command] async fn kill_pty(id: u32) -> Result<(), String>`

### 2.3 言語サーバー LSP (`lsp_service`)
- `#[tauri::command] async fn start_lsp_server(language_id: String, server_path: String, on_message: tauri::ipc::Channel<String>) -> Result<u32, String>`
- `#[tauri::command] async fn send_lsp_message(id: u32, message: String) -> Result<(), String>`

---

## 3. ディレクトリ構成 (`crates/` & `src/`)

```
oxide-editor/
├── src-tauri/                 # Tauri v2 バックエンド (Rust)
│   ├── Cargo.toml             # Tauri, Tokio, Notify, Portable-Pty 依存関係
│   ├── tauri.conf.json        # ウィンドウ, 権限, プラグイン設定
│   ├── src/
│   │   ├── main.rs            # エントリポイント & プラグイン登録
│   │   ├── commands/          # Tauri IPC コマンド群 (file, pty, git, lsp)
│   │   ├── services/          # コアビジネスロジック (FileService, PtyManager)
│   │   └── sidecar/           # Node.js Extension Host 起動・監視
├── src/                       # フロントエンド (VS Code Workbench & Monaco)
│   ├── package.json           # @tauri-apps/api, monaco-editor, xterm.js
│   ├── index.html             # メインエントリポイント
│   └── vs/                    # VS Code Workbench Webview レイヤ
└── docs/                      # ナレッジベース & 設計書群
```
