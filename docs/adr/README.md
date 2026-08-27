# Architecture Decision Records

Oxide Editor のアーキテクチャ上の判断を記録するディレクトリです。ADR の内容はソースコードより優先されません。実装状況は [プロジェクト状況](../project-status.md) を、機能の受け入れ条件は関連 Issue を参照してください。

## ステータスの定義

| ステータス | 意味 |
|---|---|
| Accepted | 現行実装で採用済みの判断。実装全体が完了・検証済みであることを意味しない。 |
| Proposed | 検討・合意待ちの判断。実装済みではない。 |
| Deferred | 将来候補として保留している判断。対応する依存関係・実装は存在しない。 |
| Superseded | 別の ADR または実装方針に置き換えられた判断。 |

## ADR 一覧

| 番号 | タイトル | ステータス | 実装状況 |
|---|---|---|---|
| [0001](0001-use-rust-and-gpu-rendering.md) | Rust とネイティブ GPU レンダリング | Deferred | Rust は利用するが、WGPU/Vello レンダラーは未採用。 |
| [0002](0002-use-rope-data-structure.md) | Ropey テキストバッファ | Deferred | Monaco のテキストモデルを利用し、Ropey 依存はない。 |
| [0003](0003-wasm-plugin-architecture.md) | WASM プラグイン基盤 | Deferred | VSIX の管理のみ実装。WASM 実行・権限モデルは未実装。 |
| [0004](0004-tree-sitter-and-lsp-hybrid-syntax.md) | Tree-sitter と LSP の構文解析 | Deferred | LSP 通信基盤はあるが、Tree-sitter は未採用。 |
| [0005](0005-migrate-electron-to-tauri.md) | Tauri v2 の採用 | Accepted | Tauri バックエンドと Monaco フロントエンドを利用する。性能値・API 互換は未検証。 |
| [0006](0006-remote-development-architecture.md) | リモート開発アーキテクチャ | Proposed | SSH ワークスペース等の実装は未着手。 |

新しい ADR では、選択肢、判断理由、ステータス、関連 Issue、現行実装への影響を明記します。実装がない ADR を `Accepted` に変更する場合は、対応する根拠と検証を同じ PR に含めます。
