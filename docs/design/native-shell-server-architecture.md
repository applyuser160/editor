# VS Code Native Shell & Server レイヤー アーキテクチャ設計書 (C4 Model & Rust 実装)

> **文書ステータス — 将来仕様**: 本書は設計・要件上の目標を記録するものであり、記載内容が実装済みであることを示しません。現在の実装状況と制限は [プロジェクト状況](../project-status.md) を参照してください。

> 本ドキュメントは、VS Code のネイティブシェル (`src/vs/code/`)、リモートサーバー (`src/vs/server/`) を Rust で構築するためのアーキテクチャ設計書です。

---

## 1. コンポーネント構成図 (C4 Component Diagram)

```mermaid
graph TB
    subgraph LocalMachine ["Local Client Machine"]
        MainProcess["🖥️ Native Main Process (Single Instance)"]
        UIWindow["🎨 Native GPU Window (Workbench UI)"]
        LocalFileService["📁 Local FileService"]
    end

    subgraph RemoteEnvironment ["Remote Server / WSL / Container / SSH"]
        RemoteServer["🌐 Oxide Headless Server (Tokio)"]
        RemoteExtHost["🧩 Remote Extension Host"]
        RemotePty["💻 Remote PTY (ConPTY / OpenPTY)"]
        RemoteLsp["🧠 Language Servers (rust-analyzer, etc.)"]
    end

    MainProcess --> UIWindow
    UIWindow <==|"Secure Multiplexed Stream (WebSocket / SSH)"|==> RemoteServer
    RemoteServer --> RemoteExtHost
    RemoteServer --> RemotePty
    RemoteServer --> RemoteLsp
```

---

## 2. コアモジュール設計

### 2.1 Single Instance & CLI (`oxide_shell::app`)
- **単一インスタンス制御:**
  - OS の Named Pipe (Windows: `\\.\pipe\oxide-ipc.sock`) / Unix Domain Socket を使用し、起動時に既存インスタンスの有無を検証。
  - 既にインスタンスが存在する場合、起動引数（開くファイルパスや行番号）を既存プロセスへ転送して即座に終了。

### 2.2 Remote Tunnel & Multiplexing (`oxide_server::tunnel`)
- 単一の TCP / WebSocket 接続上で、複数の独立したストリーム（ファイル転送、PTY 入出力、Extension Host RPC、LSP メッセージ）を高スループットに多重化（Multiplexing）。
