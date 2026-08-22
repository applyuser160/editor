# VS Code 拡張機能互換 Extension Host 設計仕様書

> VS Code の拡張機能エコシステム（Marketplace / `.vsix`）を活用し、`vscode.*` API を Rust 上でエミュレートしてサンドボックス実行するためのアーキテクチャ設計書です。

---

## 1. Extension Host アーキテクチャ

```mermaid
graph TB
    subgraph MainProcess ["Main Process (Oxide Editor / UI Thread)"]
        Workbench["🎨 Workbench UI"]
        MainRPC["🔌 Main Thread RPC Protocol"]
        DocManager["📄 Document / Buffer Manager"]
    end

    subgraph ExtHostProcess ["Extension Host Process (Isolated Thread / Sandbox)"]
        ExtRPC["🔌 ExtHost RPC Protocol"]
        VsCodeAPI["🧩 vscode.* API Implementation"]
        ExtRegistry["📦 Extension Registry & Loader"]
        WasmRuntime["⚡ WASM / QuickJS Engine"]
    end

    Workbench -->|"UI Events"| MainRPC
    MainRPC <==|"Bi-directional IPC (Shared Memory / Streams)"|==> ExtRPC
    ExtRPC --> VsCodeAPI
    VsCodeAPI --> ExtRegistry
    ExtRegistry --> WasmRuntime
    MainRPC --> DocManager
```

---

## 2. コア API 互換レイヤーの仕様

### 2.1 `vscode.commands`
- `registerCommand(command: string, callback: Function): Disposable`
- `executeCommand(command: string, ...args: any[]): Thenable<any>`

### 2.2 `vscode.window`
- `showInformationMessage(message: string, ...items: string[]): Thenable<string>`
- `showErrorMessage(message: string): Thenable<void>`
- `showQuickPick(items: string[] | QuickPickItem[]): Thenable<QuickPickItem>`
- `createTerminal(name?: string, shellPath?: string): Terminal`
- `activeTextEditor: TextEditor | undefined`

### 2.3 `vscode.workspace`
- `openTextDocument(uri: Uri): Thenable<TextDocument>`
- `onDidChangeTextDocument: Event<TextDocumentChangeEvent>`
- `getConfiguration(section?: string): WorkspaceConfiguration`
- `fs: FileSystem`

### 2.4 `vscode.languages`
- `registerCompletionItemProvider(selector: DocumentSelector, provider: CompletionItemProvider): Disposable`
- `registerHoverProvider(selector: DocumentSelector, provider: HoverProvider): Disposable`
- `registerDefinitionProvider(selector: DocumentSelector, provider: DefinitionProvider): Disposable`

---

## 3. VSIX パッケージのインストールとライフサイクル

1. **パッケージ展開:** `.vsix` は ZIP 形式であり、`package.json`（Manifest）と拡張機能コードを抽出。
2. **`package.json` の `contributes` 解析:**
   - `commands` (コマンドパレット登録)
   - `languages` (言語定義 & 文法)
   - `grammars` (TextMate 構文ハイライト)
   - `themes` (VS Code カラーテーマ)
   - `keybindings` (キーバインド)
   - `menus` (コンテキストメニュー)
3. **アクティベーションイベント (`activationEvents`):**
   - `onLanguage:rust` (Rust ファイルが開かれた時に遅延起動)
   - `onCommand:custom.command` (コマンド実行時に遅延起動)
   - `*` (起動時常時ロード)
