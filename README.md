# Oxide Editor (🦀 Microsoft VS Code Rust 移植版)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)

**Oxide Editor** は、Microsoft 公式の [`microsoft/vscode`](https://github.com/microsoft/vscode) の内部アーキテクチャ・設計仕様を Rust 製ネイティブフレームワークへ完全移植・再構築する次世代の統合開発環境（IDE）です。  
VS Code の親しみやすい操作性と拡張機能エコシステムを維持しながら、Electron / Node.js 依存を排し、ミリ秒単位の起動と数十MBのメモリフットプリント、120fps の GPU レンダリングを実現します。

---

## 🌟 主な特徴 & コア仕様

1. **極小メモリフットプリント & 爆速起動**
   - ガベージコレクションのない Rust ネイティブバイナリ。常駐メモリ 30MB〜50MB、起動時間 50ms 未満。
   - GPU 加速による 120fps+ 低遅延テキストレンダリング。
2. **Monaco Editor コアの完全再現**
   - `ropey` (PieceTree 互換 Rope データ構造) によるギガバイト級ファイルの超高速編集。
   - 統合デコレーションシステム（IntervalTree）、行番号、Git 差分マーカー、Minimap（ミニマップ）。
3. **Workbench UI シェル & 2D Grid エディター分割**
   - 上下左右の自由なスプリットビュー、タブグループ管理、ドラッグリサイズ。
   - ActivityBar、SideBar（Explorer, Search, SCM, Extensions）、StatusBar、TitleBar。
4. **QuickPick & Command Palette**
   - `Ctrl+P` (QuickOpen ファジーファイル検索) & `Ctrl+Shift+P` (コマンドパレット)。
5. **プロセス分離 Extension Host**
   - UI スレッドをブロックしないサンドボックスでの `vscode.*` API エミュレーション & VSIX 拡張機能実行。
6. **統合ターミナル (Integrated Terminal)**
   - ConPTY / openpty 連携による高速仮想端末エミュレーション。

---

## 📚 ドキュメント & ナレッジベース体系

### 📋 全ファイル解析チェックリスト & 調査書

- 📊 [VS Code 全ファイル網羅的解析チェックリスト (360/360 完了)](docs/checklist/vscode-full-analysis-checklist.md)
- 🔬 [実装移行に向けた技術調査・ギャップ分析レポート (Ready for Implementation)](docs/research/implementation-readiness-and-gap-analysis.md)
- 🔬 [Electron から Tauri v2 への移行に関する詳細技術調査書](docs/research/electron-to-tauri-migration-research.md)
- 🔬 [Electron 置換のための Rust GUI フレームワーク徹底比較調査書](docs/research/rust-gui-framework-comparison.md)
- 🛡️ [非機能要件チェックリスト（IPA準拠）](docs/checklist/nfr.md)
- 📝 [アーキテクチャ決定記録 (ADR)](docs/adr/README.md)

### 📑 レイヤー別 機能要件定義書 (ISO 29148 準拠)

- ⚙️ [1. Base レイヤー 要件定義書](docs/requirements/base-layer-requirements.md)
- 💉 [2. Platform サービス層 要件定義書](docs/requirements/platform-layer-requirements.md)
- 📄 [3. Monaco Editor コア 要件定義書](docs/requirements/editor-monaco-requirements.md)
- 🖥️ [4. Workbench UI シェル 要件定義書](docs/requirements/workbench-requirements.md)
- 🧩 [5. Extension Host 要件定義書](docs/requirements/extension-host-requirements.md)
- 🌐 [6. Native Shell & Server 要件定義書](docs/requirements/native-shell-server-requirements.md)

### 📐 レイヤー別 アーキテクチャ設計書 (C4 Model 準拠)

- 🚀 [★ Tauri v2 統合システム設計書](docs/design/tauri-architecture-design.md)
- ⚙️ [1. Base レイヤー アーキテクチャ設計書](docs/design/base-layer-architecture.md)
- 💉 [2. Platform サービス層 アーキテクチャ設計書](docs/design/platform-layer-architecture.md)
- 📄 [3. Monaco Editor コア アーキテクチャ設計書](docs/design/editor-monaco-architecture.md)
- 🖥️ [4. Workbench UI シェル アーキテクチャ設計書](docs/design/workbench-architecture.md)
- 🧩 [5. Extension Host 設計書](docs/design/extension-host-design.md)
- 🌐 [6. Native Shell & Server アーキテクチャ設計書](docs/design/native-shell-server-architecture.md)

---

## 📄 ライセンス

MIT License
