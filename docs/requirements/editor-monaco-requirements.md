# Monaco Editor コア 機能要件定義書 (ISO 29148 準拠)

> **文書ステータス — 将来仕様**: 本書は設計・要件上の目標を記録するものであり、記載内容が実装済みであることを示しません。現在の実装状況と制限は [プロジェクト状況](../project-status.md) を参照してください。

> 本ドキュメントは、Microsoft VS Code の Monaco Editor コア (`src/vs/editor/`) を Rust ネイティブ環境へ移植するための機能要件定義書です。

---

## 1. 概要とスコープ

`src/vs/editor/` は、VS Code の中核となる高機能テキストエディタエンジンです。大容量ファイルの編集、マルチカーソル、Undo/Redo、シンタックスハイライト、デコレーション（ガターマーカー・インライン波線）、ミニマップ、IntelliSense（コード補完・ホバー・定義ジャンプ）を管轄します。

---

## 2. 機能要件一覧 (Functional Requirements)

### REQ-MONACO-001: PieceTree / Rope テキストバッファ
- **説明:** 100MB 以上の巨大ファイルでも O(log N) での挿入・削除、行・桁とオフセットの相互変換を保証する。
- **詳細要件:**
  - `TextModel` によるテキストバッファ管理。
  - 不変スナップショット（Snapshot）の取得とマルチスレッド非同期検索・LSP 解析。
  - CR/LF、LF、UTF-8、UTF-16 エンコーディングの自動判別と正規化。

### REQ-MONACO-002: マルチカーソル & 選択範囲正規化 (Cursors & Selections)
- **説明:** 複数カーソル (`Selection[]`) の同時操作と重複統合。
- **詳細要件:**
  - カーソル追加 (`Alt+Click`, `Ctrl+Alt+Up/Down`, `Ctrl+D` 単語選択)。
  - カーソル位置が重なった場合の自動マージ（Overlap Normalization）。
  - 各カーソルに対する独立した矩形選択（Column Selection Mode）。

### REQ-MONACO-003: トランザクション & Undo/Redo スタック (EditStack)
- **説明:** 複合編集操作（一括置換、マルチカーソル同時編集）を単一のアトミックな操作として記録する。
- **詳細要件:**
  - `IIdentifiedSingleEditOperation` のグループ化（Transaction）。
  - Undo / Redo 実行時のカーソル位置・選択状態の完全復元。
  - カーソル移動のみを巻き戻す Cursor Undo (`Ctrl+U`) のサポート。

### REQ-MONACO-004: 統合デコレーションシステム (ModelDecorations)
- **説明:** 行番号横（Gutter）、行全体（Line）、文字単位（Inline）に対する視覚的装飾の統合管理。
- **詳細要件:**
  - **Gutter:** Git 差分マーカー（追加・変更・削除）、ブレークポイント、折りたたみアイコン。
  - **Line:** 現在行ハイライト、差分削除行の背景色。
  - **Inline:** LSP 診断波線（エラー=赤、警告=黄、情報=青）、検索マッチ背景色、選択範囲。
  - テキスト挿入・削除に伴うデコレーション座標の自動追従（Tracking Range）。

### REQ-MONACO-005: 60/120fps 仮想スクロール描画 (ViewLines)
- **説明:** 画面内に表示される可視行のみを GPU レンダリングし、数万行のファイルでも 120fps 以上の描画を維持する。
- **詳細要件:**
  - スクロール位置に基づく可視行範囲（`startLineNumber..endLineNumber`）の高速カリング。
  - トークン色・フォントウェイト・背景色に基づく GPU グリフバッチ描画。

### REQ-MONACO-006: ミニマップ (Minimap Engine)
- **説明:** ファイル全体のコード構造を俯瞰できる縮小ビュー。
- **詳細要件:**
  - 1/10 スケールでの高速ピクセル/文字プレビュー描画。
  - 現在のビューポート表示領域を示す半透明スライダーとドラッグスクロール。

### REQ-MONACO-007: エディター機能拡張 (Editor Contributions)
- **説明:** コード編集を支援する高度なインテリジェント機能群。
- **詳細要件:**
  - `suggest`: IntelliSense 補完ポップアップとキーボード選択。
  - `hover`: 型シグネチャ、ドキュメント、エラー詳細のホバー表示。
  - `gotoSymbol`: 定義ジャンプ (`F12`)、参照一覧 (`Shift+F12`)。
  - `folding`: 構文/インデントベースのコード折りたたみ。
  - `format`: ドキュメント・選択範囲の自動フォーマット。
