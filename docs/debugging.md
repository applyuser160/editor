# デバッグ

Oxide Editor は Debug Adapter Protocol（DAP）を用いて、Rust の LLDB または Python の `debugpy` を最初の対応アダプタとして接続します。デバッグビューは、アクティビティバーの **実行とデバッグ** から開きます。行番号余白をクリックするとブレークポイントを設定または解除できます。

## 起動構成

起動構成は、ワークスペースの `.vscode/launch.json` を優先して読み込み、存在しない場合は `.oxide/launch.json` を読み込みます。各構成は `name`、`type`、`request` を必須とします。`launch` では `program` を必須とし、パスはワークスペース内に存在する必要があります。

```json
{
  "configurations": [
    {
      "name": "Python: 現在のファイル",
      "type": "python",
      "request": "launch",
      "program": "examples/hello.py",
      "cwd": ".",
      "args": [],
      "env": {}
    }
  ]
}
```

Rust の実行可能ファイルを LLDB で開始する場合は、`type` を `lldb` に変更し、`program` にビルド済み実行可能ファイルへのワークスペース内パスを指定します。

## アダプタの準備

| 言語 | 構成の `type` | 必要なアダプタ | 開始時の確認 |
|---|---|---|---|
| Rust | `lldb` | `lldb-dap`、または旧形式の `lldb-vscode` | 実行ファイルが `PATH` 上にあること |
| Python | `python` | Python と `debugpy` | Python で `import debugpy` が成功すること |

Python のアダプタが見つからない場合は、対象環境で `python -m pip install debugpy` を実行してください。アダプタが未導入、起動構成が不正、対象プログラムまたは作業ディレクトリが存在しない、ワークスペース外のパスを指定した場合は、開始操作で原因を示すメッセージを表示します。

## デバッグ操作

セッションの開始後、デバッグビューから **Continue**、**Step Over**、**Step Into**、**Step Out**、**Pause**、**停止**を実行できます。停止イベントを受信すると、コールスタック、先頭フレームのローカル変数、Watch 式を表示します。フレームを選択すると、そのフレームのスコープを再読込します。REPL 欄では停止中のフレームに対して式を評価できます。

アダプタの標準出力・標準エラー出力、ブレークポイントの更新、停止理由は下部パネルの **出力** チャネルにも追記されます。アダプタが終了または切断された場合は、セッション状態を終了へ戻します。

## 検証上の注意

本リポジトリのユニットテストは DAP フレームの `Content-Length` 付きメッセージを検証します。実際のアダプタとのデバッグは、利用者の OS に導入された `lldb-dap` または `debugpy` と、上記の有効な `launch.json` を使用して確認してください。
