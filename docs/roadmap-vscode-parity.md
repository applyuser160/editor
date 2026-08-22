# VS Code 完全互換へのロードマップ（Phase 1 〜 Phase 6）

> Microsoft VS Code (`microsoft/vscode`) との機能完全互換およびパフォーマンス超克を達成するためのロードマップです。

---

## 🗺️ マイルストーン概要

| フェーズ | 目標 | 主な成果物 |
| :---: | :--- | :--- |
| **Phase 1** | **基盤 & コアエディタ（完了）** | Rope TextBuffer, Multi-Cursor, Undo/Redo, 日本語GUI, 統合ターミナル |
| **Phase 2** | **Workbench & QuickPick** | Command Palette (`Ctrl+Shift+P`), QuickOpen (`Ctrl+P`), Minimap, Split Editor |
| **Phase 3** | **LSP & Language Tools** | LSP Client (rust-analyzer, tsserver), Definition Jump, Diagnostics Hover, Auto-complete |
| **Phase 4** | **Git & SCM 拡張** | 3-way Merge, Interactive Staging, Git Graph, Branch Graph |
| **Phase 5** | **VS Code 拡張機能互換** | `vscode.*` API Layer, WASM/QuickJS Engine, `.vsix` Package Installer |
| **Phase 6** | **DAP (デバッガ) & パフォーマンス最適化** | Debug Adapter Protocol (lldb, gdb, node), ゼロアロケーション描画, メモリ<30MB |

---

## 🎯 各フェーズ詳細計画

### Phase 2: Workbench & QuickPick (現在進行中)
- [x] VS Code ライクな ActivityBar, SideBar, Status Bar, Menu Bar
- [ ] **QuickPick / Command Palette (`Ctrl+P` / `Ctrl+Shift+P`)**
- [ ] **Minimap (ミニマップ描画)**
- [ ] **エディターの左右・上下分割 (Split Editor)**
- [ ] **VS Code 設定 (`settings.json`) / キーバインド (`keybindings.json`) 互換**

### Phase 3: LSP 完全統合
- [ ] 言語サーバー（`rust-analyzer` 等）の自動検出とバックグラウンド起動
- [ ] コード補完ポップアップ (IntelliSense) とキーナビゲーション
- [ ] `F12` / `Ctrl+Click` による定義ジャンプ
- [ ] エラー・警告のホバーツールチップとクイックフィックス (Code Actions)

### Phase 5: VS Code 拡張機能エコシステム
- [ ] Open VSX / Marketplace API からのプラグイン検索 & ダウンロード
- [ ] TextMate テーマ (`.json`) の読み込みと完全なカラースキーム再現
- [ ] サンドボックス内での `vscode.commands` / `vscode.workspace` 実行
