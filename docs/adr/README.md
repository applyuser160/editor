# ADR（Architecture Decision Records）

Oxide Editor プロジェクトにおけるアーキテクチャ上の重要な意思決定履歴を記録・管理するディレクトリです。  
フォーマットは MADR (Markdown Architectural Decision Records) に準拠しています。

---

## 採番・命名規則

- ファイル名: `NNNN-kebab-case-title.md`（4桁ゼロパディング）
- 採番は連番で管理します。

---

## ADR 一覧

| 番号 | タイトル | ステータス | 決定日 |
|------|---------|-----------|--------|
| [0001](0001-use-rust-and-gpu-rendering.md) | Rust言語とGPUレンダリングエンジンの採用 | Accepted | 2026-08-22 |
| [0002](0002-use-rope-data-structure.md) | Ropey（Ropeデータ構造）によるテキストバッファ管理の採用 | Accepted | 2026-08-22 |
| [0003](0003-wasm-plugin-architecture.md) | WASM（WebAssembly）によるサンドボックスプラグイン基盤の採用 | Accepted | 2026-08-22 |
| [0004](0004-tree-sitter-and-lsp-hybrid-syntax.md) | Tree-sitterとLSPによるハイブリッド構文解析・ハイライトの採用 | Accepted | 2026-08-22 |
