# Oxide Editor のデバッグ起動構成

Oxide Editor は、Debug Adapter Protocol（DAP）によるデバッグ実行に向けた第一段階として、起動構成を読み込み、検証する基盤を提供します。設定ファイルは優先順に `.vscode/launch.json`、`.oxide/launch.json` を検索します。どちらもなければ、Rust実行ファイルを対象にした既定構成を返します。

## 対応形式

各設定は `name`、`type`、`request` を必須とします。`type` は `lldb` または `python`、`request` は `launch` または `attach` を受け付けます。`launch` の場合は `program` が必要です。`program` と `cwd` はワークスペース内に解決でき、存在する必要があります。設定例は次のとおりです。

```json
{
  "configurations": [
    {
      "name": "Rust: Debug app",
      "type": "lldb",
      "request": "launch",
      "program": "target/debug/app",
      "cwd": ".",
      "args": [],
      "env": {}
    }
  ]
}
```

## 現在の範囲

この実装は構成の選択と失敗理由の検証までを提供します。DAPアダプタプロセスとのJSON-RPC通信、ブレークポイント、継続・ステップ実行、スタック、変数、ウォッチ、標準出力、デバッグコンソールの実行部分は後続の実装対象です。実行機能を有効にする前に、最初のアダプタ（CodeLLDBまたはdebugpy）の導入確認、アダプタプロセスのライフサイクル、DAPメッセージの厳格な型、セッション終了時の後始末、UI状態モデルを追加します。
