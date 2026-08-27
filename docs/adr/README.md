# ADR（Architecture Decision Records）

Oxide Editor プロジェクトにおけるアーキテクチャ上の重要な意思決定履歴を記録・管理するディレクトリです。  
フォーマットは MADR (Markdown Architectural Decision Records) に準拠しています。

---

## 採番・命名規則

- ファイル名: `NNNN-kebab-case-title.md`（4桁ゼロパディング）
- 採番は連番で管理します。

---

## ADR 一覧

> ADRの状態は**アーキテクチャ上の意思決定**を表し、機能の実装・検証完了を意味しません。現在提供する機能は[実装状況とロードマップ](../implementation-status.md)を参照してください。

| 状態 | 意味 |
|---|---|
| Accepted | 現行アーキテクチャで採用した意思決定。実装範囲は各ADRと実装状況を参照する。 |
| Deferred | 将来案として保留した意思決定。現行実装には含まれない。 |

| 番号 | タイトル | ステータス | 決定日 |
|------|---------|-----------|--------|
| [0001](0001-use-rust-and-gpu-rendering.md) | Rust言語とGPUレンダリングエンジンの採用 | Deferred | 2026-08-22 |
| [0002](0002-use-rope-data-structure.md) | Ropey（Ropeデータ構造）によるテキストバッファ管理の採用 | Deferred | 2026-08-22 |
| [0003](0003-wasm-plugin-architecture.md) | WASM（WebAssembly）によるサンドボックスプラグイン基盤の採用 | Deferred | 2026-08-22 |
| [0004](0004-tree-sitter-and-lsp-hybrid-syntax.md) | Tree-sitterとLSPによるハイブリッド構文解析・ハイライトの採用 | Deferred | 2026-08-22 |
| [0005](0005-migrate-electron-to-tauri.md) | Tauri v2への段階的移行 | Accepted（部分実装） | 2026-08-22 |
