# Oxide Editor (🦀 VS Code Alternative in Rust)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)

**Oxide Editor** は、Rust で構築されたメモリ効率とパフォーマンスに特化した次世代の統合開発環境（IDE）です。  
VS Code の親しみやすい操作性と高機能性を維持しながら、Electron / Node.js 依存を排し、ミリ秒単位の起動と数十MBのメモリフットプリントを実現します。

---

## 🌟 主な特徴 & コア機能

1. **極小メモリフットプリント & 爆速パフォーマンス**
   - ガベージコレクションのない Rust ネイティブバイナリ。常駐メモリ 50MB 未満、起動時間 100ms 未満。
   - WGPU / Vello による GPU 加速テキストレンダリング (60 / 120fps)。
2. **高速テキストエディター & シンタックス解析**
   - `ropey` (Rope データ構造) によるギガバイト級ファイルの超高速編集 & 低メモリ消費。
   - Tree-sitter 増分構文解析 + Language Server Protocol (LSP) による高精度セマンティックハイライト・コード補完・エラー診断。
3. **フォルダツリー & ワークスペース管理**
   - 10,000 ファイル超の大規模プロジェクトでも仮想スクロールで遅延なく描画。
   - OS ファイル変更通知 (`notify`) とのリアルタイム同期。
4. **Git GUI 統合 (Source Control)**
   - 変更差分ビュー (Diff View)、Hunk 単位のステージング、コミット、ブランチ切り替え。
5. **統合ターミナル (Integrated Terminal)**
   - ConPTY / openpty 連携による高速な仮想端末エミュレーション (PowerShell, Bash, Zsh)。
6. **WASM サンドボックス拡張機能 (Plugin System)**
   - WebAssembly ランタイムによる安全かつ高速なプラグイン実行基盤。
7. **並列検索エンジン (Search & Replace)**
   - `ripgrep` コア統合によるプロジェクト全体検索、ファイル名ファジーファインダー (`Ctrl+P`)。
8. **Markdown リアルタイムプレビュー**
   - `pulldown-cmark` によるエディター・プレビューの即時レンダリングと同期スクロール。

---

## 🏗️ アーキテクチャ概要

```
oxide-editor/
├── Cargo.toml                 # Workspace Root
├── docs/                      # ナレッジベース & 体系的設計書
│   ├── requirements/          # 要件定義書 (SRS簡易版 / ISO 29148準拠)
│   ├── design/                # アーキテクチャ設計書 (C4モデル準拠)
│   ├── adr/                   # アーキテクチャ決定記録 (MADR準拠)
│   └── checklist/             # 非機能要件チェックリスト (IPA準拠)
└── crates/                    # モジュラー Rust クレート
    ├── editor-core/           # Ropeバッファ, カーソル, Undo/Redo
    ├── editor-syntax/         # Tree-sitter構文解析 & ハイライト
    ├── editor-lsp/            # Language Server Protocol クライアント
    ├── editor-workspace/      # ファイルツリー, タブ管理, 設定
    ├── editor-git/            # Git GUI & 差分エンジン
    ├── editor-terminal/       # PTY仮想端末ホスト
    ├── editor-search/         # ripgrep検索 & ファジーファインダー
    ├── editor-markdown/       # Markdownプレビューレンダラ
    ├── editor-plugin/         # WASMプラグインホスト
    ├── editor-ui/             # GPU描画 & ウィンドウ管理
    └── editor-app/            # アプリケーションエントリポイント
```

---

## 📚 ドキュメント & ナレッジベース

- 📋 [要件定義書（SRS簡易版）](docs/requirements/editor-requirements.md)
- 📐 [システム設計書（C4モデル準拠）](docs/design/editor-architecture.md)
- 📝 [アーキテクチャ決定記録 (ADR)](docs/adr/README.md)
- 🛡️ [非機能要件チェックリスト（IPA準拠）](docs/checklist/nfr.md)

---

## 🚀 クイックスタート (開発者向け)

### 前提条件
- Rust 1.80 以上 (`rustup update stable`)

```bash
# リポジトリのクローン
git clone https://github.com/applyuser160/editor.git
cd editor

# ビルド & テスト
cargo build
cargo test
```

---

## 📄 ライセンス

MIT License
