# Oxide Extension API v0.1

> **状態: 実験的。** この文書は、VSIXを利用するNode.js拡張のうち、Oxide Editorで実行可能な最小API集合を定義します。VS Code APIとの完全互換性を意味しません。

## 実行モデル

信頼済みワークスペースでのみ、Oxide EditorはNode.jsの別プロセスとして拡張ホストを起動します。拡張は展開済みVSIX内の`main`をCommonJSとして読み込み、`activationEvents`の`onCommand:<id>`または`onLanguage:<language>`に応じて`activate(context)`されます。インストール直後の外部VSIXは**無効**で保存され、利用者が拡張機能ビューで権限の説明を確認して有効化するまで実行されません。

| 区分 | v0.1での対応 | 備考 |
|---|---|---|
| VSIX manifest | `name`、`publisher`、`version`、`main`、`browser`、`activationEvents`、`extensionKind`、`engines.vscode`、languages、themes、commands | `browser`のみのWeb拡張は実行しません。 |
| コマンド | `contributes.commands`、`vscode.commands.registerCommand`、`executeCommand` | コマンドパレットから実行します。 |
| 通知 | `vscode.window.showInformationMessage`、`showWarningMessage`、`showErrorMessage` | Oxideのステータス領域に表示します。 |
| ワークスペース | `workspace.workspaceFolders`、読み取り専用の`workspace.fs.readFile` | 信頼済みワークスペース内に限定し、1回10 MiBまでです。 |
| 言語機能 | `languages.registerCompletionItemProvider`、`registerHoverProvider` | 文字列の言語セレクターだけを受け付け、応答は2秒で打ち切ります。 |
| ライフサイクル | `activate`、`deactivate`、`context.subscriptions` | 無効化・再読み込み時に`Disposable`を解放します。 |

## 意図的な制約

v0.1では、拡張の書き込み、ネットワーク、子プロセス、Webview、デバッグアダプター、リモート拡張、設定更新、拡張ストレージを提供しません。Nodeの権限モデルで書き込みと子プロセスを拒否し、拡張モジュールからは`fs`、`child_process`、`net`、`http`、`https`、`tls`などの高権限Node組込みモジュールを読み込めないようにしています。`workspace.fs.readFile`はRust側のブローカーを通るため、ワークスペース外へのパス移動やシンボリックリンク経由の脱出も既存のワークスペース境界検証で拒否されます。

この分離は、信頼済みの外部JavaScriptを完全に安全なものにするサンドボックスではありません。未知の拡張を有効にする前に、発行者、ソースコード、依存関係、必要なAPIを確認してください。

## 対応する最小拡張の例

```js
const vscode = require("vscode");

function activate(context) {
  context.subscriptions.push(
    vscode.commands.registerCommand("example.hello", async () => {
      await vscode.window.showInformationMessage("Hello from Oxide");
    }),
    vscode.languages.registerCompletionItemProvider("typescript", {
      provideCompletionItems() {
        return [{ label: "oxideExample", insertText: "oxideExample", kind: 3 }];
      },
    }),
  );
}

module.exports = { activate };
```

この形式の完全な実行用fixtureは、[`examples/oxide-smoke-command/`](../examples/oxide-smoke-command/)にあります。検証はリポジトリ直下で`npm run test:extension-host`を実行してください。
