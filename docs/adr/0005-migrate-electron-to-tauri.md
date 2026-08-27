# ADR-0005: Tauri v2 の採用

- **ステータス:** Accepted
- **決定者:** Syun
- **初版作成日:** 2026-08-22
- **状態確認日:** 2026-08-27
- **関連文書:** [プロジェクト状況](../project-status.md)

## コンテキスト

Oxide Editor は Rust バックエンドと OS WebView を用いるデスクトップアプリケーションとして Tauri v2 を採用しています。フロントエンドは Monaco Editor を利用します。この ADR はフレームワーク採用の記録であり、VS Code 機能・拡張 API の完全な移植または性能目標の達成を示すものではありません。

## 検討した選択肢

1. Electron を維持し、Native Addon を追加する
2. ピュア Rust GUI で再実装する
3. Tauri v2 を利用する

## 決定

選択肢 3 の **Tauri v2** を採用します。現行の `src-tauri` クレートは Tauri を、フロントエンドは TypeScript と Monaco を利用しています。Tauri への移行完了ではなく、Tauri を基盤として開発する判断です。

## 検証状況

| 項目 | 状態 |
|---|---|
| Tauri バックエンドの採用 | 実装済み |
| Monaco を利用したフロントエンド | 実装済み |
| 常駐メモリ、バイナリサイズ、起動時間 | 未検証 |
| Node.js Sidecar による VS Code 拡張互換 | 未実装 |
| VS Code Marketplace / `vscode.*` API 互換 | 未実装 |

性能値、削減率、API 互換率は、対応する再現可能な測定・E2E テストが公開されるまで主張しません。
