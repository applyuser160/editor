# VS Code 参照アーキテクチャ調査カタログ

> この文書は、Microsoft の [`microsoft/vscode`](https://github.com/microsoft/vscode) を参照する際の**調査対象カタログ**です。ここに列挙するモジュールは Oxide Editor に実装済みであること、または VS Code の全ファイルを解析済みであることを示しません。実装状況は [プロジェクト状況](../project-status.md) を、具体的な未実装機能は関連 Issue を参照してください。

## 調査対象の分類

| 参照領域 | 代表的な対象 | Oxide Editor の扱い |
|---|---|---|
| `src/vs/base/` | ライフサイクル、イベント、URI、IPC、ファイルシステム | 必要な概念を選択的に参照する。VS Code 実装の移植状況は未評価。 |
| `src/vs/platform/` | 構成、コマンド、ストレージ、通知、テーマ | JSON 設定サービス・汎用 DI・完全なストレージ抽象は未実装。 [#63](https://github.com/applyuser160/editor/issues/63) |
| `src/vs/editor/` | テキストモデル、言語機能、表示、エディター寄与機能 | Monaco を利用する。Ropey、Tree-sitter、独自 GPU レンダラーは未採用。 |
| `src/vs/workbench/` | Activity Bar、サイドバー、パネル、SCM、検索、ターミナル | UI と一部バックエンド機能を実装する。高度な編集機能は [#47](https://github.com/applyuser160/editor/issues/47)。 |
| `src/vs/workbench/api/` | Extension Host、RPC、`vscode.*` API | VSIX の管理のみ実装。拡張コード実行と API 互換は未提供。 [#38](https://github.com/applyuser160/editor/issues/38) |
| `src/vs/code/` | アプリケーションのライフサイクルとネイティブシェル | Tauri を利用する。Electron の機構をエミュレートしない。 |
| `src/vs/server/` | SSH、WSL、コンテナ、トンネル、Web クライアント | 将来仕様。リモート開発は未実装。 |
| `extensions/` | Git、Markdown、テーマ、設定編集 | VSIX マニフェスト由来の限定情報を扱う。標準拡張 API の完全実装はない。 |

## 利用ルール

調査を完了した範囲は、参照元のコミット、対象パス、調査日、得られた設計上の判断を PR または ADR へ記録します。実装を完了した範囲は、対応ソース、テスト、受け入れ条件、追跡 Issue を明記します。調査完了と実装完了を同じチェック記号・達成率で表現してはなりません。

## 実装に関する主な追跡先

| 領域 | Issue |
|---|---|
| 設定とストレージ | [#63](https://github.com/applyuser160/editor/issues/63) |
| ワークスペース境界・信頼・複数ルート | [#62](https://github.com/applyuser160/editor/issues/62) |
| 高度なエディター操作 | [#47](https://github.com/applyuser160/editor/issues/47) |
| テスト実行 | [#44](https://github.com/applyuser160/editor/issues/44) |
| 複数ファイル編集 | [#43](https://github.com/applyuser160/editor/issues/43) |
| タスク実行 | [#40](https://github.com/applyuser160/editor/issues/40) |
| DAP デバッグ | [#39](https://github.com/applyuser160/editor/issues/39) |
| Extension Host | [#38](https://github.com/applyuser160/editor/issues/38) |
