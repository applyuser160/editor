# Oxide Editor のプロジェクト状況

> **基準日: 2026-08-27**。本書は、リポジトリに含まれるソースコード、依存関係、現行仕様書、および追跡 Issue を基にした実装状況の基準文書です。将来仕様、調査結果、未検証の性能値を実装済みと解釈してはなりません。

## 状態の定義

| 状態 | 意味 |
|---|---|
| 実装済み | 対応するソースコードまたは設定がリポジトリに存在する。網羅的なテスト済みを意味しない。 |
| 部分実装 | 基盤または限定された機能だけが存在し、Issue の受け入れ条件は満たしていない。 |
| 提案・将来仕様 | 要件書、設計書、ADR、調査書に記載されるが、実装の根拠がない。 |
| 未検証 | 実装の有無にかかわらず、再現可能な測定・テスト・対応 OS の確認結果がない。 |

## 実装スナップショット

| 領域 | 状態 | リポジトリ上の根拠 | 制限・追跡先 |
|---|---|---|---|
| デスクトップ基盤 | 実装済み | [Tauri エントリポイント](../src-tauri/src/lib.rs)、[Cargo 定義](../src-tauri/Cargo.toml) | OS ごとの配布・署名・更新は [#48](https://github.com/applyuser160/editor/issues/48) |
| エディター UI | 部分実装 | [Monaco を利用するフロントエンド](../src/main.ts)、[依存関係](../package.json) | Diff/Merge、Peek、Outline、Minimap は [#47](https://github.com/applyuser160/editor/issues/47) |
| ワークスペース | 部分実装 | [ワークスペース状態](../src-tauri/src/workspace.rs)、[現行仕様](workspace-model.md) | 信頼、除外規則、複数ルートは [#62](https://github.com/applyuser160/editor/issues/62) |
| ターミナル・Git | 部分実装 | [PTY 状態](../src-tauri/src/pty_manager.rs)、[Git コマンド](../src-tauri/src/commands.rs) | タスク定義・Problems 連携は [#40](https://github.com/applyuser160/editor/issues/40) |
| 言語支援 | 部分実装 | [LSP クライアント](../src-tauri/src/lsp_client.rs) | デバッグは [#39](https://github.com/applyuser160/editor/issues/39)、複数ファイル編集は [#43](https://github.com/applyuser160/editor/issues/43) |
| 拡張機能 | 部分実装 | [VSIX ライフサイクル](extension-lifecycle.md)、[実装](../src-tauri/src/extension_host.rs) | 拡張コードの実行・API・クラッシュ分離は [#38](https://github.com/applyuser160/editor/issues/38) |
| 設定・プロファイル | 部分実装 | [設定の保存実装](../src/settings.ts) | JSON ファイル永続化と資格情報ストアは [#63](https://github.com/applyuser160/editor/issues/63) |
| テスト・CI | 未検証 | フロントエンドテストランナーと PR 用ワークフローは未導入 | [#64](https://github.com/applyuser160/editor/issues/64) と [#44](https://github.com/applyuser160/editor/issues/44) |
| アクセシビリティ・国際化 | 部分実装 | 日本語 UI 文字列は存在する | WCAG、キーボード操作、ローカライズ、IME 検証は [#49](https://github.com/applyuser160/editor/issues/49) |

## 文書の位置付け

| 文書群 | 種別 | 読み方 |
|---|---|---|
| `README.md`、`workspace-model.md`、`extension-lifecycle.md`、`security/native-security-boundaries.md` | 現行実装の説明 | 本書のスナップショットと矛盾する場合は Issue を作成して是正する。 |
| `docs/adr/` | 意思決定記録 | ADR ごとのステータスを確認する。`Deferred` と `Proposed` は未実装である。 |
| `docs/requirements/`、`docs/design/` | 将来仕様 | 文書冒頭のステータスに従う。実装の証拠ではない。 |
| `docs/research/` | 調査記録 | 技術選定時の比較・仮説であり、現行の性能・互換性を示さない。 |
| `docs/checklist/` | 検証・調査の追跡 | チェック状態は実装完了を示さない。根拠へのリンクがあるときだけ検証済みとする。 |

## 性能・互換性・セキュリティの検証方針

現時点で、起動時間、メモリ使用量、入力遅延、フレームレート、ファイル操作性能、検索性能、VS Code API 互換率、OS 対応範囲について、公開可能な再現試験はありません。したがって、数値目標や比較表の数値は **未検証の目標または調査時点の仮説** と扱います。

性能値を公開する変更では、少なくとも対象コミット、OS・CPU・メモリ・GPU・ストレージ、ビルド種別、測定コマンド、測定対象データ、反復回数、集計方法、元データを併記します。互換性を主張する変更では、対象 API・拡張機能・OS の範囲と E2E テスト結果を示します。セキュリティ特性を主張する変更では、脅威モデル、適用範囲、テストまたはレビュー結果、既知の除外事項を記録します。

## 文書保守ルール

実装を追加した PR では、対応する Issue の受け入れ条件、必要であれば本書の状態、関連する ADR、現行仕様を同じ変更内で見直します。計画・要件・調査だけを更新する PR では、実装済みを示す言葉、性能実績、完全互換の表現を使わず、設計・目標・提案であることを明示します。

## 追跡対象外ファイル

`sample.py` と `scratch_patch.py` は、製品機能やテストで使用されないため削除しました。`welcome.rs` は初期編集タブで表示する歓迎用の Rust フィクスチャであり、アプリケーションの Rust バックエンドではありません。
