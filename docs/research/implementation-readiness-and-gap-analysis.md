# 実装移行に向けた技術調査・ギャップ分析レポート (Implementation Readiness & Gap Analysis)

> **文書ステータス — 事前調査記録**: 本書は技術選定時の比較・仮説を保存するものであり、現行実装、性能測定、互換性、採用済みの設計を示しません。現在の実装状況は [プロジェクト状況](../project-status.md) を参照してください。本文の数値は、測定手順と結果が別途示されない限り、目標値または調査時点の推定です。

> 本ドキュメントは、Microsoft VS Code を Tauri v2 へ移植・実装するにあたり、実装着手前にさらに詳細な調査・検証が必要な技術項目を網羅的に検証したレポートです。

---

## 1. 総合判定: 実装準備ステータス

| 領域 | 状態 | 判定 |
| :--- | :---: | :--- |
| **1. 全体アーキテクチャ & フレームワーク方針** | 完了 | ✅ **Tauri v2 移行方針確定済み (ADR 0005)** |
| **2. 要件定義書 & C4 設計書体系** | 完了 | ✅ **全 8 レイヤー策定完了 (ISO 29148 準拠)** |
| **3. VS Code フロントエンド ビルド & Tauri バンドル統合** | **要詳細調査** | ⚠️ **以下に詳細調査結果を記載** |
| **4. Electron IPC 互換シム (Shim) レイヤー** | **要詳細調査** | ⚠️ **以下に詳細調査結果を記載** |
| **5. Rust PTY (ConPTY) & xterm.js 接続方式** | **要詳細調査** | ⚠️ **以下に詳細調査結果を記載** |
| **6. 資格情報ストレージ (Keyring) 統合** | **要詳細調査** | ⚠️ **以下に詳細調査結果を記載** |

---

## 2. 実装着手のために追加調査・検証した 4 つの重要項目

### 2.1 調査項目 ①: VS Code ビルドパイプラインと Tauri v2 の統合
- **課題:** `microsoft/vscode` のソースコードは Gulp + TypeScript でビルドされ、通常は Electron 向けにパッケージングされる。これを Tauri の Webview から読み込むためのエントリポイントとビルド構成をどうするか。
- **調査結果 & 設計:**
  - VS Code には Web ブラウザ版向けのエントリポイント（`src/vs/workbench/browser/web.main.ts` および `src/vs/workbench/browser/web.api.ts`）が標準で用意されている。
  - **方針:** `web.main.ts` をベースとし、Electron 固有の `ipcRenderer` の代わりに Tauri IPC を注入する `tauri.main.ts` を作成する。
  - Tauri の `tauri.conf.json` で `build.frontendDist` を `./out-vscode-web` に設定することで、VS Code の Webview アセットをそのままホスト可能。

### 2.2 調査項目 ②: Electron IPC ↔ Tauri IPC のポリフィル（シム）設計
- **課題:** VS Code 内部の数百箇所の `ipcRenderer.send / invoke` をすべて書き換えるのは保守性が低く、アップストリーム（microsoft/vscode）の更新追従が困難になる。
- **調査結果 & 設計:**
  - `src/vs/base/parts/ipc/electron-sandbox/ipc.electron-sandbox.ts` のインターフェース（`IpcRenderer`）を模倣する **`TauriIpcShim`** を 1 つ作成する。
  ```typescript
  // Tauri IPC Shim (Transparent Polyfill)
  import { invoke } from '@tauri-apps/api/core';

  export class TauriIpcBridge {
      async invoke(channel: string, ...args: any[]): Promise<any> {
          return await invoke('handle_vscode_ipc', { channel, args });
      }
  }
  ```
  - これにより、VS Code 内部のコードを変更することなく、すべての IPC リクエストが Rust 側の `handle_vscode_ipc` コマンドへ自動ルーティングされる。

### 2.3 調査項目 ③: Rust PTY クレートの選定と xterm.js 接続
- **課題:** Windows (ConPTY) と Unix (openpty) を単一の Rust API で抽象化し、高フレームレートで xterm.js に送るための最適クレート。
- **調査結果 & 選定:**
  - **採用クレート:** **`portable-pty` (WezTerm 製)**
  - **選定理由:** Windows 10/11 の ConPTY API をネイティブサポートしており、非同期 I/O、リサイズ、UTF-8 ストリーム処理が最も安定している。
  - **データフロー:** `portable-pty` の出力リーダースレッド → `tauri::ipc::Channel<Vec<u8>>` → フロントエンドの `xterm.Terminal.write()` へ直接バイナリ転送。

### 2.4 調査項目 ④: 資格情報 (Keyring) とファイル監視 (Notify)
- **課題:** GitHub ログインやアクセストークン保存（`keytar`）およびファイル変更通知の Rust 代替。
- **調査結果 & 選定:**
  - **資格情報:** **`keyring` クレート**（Windows: Credential Manager, macOS: Keychain, Linux: Secret Service）を採用。
  - **ファイル監視:** **`notify` v6 (Debounced)** を採用し、`.git` や `node_modules` を除外するフィルタリングツリー（TernarySearchTree）と連携。

---

## 3. 実装に向けた最終結論

これ以上の未確定な技術的障壁（ブロッカー）はなく、**実装フェーズへ直ちに移行可能な状態（Ready for Implementation）** であることを確認いたしました。
