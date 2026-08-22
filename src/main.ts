import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as monaco from "monaco-editor";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  depth: number;
}

interface SearchMatch {
  file_path: string;
  line_number: number;
  line_text: string;
}

interface GitStatusResult {
  branch: string;
  changed_files: string[];
}

interface ExtensionManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  contributes_languages: string[];
  contributes_themes: string[];
}

interface OpenVsxExtension {
  namespace: string;
  name: string;
  version: string;
  display_name: string | null;
  description: string | null;
  download_count: number | null;
}

interface OpenTab {
  path: string;
  name: string;
  model: monaco.editor.ITextModel;
  isDirty: boolean;
}

interface MenuItemDef {
  type?: "separator";
  label?: string;
  shortcut?: string;
  disabled?: boolean;
  submenu?: MenuItemDef[];
  action?: () => void;
}

// Global State
let editor1: monaco.editor.IStandaloneCodeEditor | null = null;
let editor2: monaco.editor.IStandaloneCodeEditor | null = null;
let isSplitActive = false;
let splitOrientation: "horizontal" | "vertical" = "horizontal";

let ptyId: number | null = null;
let xtermInstance: Terminal | null = null;
let fitAddon: FitAddon | null = null;

let isTopMenuOpen = false;
let currentOpenMenuKey: string | null = null;
let activeSubmenuEl: HTMLElement | null = null;

let quickPickItems: Array<{ id: string; title: string; subtitle?: string; shortcut?: string; action: () => void }> = [];
let quickPickSelectedIndex = 0;

const openTabs: Map<string, OpenTab> = new Map();
let activeFilePath: string | null = null;
let currentActiveView = "explorer";
let isSidebarVisible = true;
let isTerminalVisible = true;

const activeLspServers = new Set<string>();

// Initialize when DOM is ready
window.addEventListener("DOMContentLoaded", () => {
  initLanguageServerIntegration();
  initMonacoEditors();
  setupVSCodeMenus();
  setupActivityBar();
  setupResizers();
  setupGridSplitters();
  setupIntegratedTerminal();
  setupBranchSwitcher();
  setupQuickPick();
  setupShortcuts();
  setupFileActions();
  setupFileWatcherListener();
  initExtensionHost();
  loadWorkspaceFiles();
});

// 1. Language Server Protocol (LSP) Integration (Hover, Completion, Definition, Formatting, Diagnostics)
async function initLanguageServerIntegration() {
  const supportedLangs = ["rust", "typescript", "javascript", "python", "go"];

  // 1. Diagnostics Listener
  await listen<{ lang: string; params: any }>("lsp-diagnostics", (event) => {
    const { params } = event.payload;
    if (!params || !params.uri || !params.diagnostics) return;

    const uriStr = params.uri;
    openTabs.forEach((tab) => {
      if (uriStr.endsWith(tab.path.replace(/\\/g, "/"))) {
        const markers: monaco.editor.IMarkerData[] = params.diagnostics.map((d: any) => ({
          severity:
            d.severity === 1
              ? monaco.MarkerSeverity.Error
              : d.severity === 2
              ? monaco.MarkerSeverity.Warning
              : monaco.MarkerSeverity.Info,
          message: d.message,
          startLineNumber: (d.range?.start?.line ?? 0) + 1,
          startColumn: (d.range?.start?.character ?? 0) + 1,
          endLineNumber: (d.range?.end?.line ?? 0) + 1,
          endColumn: (d.range?.end?.character ?? 0) + 1,
        }));
        monaco.editor.setModelMarkers(tab.model, "lsp", markers);
        updateStatusMarkersCount(markers);
      }
    });
  });

  // 2. Register LSP Providers for all supported languages
  supportedLangs.forEach((lang) => {
    // Hover Provider
    monaco.languages.registerHoverProvider(lang, {
      provideHover: async (model, position) => {
        const uri = `file:///${model.uri.path}`;
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/hover",
            params: {
              textDocument: { uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            },
          });
          if (res && res.contents) {
            const rawContent = Array.isArray(res.contents) ? res.contents.map((c: any) => c.value || c).join("\n\n") : res.contents.value || res.contents;
            return {
              contents: [{ value: rawContent }],
            };
          }
        } catch (e) {
          // Fallback to symbol lookup
        }

        const word = model.getWordAtPosition(position);
        if (word) {
          return {
            contents: [{ value: `**${word.word}** (${lang})` }],
          };
        }
        return null;
      },
    });

    // Completion Provider
    monaco.languages.registerCompletionItemProvider(lang, {
      provideCompletionItems: async (model, position) => {
        const uri = `file:///${model.uri.path}`;
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/completion",
            params: {
              textDocument: { uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            },
          });
          if (res) {
            const items = Array.isArray(res) ? res : res.items || [];
            const suggestions: monaco.languages.CompletionItem[] = items.map((item: any) => ({
              label: item.label,
              kind: mapLspKindToMonaco(item.kind),
              detail: item.detail,
              documentation: item.documentation?.value || item.documentation,
              insertText: item.insertText || item.label,
              range: {
                startLineNumber: position.lineNumber,
                startColumn: position.column - (model.getWordUntilPosition(position).word.length),
                endLineNumber: position.lineNumber,
                endColumn: position.column,
              },
            }));
            return { suggestions };
          }
        } catch (e) {
          // Fallback
        }
        return { suggestions: [] };
      },
    });

    // Go to Definition Provider (F12)
    monaco.languages.registerDefinitionProvider(lang, {
      provideDefinition: async (model, position) => {
        const word = model.getWordAtPosition(position);
        if (!word) return null;
        const targetSymbol = word.word;

        // 1. ALWAYS search in the current model first!
        const currentMatches = model.findMatches(
          `\\b(fn|let|struct|enum|trait|class|def|function|const|var|interface|type)\\s+${targetSymbol}\\b`,
          false,
          true,
          false,
          null,
          true
        );

        if (currentMatches.length > 0) {
          const match = currentMatches[0];
          return {
            uri: model.uri,
            range: match.range,
          };
        }

        // 2. Try LSP server if connected
        const targetUriPath = activeFilePath || model.uri.path;
        const uri = `file:///${targetUriPath.replace(/\\/g, "/")}`;
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/definition",
            params: {
              textDocument: { uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            },
          });

          if (res) {
            const loc = Array.isArray(res) ? res[0] : res;
            const targetUri = loc?.uri || loc?.targetUri;
            const targetRange = loc?.range || loc?.targetSelectionRange || loc?.targetRange;
            if (targetUri && targetRange) {
              const rawPath = targetUri.replace(/^file:\/\/\/?/, "");
              const targetPath = decodeURIComponent(rawPath);
              const fileName = targetPath.split(/[/\\]/).pop() || targetPath;
              await openFile(targetPath, fileName);
              return {
                uri: monaco.Uri.parse(targetUri),
                range: new monaco.Range(
                  (targetRange.start?.line ?? 0) + 1,
                  (targetRange.start?.character ?? 0) + 1,
                  (targetRange.end?.line ?? targetRange.start?.line ?? 0) + 1,
                  (targetRange.end?.character ?? targetRange.start?.character ?? 0) + 1
                ),
              };
            }
          }
        } catch (e) {
          // Fallback to workspace symbol search
        }

        // 3. Search other files in workspace
        try {
          const matches = await invoke<SearchMatch[]>("search_in_workspace", {
            query: targetSymbol,
            caseSensitive: true,
          });

          const defMatch = matches.find((m) => {
            const line = m.line_text;
            return (
              line.includes(`fn ${targetSymbol}`) ||
              line.includes(`struct ${targetSymbol}`) ||
              line.includes(`enum ${targetSymbol}`) ||
              line.includes(`class ${targetSymbol}`) ||
              line.includes(`def ${targetSymbol}`) ||
              line.includes(`function ${targetSymbol}`) ||
              line.includes(`const ${targetSymbol}`) ||
              line.includes(`let ${targetSymbol}`)
            );
          });

          if (defMatch) {
            await openFile(defMatch.file_path, defMatch.file_path.split("/").pop() || defMatch.file_path);
            const targetTab = openTabs.get(defMatch.file_path);
            if (targetTab) {
              return {
                uri: targetTab.model.uri,
                range: new monaco.Range(defMatch.line_number, 1, defMatch.line_number, targetSymbol.length + 1),
              };
            }
          }
        } catch (e) {
          console.error(e);
        }

        return null;
      },
    });

    // Document Formatting Provider (Shift+Alt+F)
    monaco.languages.registerDocumentFormattingEditProvider(lang, {
      provideDocumentFormattingEdits: async (model) => {
        const uri = `file:///${model.uri.path}`;
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/formatting",
            params: {
              textDocument: { uri },
              options: { tabSize: 4, insertSpaces: true },
            },
          });
          if (Array.isArray(res)) {
            return res.map((e: any) => ({
              range: new monaco.Range(
                e.range.start.line + 1,
                e.range.start.character + 1,
                e.range.end.line + 1,
                e.range.end.character + 1
              ),
              text: e.newText,
            }));
          }
        } catch (e) {
          console.error("Formatting error:", e);
        }
        return [];
      },
    });
  });
}

function mapLspKindToMonaco(kind: number): monaco.languages.CompletionItemKind {
  switch (kind) {
    case 1: return monaco.languages.CompletionItemKind.Text;
    case 2: return monaco.languages.CompletionItemKind.Method;
    case 3: return monaco.languages.CompletionItemKind.Function;
    case 4: return monaco.languages.CompletionItemKind.Constructor;
    case 5: return monaco.languages.CompletionItemKind.Field;
    case 6: return monaco.languages.CompletionItemKind.Variable;
    case 7: return monaco.languages.CompletionItemKind.Class;
    case 8: return monaco.languages.CompletionItemKind.Interface;
    case 9: return monaco.languages.CompletionItemKind.Module;
    case 10: return monaco.languages.CompletionItemKind.Property;
    case 14: return monaco.languages.CompletionItemKind.Keyword;
    case 15: return monaco.languages.CompletionItemKind.Snippet;
    default: return monaco.languages.CompletionItemKind.Text;
  }
}

async function ensureLspServerStarted(lang: string) {
  if (activeLspServers.has(lang)) return;

  const validLspLangs = ["rust", "typescript", "javascript", "python", "go"];
  if (!validLspLangs.includes(lang)) return;

  try {
    const res = await invoke<string>("lsp_start_server", {
      lang,
      workspaceRoot: ".",
    });
    activeLspServers.add(lang);
    showStatusMessage(`LSP: ${res}`);
  } catch (err) {
    console.warn(`LSP startup notice: ${err}`);
  }
}

function updateStatusMarkersCount(markers: monaco.editor.IMarkerData[]) {
  const errors = markers.filter((m) => m.severity === monaco.MarkerSeverity.Error).length;
  const warnings = markers.filter((m) => m.severity === monaco.MarkerSeverity.Warning).length;
  const statusMarkersEl = document.querySelector("#statusbar .statusbar-left span:nth-child(2)");
  if (statusMarkersEl) {
    statusMarkersEl.textContent = `⚠️ ${warnings}  ❌ ${errors}`;
  }
}

// 2. Initialize Monaco Editors
function initMonacoEditors() {
  const container1 = document.getElementById("editor-container-1");
  const container2 = document.getElementById("editor-container-2");
  if (!container1 || !container2) return;

  monaco.editor.defineTheme("vscode-dark-plus", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "6A9955" },
      { token: "keyword", foreground: "569CD6" },
      { token: "string", foreground: "CE9178" },
      { token: "number", foreground: "B5CEA8" },
      { token: "type", foreground: "4EC9B0" },
      { token: "function", foreground: "DCDCAA" },
      { token: "variable", foreground: "9CDCFE" },
    ],
    colors: {
      "editor.background": "#1e1e1e",
      "editor.foreground": "#d4d4d4",
      "editorCursor.foreground": "#aeafad",
      "editor.lineHighlightBackground": "#2a2d2e",
      "editorLineNumber.foreground": "#858585",
      "editorLineNumber.activeForeground": "#c6c6c6",
      "editor.selectionBackground": "#264f78",
      "editor.inactiveSelectionBackground": "#3a3d41",
    },
  });

  const defaultContent = `// 🦀 Welcome to Oxide Editor (VS Code on Tauri v2)!
// Ultra-fast, Memory-Efficient, Native Desktop IDE.

struct UserProfile {
    username: String,
    role: String,
}

fn calculate_total(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let user = UserProfile {
        username: String::from("OxideUser"),
        role: String::from("Developer"),
    };

    let total = calculate_total(10, 20);
    println!("User: {}, Total: {}", user.username, total);
}
`;

  const initialModel = monaco.editor.createModel(defaultContent, "rust");

  const commonOptions: monaco.editor.IStandaloneEditorConstructionOptions = {
    theme: "vscode-dark-plus",
    fontSize: 14,
    fontFamily: "Consolas, 'Courier New', monospace",
    lineNumbers: "on",
    roundedSelection: false,
    scrollBeyondLastLine: false,
    readOnly: false,
    cursorBlinking: "smooth",
    smoothScrolling: true,
    minimap: { enabled: true, scale: 1, showSlider: "mouseover" },
    automaticLayout: true,
    tabSize: 4,
    insertSpaces: true,
    contextmenu: true,
    mouseWheelZoom: true,
    gotoLocation: {
      multiple: "peek",
    },
  };

  editor1 = monaco.editor.create(container1, {
    ...commonOptions,
    model: initialModel,
  });

  editor2 = monaco.editor.create(container2, {
    ...commonOptions,
    model: initialModel,
  });

  editor1.onDidFocusEditorText(() => closeGlobalMenu());
  editor2.onDidFocusEditorText(() => closeGlobalMenu());
  editor1.onMouseDown(() => closeGlobalMenu());
  editor2.onMouseDown(() => closeGlobalMenu());

  editor1.onDidChangeCursorPosition((e) => {
    const statusLineCol = document.getElementById("status-line-col");
    if (statusLineCol) {
      statusLineCol.textContent = `行: ${e.position.lineNumber}, 列: ${e.position.column}`;
    }
  });

  initialModel.onDidChangeContent(() => {
    if (activeFilePath && openTabs.has(activeFilePath)) {
      const tab = openTabs.get(activeFilePath)!;
      tab.isDirty = true;
      updateTabBar();
    }
  });

  // Add explicit Go to Definition action to Monaco context menu and F12
  editor1.addAction({
    id: "oxide.gotoDefinition",
    label: "定義へ移動 (Go to Definition)",
    keybindings: [monaco.KeyCode.F12],
    contextMenuGroupId: "navigation",
    contextMenuOrder: 1.5,
    run: () => {
      performGoToDefinition();
    },
  });

  editor2.addAction({
    id: "oxide.gotoDefinition",
    label: "定義へ移動 (Go to Definition)",
    keybindings: [monaco.KeyCode.F12],
    contextMenuGroupId: "navigation",
    contextMenuOrder: 1.5,
    run: () => {
      performGoToDefinition();
    },
  });

  openTabs.set("welcome.rs", {
    path: "welcome.rs",
    name: "welcome.rs",
    model: initialModel,
    isDirty: false,
  });
  activeFilePath = "welcome.rs";
  ensureLspServerStarted("rust");
  updateTabBar();
}

// 2.5 Perform Go to Definition (Instant AST & Regex Fallback)
async function performGoToDefinition() {
  if (!editor1) return;
  editor1.focus();
  const position = editor1.getPosition();
  const model = editor1.getModel();
  if (!position || !model) return;

  const word = model.getWordAtPosition(position);
  if (!word) {
    showStatusMessage("定義に移動: カーソル位置にシンボル（識別子）がありません");
    return;
  }

  const targetSymbol = word.word;
  showStatusMessage(`定義を検索中: '${targetSymbol}'...`);

  // 1. Check current model for definition
  const currentMatches = model.findMatches(
    `\\b(fn|let|struct|enum|trait|class|def|function|const|var|interface|type)\\s+${targetSymbol}\\b`,
    false,
    true,
    false,
    null,
    true
  );

  if (currentMatches.length > 0) {
    const match = currentMatches[0];
    editor1.revealRangeInCenter(match.range);
    editor1.setPosition({ lineNumber: match.range.startLineNumber, column: match.range.startColumn });
    editor1.setSelection(match.range);
    showStatusMessage(`📍 定義へジャンプ完了: '${targetSymbol}' (${match.range.startLineNumber}行目)`);
    return;
  }

  // 2. Try LSP server if connected (supports external libraries & standard library)
  const lang = model.getLanguageId();
  try {
    const targetUriPath = activeFilePath || model.uri.path;
    const uri = `file:///${targetUriPath.replace(/\\/g, "/")}`;
    const res: any = await invoke("lsp_send_request", {
      lang,
      method: "textDocument/definition",
      params: {
        textDocument: { uri },
        position: { line: position.lineNumber - 1, character: position.column - 1 },
      },
    });

    if (res) {
      const loc = Array.isArray(res) ? res[0] : res;
      const targetUri = loc?.uri || loc?.targetUri;
      const targetRange = loc?.range || loc?.targetSelectionRange || loc?.targetRange;
      if (targetUri && targetRange) {
        const rawPath = targetUri.replace(/^file:\/\/\/?/, "");
        const targetPath = decodeURIComponent(rawPath);
        const fileName = targetPath.split(/[/\\]/).pop() || targetPath;
        await openFile(targetPath, fileName);
        if (editor1) {
          const line = (targetRange.start?.line ?? 0) + 1;
          const col = (targetRange.start?.character ?? 0) + 1;
          const endLine = (targetRange.end?.line ?? targetRange.start?.line ?? 0) + 1;
          const endCol = (targetRange.end?.character ?? targetRange.start?.character ?? 0) + 1;
          const range = new monaco.Range(line, col, endLine, endCol);
          editor1.revealRangeInCenter(range);
          editor1.setPosition({ lineNumber: line, column: col });
          editor1.setSelection(range);
          showStatusMessage(`📍 定義へジャンプ完了 (LSP): '${targetSymbol}' -> ${fileName}:${line}`);
          return;
        }
      }
    }
  } catch (e) {
    // Fallback to workspace symbol search
  }

  // 3. Fallback: Search workspace
  try {
    const workspaceMatches = await invoke<SearchMatch[]>("search_in_workspace", {
      query: targetSymbol,
      caseSensitive: true,
    });

    const defMatch = workspaceMatches.find((m) => {
      const line = m.line_text;
      return (
        line.includes(`fn ${targetSymbol}`) ||
        line.includes(`struct ${targetSymbol}`) ||
        line.includes(`enum ${targetSymbol}`) ||
        line.includes(`class ${targetSymbol}`) ||
        line.includes(`def ${targetSymbol}`) ||
        line.includes(`function ${targetSymbol}`) ||
        line.includes(`const ${targetSymbol}`) ||
        line.includes(`let ${targetSymbol}`)
      );
    });

    if (defMatch) {
      await openFile(defMatch.file_path, defMatch.file_path.split("/").pop() || defMatch.file_path);
      if (editor1) {
        editor1.revealLineInCenter(defMatch.line_number);
        editor1.setPosition({ lineNumber: defMatch.line_number, column: 1 });
        showStatusMessage(`📍 定義へジャンプ完了: '${targetSymbol}' -> ${defMatch.file_path}:${defMatch.line_number}`);
      }
      return;
    }
  } catch (e) {
    console.error(e);
  }

  showStatusMessage(`'${targetSymbol}' の定義は見つかりませんでした`);
}

// 3. VS Code Exact Menu System
function setupVSCodeMenus() {
  const menuDefs: Record<string, MenuItemDef[]> = {
    file: [
      { label: "新しいテキスト ファイル", shortcut: "Ctrl+N", action: () => document.getElementById("btn-new-file")?.click() },
      { label: "新しいファイル...", shortcut: "Ctrl+Alt+Win+N", action: () => document.getElementById("btn-new-file")?.click() },
      { label: "新しいウィンドウ", shortcut: "Ctrl+Shift+N", action: () => showStatusMessage("新しいウィンドウを開きます") },
      {
        label: "プロファイルを含む新しいウィンドウ",
        submenu: [
          { label: "既定 (Default)", action: () => showStatusMessage("既定プロファイルで起動") },
        ],
      },
      { type: "separator" },
      { label: "ファイルを開く...", shortcut: "Ctrl+O", action: () => openQuickPick(false) },
      { label: "フォルダーを開く...", shortcut: "Ctrl+K Ctrl+O", action: () => loadWorkspaceFiles() },
      { label: "ファイルでワークスペースを開く...", action: () => openQuickPick(false) },
      {
        label: "最近使用した項目を開く",
        submenu: [
          { label: "welcome.rs", action: () => openFile("welcome.rs", "welcome.rs") },
          { label: "Cargo.toml", action: () => openFile("src-tauri/Cargo.toml", "Cargo.toml") },
          { label: "package.json", action: () => openFile("package.json", "package.json") },
        ],
      },
      { type: "separator" },
      { label: "フォルダーをワークスペースに追加...", action: () => document.getElementById("btn-new-folder")?.click() },
      { label: "名前を付けてワークスペースを保存...", action: () => showStatusMessage("ワークスペースを保存しました") },
      { label: "ワークスペースを複製", shortcut: "Ctrl+W Ctrl+A", action: () => showStatusMessage("ワークスペースを複製しました") },
      { type: "separator" },
      { label: "保存", shortcut: "Ctrl+S", action: () => saveActiveFile() },
      { label: "名前を付けて保存...", shortcut: "Ctrl+Shift+S", action: () => saveActiveFile() },
      { label: "すべて保存", shortcut: "Ctrl+K S", action: () => saveActiveFile() },
      { type: "separator" },
      {
        label: "共有",
        submenu: [
          { label: "GitHub で共有...", action: () => window.open("https://github.com/applyuser160/editor", "_blank") },
        ],
      },
      { type: "separator" },
      { label: "自動保存", action: () => showStatusMessage("自動保存を有効にしました") },
      {
        label: "ユーザー設定",
        submenu: [
          { label: "設定 (Settings)", shortcut: "Ctrl+,", action: () => document.querySelector<HTMLButtonElement>('[data-view="settings"]')?.click() },
          { label: "キーボード ショートカット", shortcut: "Ctrl+K Ctrl+S", action: () => openQuickPick(true) },
          { label: "拡張機能 (Extensions)", shortcut: "Ctrl+Shift+X", action: () => document.querySelector<HTMLButtonElement>('[data-view="extensions"]')?.click() },
        ],
      },
      { type: "separator" },
      { label: "ファイルを元に戻す", action: () => { if (activeFilePath) openFile(activeFilePath, activeFilePath); } },
      { label: "エディターを閉じる", shortcut: "Ctrl+F4", action: () => { if (activeFilePath) closeTab(activeFilePath); } },
      { label: "フォルダーを閉じる", shortcut: "Ctrl+K F", action: () => { openTabs.clear(); activeFilePath = null; updateTabBar(); } },
      { label: "ウィンドウを閉じる", shortcut: "Alt+F4", action: () => window.close() },
      { type: "separator" },
      { label: "終了", action: () => window.close() },
    ],

    edit: [
      { label: "元に戻す", shortcut: "Ctrl+Z", action: () => editor1?.trigger("menu", "undo", null) },
      { label: "やり直し", shortcut: "Ctrl+Y", action: () => editor1?.trigger("menu", "redo", null) },
      { type: "separator" },
      { label: "切り取り", shortcut: "Ctrl+X", action: () => document.execCommand("cut") },
      { label: "コピー", shortcut: "Ctrl+C", action: () => document.execCommand("copy") },
      {
        label: "形式を指定してコピー",
        submenu: [
          { label: "構文の強調表示を付けてコピー", action: () => document.execCommand("copy") },
        ],
      },
      { label: "貼り付け", shortcut: "Ctrl+V", action: () => document.execCommand("paste") },
      { type: "separator" },
      { label: "検索", shortcut: "Ctrl+F", action: () => editor1?.trigger("menu", "actions.find", null) },
      { label: "置換", shortcut: "Ctrl+H", action: () => editor1?.trigger("menu", "editor.action.startFindReplaceAction", null) },
      { type: "separator" },
      { label: "フォルダーを指定して検索", shortcut: "Ctrl+Shift+F", action: () => document.querySelector<HTMLButtonElement>('[data-view="search"]')?.click() },
      { label: "複数のファイルで置換", shortcut: "Ctrl+Shift+H", action: () => document.querySelector<HTMLButtonElement>('[data-view="search"]')?.click() },
      { type: "separator" },
      { label: "行コメントの切り替え", shortcut: "Ctrl+/", action: () => editor1?.trigger("menu", "editor.action.commentLine", null) },
      { label: "ブロック コメントの切り替え", shortcut: "Shift+Alt+A", action: () => editor1?.trigger("menu", "editor.action.blockComment", null) },
      { label: "Emmet: 省略記法を展開", shortcut: "Tab", action: () => editor1?.trigger("menu", "editor.action.triggerSuggest", null) },
    ],

    selection: [
      { label: "すべて選択", shortcut: "Ctrl+A", action: () => editor1?.trigger("menu", "editor.action.selectAll", null) },
      { label: "行を上にコピー", shortcut: "Alt+Shift+Up", action: () => editor1?.trigger("menu", "editor.action.copyLinesUpAction", null) },
      { label: "行を下にコピー", shortcut: "Alt+Shift+Down", action: () => editor1?.trigger("menu", "editor.action.copyLinesDownAction", null) },
      { label: "行を上に移動", shortcut: "Alt+Up", action: () => editor1?.trigger("menu", "editor.action.moveCarretUpAction", null) },
      { label: "行を下に移動", shortcut: "Alt+Down", action: () => editor1?.trigger("menu", "editor.action.moveCarretDownAction", null) },
    ],

    view: [
      { label: "コマンド パレット...", shortcut: "Ctrl+Shift+P", action: () => openQuickPick(true) },
      { label: "クイック オープン...", shortcut: "Ctrl+P", action: () => openQuickPick(false) },
      { type: "separator" },
      { label: "エクスプローラー", shortcut: "Ctrl+Shift+E", action: () => document.querySelector<HTMLButtonElement>('[data-view="explorer"]')?.click() },
      { label: "検索", shortcut: "Ctrl+Shift+F", action: () => document.querySelector<HTMLButtonElement>('[data-view="search"]')?.click() },
      { label: "ソース管理", shortcut: "Ctrl+Shift+G", action: () => document.querySelector<HTMLButtonElement>('[data-view="scm"]')?.click() },
      { label: "拡張機能", shortcut: "Ctrl+Shift+X", action: () => document.querySelector<HTMLButtonElement>('[data-view="extensions"]')?.click() },
      { type: "separator" },
      { label: "ターミナル", shortcut: "Ctrl+J", action: () => toggleTerminal() },
      { label: "プライマリ サイドバーの切り替え", shortcut: "Ctrl+B", action: () => toggleSidebar() },
      { type: "separator" },
      { label: "エディターを右に分割", shortcut: "◫", action: () => document.getElementById("btn-split-right")?.click() },
      { label: "エディターを下に分割", shortcut: "⬒", action: () => document.getElementById("btn-split-down")?.click() },
    ],

    go: [
      { label: "戻る", shortcut: "Alt+Left", action: () => editor1?.trigger("menu", "workbench.action.navigateBack", null) },
      { label: "進む", shortcut: "Alt+Right", action: () => editor1?.trigger("menu", "workbench.action.navigateForward", null) },
      { type: "separator" },
      { label: "ファイルへ移動...", shortcut: "Ctrl+P", action: () => openQuickPick(false) },
      { label: "定義へ移動 (Go to Definition)", shortcut: "F12", action: () => performGoToDefinition() },
      { label: "定義をここに表示 (Peek Definition)", shortcut: "Alt+F12", action: () => editor1?.trigger("menu", "editor.action.peekDefinition", null) },
      { label: "参照へ移動 (Go to References)", shortcut: "Shift+F12", action: () => editor1?.trigger("menu", "editor.action.referenceSearch.trigger", null) },
      { label: "行/列へ移動...", shortcut: "Ctrl+G", action: () => editor1?.trigger("menu", "editor.action.gotoLine", null) },
      { label: "記号へ移動...", shortcut: "Ctrl+Shift+O", action: () => openQuickPick(true) },
    ],

    run: [
      { label: "デバッグの開始", shortcut: "F5", action: () => showStatusMessage("デバッガーを起動中...") },
      { label: "デバッグなしで実行", shortcut: "Ctrl+F5", action: () => showStatusMessage("プログラムを実行中...") },
    ],

    terminal: [
      { label: "新しいターミナル", shortcut: "Ctrl+Shift+`", action: () => toggleTerminal(true) },
      { label: "ターミナルをクリア", action: () => xtermInstance?.clear() },
      { label: "ターミナル パネルの切り替え", shortcut: "Ctrl+J", action: () => toggleTerminal() },
    ],

    help: [
      { label: "へようこそ", action: () => openFile("welcome.rs", "welcome.rs") },
      { label: "ドキュメント", action: () => window.open("https://github.com/applyuser160/editor#readme", "_blank") },
      { label: "キーボード ショートカット", shortcut: "Ctrl+K Ctrl+S", action: () => openQuickPick(true) },
      { type: "separator" },
      { label: "GitHub リポジトリを開く", action: () => window.open("https://github.com/applyuser160/editor", "_blank") },
      { type: "separator" },
      { label: "Oxide Editor について", action: () => alert("🦀 Oxide Editor v0.1.0\nMicrosoft VS Code on Tauri v2 Architecture\nUltra-fast & Lightweight Native Rust IDE") },
    ],
  };

  const menuButtons = document.querySelectorAll<HTMLButtonElement>("#top-menu-bar .menu-btn");
  const dropdownEl = document.getElementById("global-menu-dropdown");
  if (!dropdownEl) return;

  function closeMenu() {
    isTopMenuOpen = false;
    currentOpenMenuKey = null;
    dropdownEl?.classList.add("hidden");
    menuButtons.forEach((b) => b.classList.remove("active"));
    if (activeSubmenuEl) {
      activeSubmenuEl.remove();
      activeSubmenuEl = null;
    }
  }

  function renderMenuLevel(items: MenuItemDef[], container: HTMLElement) {
    container.innerHTML = "";
    items.forEach((item) => {
      if (item.type === "separator") {
        const sep = document.createElement("div");
        sep.className = "menu-dropdown-separator";
        container.appendChild(sep);
      } else {
        const row = document.createElement("div");
        row.className = `menu-dropdown-item ${item.disabled ? "disabled" : ""}`;

        const hasSub = Boolean(item.submenu && item.submenu.length > 0);
        row.innerHTML = `
          <span class="item-label">${item.label || ""}</span>
          <div class="item-right" style="display: flex; align-items: center;">
            ${item.shortcut ? `<span class="item-shortcut">${item.shortcut}</span>` : ""}
            ${hasSub ? `<span class="item-arrow">›</span>` : ""}
          </div>
        `;

        if (hasSub) {
          row.addEventListener("mouseenter", () => {
            if (activeSubmenuEl) activeSubmenuEl.remove();

            const subContainer = document.createElement("div");
            subContainer.className = "vs-dropdown";
            const rect = row.getBoundingClientRect();
            subContainer.style.left = `${rect.right + 2}px`;
            subContainer.style.top = `${rect.top - 4}px`;
            renderMenuLevel(item.submenu!, subContainer);
            document.body.appendChild(subContainer);
            activeSubmenuEl = subContainer;
          });
        } else {
          row.addEventListener("mouseenter", () => {
            if (activeSubmenuEl) {
              activeSubmenuEl.remove();
              activeSubmenuEl = null;
            }
          });
        }

        row.onclick = (e) => {
          e.stopPropagation();
          if (!hasSub) {
            closeGlobalMenu();
            if (item.action && !item.disabled) {
              item.action();
            }
          }
        };

        container.appendChild(row);
      }
    });
  }

  function openMenu(menuKey: string, btnEl: HTMLButtonElement) {
    const items = menuDefs[menuKey];
    if (!items || !dropdownEl) return;

    isTopMenuOpen = true;
    currentOpenMenuKey = menuKey;
    menuButtons.forEach((b) => b.classList.remove("active"));
    btnEl.classList.add("active");

    if (activeSubmenuEl) {
      activeSubmenuEl.remove();
      activeSubmenuEl = null;
    }

    const rect = btnEl.getBoundingClientRect();
    dropdownEl.className = "vs-dropdown";
    dropdownEl.style.left = `${rect.left}px`;
    dropdownEl.style.top = `${rect.bottom}px`;
    renderMenuLevel(items, dropdownEl);

    dropdownEl.classList.remove("hidden");
  }

  menuButtons.forEach((btn) => {
    const key = btn.getAttribute("data-menu");
    if (!key) return;

    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      if (isTopMenuOpen && currentOpenMenuKey === key) {
        closeGlobalMenu();
      } else {
        openMenu(key, btn);
      }
    });

    btn.addEventListener("mouseenter", () => {
      if (isTopMenuOpen && currentOpenMenuKey !== key) {
        openMenu(key, btn);
      }
    });
  });

  // Capture-phase pointerdown to dismiss menus when clicking anywhere outside
  document.addEventListener(
    "pointerdown",
    (e) => {
      if (!isTopMenuOpen) return;
      const target = e.target as HTMLElement | null;
      if (target?.closest("#global-menu-dropdown") || target?.closest(".vs-dropdown") || target?.closest("#top-menu-bar")) {
        return;
      }
      closeGlobalMenu();
    },
    true
  );

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && isTopMenuOpen) {
      closeGlobalMenu();
    }
  });
}

function closeGlobalMenu() {
  isTopMenuOpen = false;
  currentOpenMenuKey = null;
  const dropdownEl = document.getElementById("global-menu-dropdown");
  dropdownEl?.classList.add("hidden");
  document.querySelectorAll<HTMLButtonElement>("#top-menu-bar .menu-btn").forEach((b) => b.classList.remove("active"));
  if (activeSubmenuEl) {
    activeSubmenuEl.remove();
    activeSubmenuEl = null;
  }
}

function toggleTerminal(forceOpen?: boolean) {
  const panelPart = document.getElementById("panel-part");
  if (panelPart) {
    isTerminalVisible = forceOpen !== undefined ? forceOpen : !isTerminalVisible;
    panelPart.style.display = isTerminalVisible ? "flex" : "none";
    editor1?.layout();
    editor2?.layout();
    fitAddon?.fit();
  }
}

// 4. Status Bar Git Branch Switcher (Click on 🌿 main)
function setupBranchSwitcher() {
  const branchEl = document.getElementById("status-branch");
  if (!branchEl) return;

  branchEl.addEventListener("click", async () => {
    try {
      const branches = await invoke<string[]>("git_list_branches");
      const modal = document.getElementById("quickpick-modal");
      const input = document.getElementById("quickpick-input") as HTMLInputElement;
      if (!modal || !input) return;

      modal.classList.remove("hidden");
      input.value = "";
      input.placeholder = "切り替えるブランチを選択、または新しいブランチ名を入力...";
      input.focus();

      quickPickItems = [
        {
          id: "new_branch",
          title: "➕ 新しいブランチを作成して切り替え (Create new branch...)",
          action: async () => {
            const newBranchName = prompt("新しいブランチ名を入力してください:");
            if (newBranchName) {
              try {
                const res = await invoke<string>("git_create_branch", { newBranch: newBranchName });
                showStatusMessage(res);
                updateGitStatus();
              } catch (e) {
                alert(`ブランチ作成失敗: ${e}`);
              }
            }
          },
        },
      ];

      branches.forEach((b) => {
        quickPickItems.push({
          id: b,
          title: `🌿 ${b}`,
          action: async () => {
            try {
              const res = await invoke<string>("git_checkout_branch", { branch: b });
              showStatusMessage(res);
              updateGitStatus();
            } catch (e) {
              alert(`ブランチ切替失敗: ${e}`);
            }
          },
        });
      });

      renderQuickPickDom();
    } catch (e) {
      alert(`ブランチ一覧取得エラー: ${e}`);
    }
  });
}

async function updateGitStatus() {
  try {
    const status = await invoke<GitStatusResult>("git_get_status");
    const branchEl = document.getElementById("status-branch");
    if (branchEl) {
      branchEl.textContent = `🌿 ${status.branch}`;
    }
    if (currentActiveView === "scm") {
      const contentEl = document.getElementById("sidebar-content");
      if (contentEl) renderScmView(contentEl);
    }
  } catch (e) {
    console.error(e);
  }
}

// 5. 2D Grid Splitter Actions
function setupGridSplitters() {
  const btnSplitRight = document.getElementById("btn-split-right");
  const btnSplitDown = document.getElementById("btn-split-down");
  const btnCloseSplit = document.getElementById("btn-close-split");
  const pane2 = document.getElementById("editor-pane-2");
  const gridResizer = document.getElementById("grid-resizer");
  const editorGrid = document.getElementById("editor-grid");

  if (btnSplitRight && pane2 && gridResizer && editorGrid) {
    btnSplitRight.addEventListener("click", () => {
      isSplitActive = true;
      splitOrientation = "horizontal";
      editorGrid.style.flexDirection = "row";
      gridResizer.className = "resizer horizontal";
      pane2.classList.remove("hidden");
      gridResizer.classList.remove("hidden");
      editor1?.layout();
      editor2?.layout();
      showStatusMessage("エディターを左右に分割しました");
    });
  }

  if (btnSplitDown && pane2 && gridResizer && editorGrid) {
    btnSplitDown.addEventListener("click", () => {
      isSplitActive = true;
      splitOrientation = "vertical";
      editorGrid.style.flexDirection = "column";
      gridResizer.className = "resizer vertical";
      pane2.classList.remove("hidden");
      gridResizer.classList.remove("hidden");
      editor1?.layout();
      editor2?.layout();
      showStatusMessage("エディターを上下に分割しました");
    });
  }

  if (btnCloseSplit && pane2 && gridResizer) {
    btnCloseSplit.addEventListener("click", () => {
      isSplitActive = false;
      pane2.classList.add("hidden");
      gridResizer.classList.add("hidden");
      editor1?.layout();
      showStatusMessage("エディター分割を閉じました");
    });
  }
}

// 6. Integrated Real-time Terminal (xterm.js + Portable-PTY)
async function setupIntegratedTerminal() {
  const container = document.getElementById("terminal-container");
  if (!container) return;

  container.innerHTML = "";

  xtermInstance = new Terminal({
    theme: {
      background: "#181818",
      foreground: "#cccccc",
      cursor: "#007acc",
      selectionBackground: "#264f78",
      black: "#000000",
      red: "#cd3131",
      green: "#0dbc79",
      yellow: "#e5e510",
      blue: "#2472c8",
      magenta: "#bc3fbc",
      cyan: "#11a8cd",
      white: "#e5e5e5",
    },
    fontFamily: "Consolas, 'Courier New', monospace",
    fontSize: 13,
    lineHeight: 1.2,
    cursorBlink: true,
  });

  fitAddon = new FitAddon();
  xtermInstance.loadAddon(fitAddon);
  xtermInstance.open(container);

  setTimeout(() => {
    fitAddon?.fit();
  }, 100);

  try {
    const cols = xtermInstance.cols || 80;
    const rows = xtermInstance.rows || 24;

    ptyId = await invoke<number>("spawn_pty", { cols, rows });

    await listen<string>(`pty-data-${ptyId}`, (event) => {
      xtermInstance?.write(event.payload);
    });

    xtermInstance.onData((data) => {
      if (ptyId !== null) {
        invoke("write_pty", { id: ptyId, data });
      }
    });

    xtermInstance.onResize((size) => {
      if (ptyId !== null) {
        invoke("resize_pty", { id: ptyId, cols: size.cols, rows: size.rows });
      }
    });

    window.addEventListener("resize", () => {
      fitAddon?.fit();
    });
  } catch (err) {
    xtermInstance.writeln(`\x1b[31mFailed to spawn PTY: ${err}\x1b[0m`);
  }
}

// 7. File Watcher Real-time Sync
async function setupFileWatcherListener() {
  await listen<{ paths: string[]; kind: string }>("fs-change", async (event) => {
    if (currentActiveView === "explorer") {
      loadWorkspaceFiles();
    } else if (currentActiveView === "scm") {
      const contentEl = document.getElementById("sidebar-content");
      if (contentEl) renderScmView(contentEl);
    }

    if (activeFilePath && event.payload.paths.some((p) => p.endsWith(activeFilePath!))) {
      const tab = openTabs.get(activeFilePath);
      if (tab && !tab.isDirty) {
        try {
          const freshContent = await invoke<string>("read_file_content", { path: activeFilePath });
          tab.model.setValue(freshContent);
          showStatusMessage(`外部変更を反映: ${tab.name}`);
        } catch (e) {
          console.error(e);
        }
      }
    }
  });
}

// 8. Extension Host Initialization
async function initExtensionHost() {
  try {
    const statusMsg = await invoke<string>("start_extension_sidecar");
    const statusEl = document.getElementById("window-status");
    if (statusEl) {
      statusEl.textContent = statusMsg.includes("Node.js") ? "Extension Host (Node.js) Ready" : "Native & WASM Runtime";
    }
  } catch (e) {
    console.error("Extension host init error:", e);
  }
}

// 9. Open / Switch Files
async function openFile(path: string, name: string) {
  if (!editor1) return;

  if (openTabs.has(path)) {
    activeFilePath = path;
    const tab = openTabs.get(path)!;
    editor1.setModel(tab.model);
    if (isSplitActive && editor2) {
      editor2.setModel(tab.model);
    }
    updateTabBar();
    updateStatusBar(path);
    return;
  }

  try {
    const content = await invoke<string>("read_file_content", { path });
    const language = getLanguageFromPath(path);
    const model = monaco.editor.createModel(content, language);

    model.onDidChangeContent(() => {
      if (openTabs.has(path)) {
        const tab = openTabs.get(path)!;
        tab.isDirty = true;
        updateTabBar();

        // Notify LSP of change
        invoke("lsp_send_notification", {
          lang: language,
          method: "textDocument/didChange",
          params: {
            textDocument: { uri: `file:///${path}`, version: 1 },
            contentChanges: [{ text: model.getValue() }],
          },
        }).catch(() => {});
      }
    });

    openTabs.set(path, { path, name, model, isDirty: false });
    activeFilePath = path;
    editor1.setModel(model);
    if (isSplitActive && editor2) {
      editor2.setModel(model);
    }

    // Ensure LSP is running and send didOpen
    ensureLspServerStarted(language);
    invoke("lsp_send_notification", {
      lang: language,
      method: "textDocument/didOpen",
      params: {
        textDocument: {
          uri: `file:///${path}`,
          languageId: language,
          version: 1,
          text: content,
        },
      },
    }).catch(() => {});

    updateTabBar();
    updateStatusBar(path);
    showStatusMessage(`開きました: ${name}`);
  } catch (err) {
    showStatusMessage(`エラー: ファイルを開けませんでした (${err})`);
  }
}

// 10. Save Active File (Ctrl+S)
async function saveActiveFile() {
  if (!activeFilePath || !editor1) return;
  const tab = openTabs.get(activeFilePath);
  if (!tab) return;

  const content = tab.model.getValue();
  const language = getLanguageFromPath(tab.path);
  try {
    await invoke("write_file_content", { path: tab.path, content });
    tab.isDirty = false;
    updateTabBar();
    showStatusMessage(`保存完了: ${tab.name}`);

    // Notify LSP of save
    invoke("lsp_send_notification", {
      lang: language,
      method: "textDocument/didSave",
      params: {
        textDocument: { uri: `file:///${tab.path}` },
        text: content,
      },
    }).catch(() => {});
  } catch (err) {
    showStatusMessage(`保存失敗: ${err}`);
  }
}

// 11. Tab Bar Rendering
function updateTabBar() {
  const tabBar = document.getElementById("tab-bar");
  if (!tabBar) return;

  tabBar.innerHTML = "";

  openTabs.forEach((tab, path) => {
    const tabEl = document.createElement("div");
    tabEl.className = `tab ${path === activeFilePath ? "active" : ""}`;

    const titleEl = document.createElement("span");
    titleEl.className = "tab-title";
    titleEl.textContent = `${tab.name}${tab.isDirty ? " ●" : ""}`;
    tabEl.appendChild(titleEl);

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.onclick = (e) => {
      e.stopPropagation();
      closeTab(path);
    };
    tabEl.appendChild(closeBtn);

    tabEl.onclick = () => openFile(tab.path, tab.name);
    tabBar.appendChild(tabEl);
  });
}

function closeTab(path: string) {
  if (!openTabs.has(path)) return;

  const tab = openTabs.get(path)!;
  tab.model.dispose();
  openTabs.delete(path);

  if (activeFilePath === path) {
    const remainingKeys = Array.from(openTabs.keys());
    if (remainingKeys.length > 0) {
      const nextPath = remainingKeys[remainingKeys.length - 1];
      openFile(nextPath, openTabs.get(nextPath)!.name);
    } else {
      activeFilePath = null;
      if (editor1) {
        const emptyModel = monaco.editor.createModel("", "plaintext");
        editor1.setModel(emptyModel);
      }
    }
  }
  updateTabBar();
}

function getLanguageFromPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "rs": return "rust";
    case "ts": return "typescript";
    case "js": return "javascript";
    case "json": return "json";
    case "md": return "markdown";
    case "toml": return "ini";
    case "html": return "html";
    case "css": return "css";
    case "py": return "python";
    case "go": return "go";
    case "sh": return "shell";
    default: return "plaintext";
  }
}

// 12. Activity Bar & All Sidebar Views
function setupActivityBar() {
  const buttons = document.querySelectorAll<HTMLButtonElement>(".activity-btn");
  buttons.forEach((btn) => {
    btn.addEventListener("click", () => {
      const view = btn.getAttribute("data-view");
      if (!view) return;

      if (currentActiveView === view) {
        toggleSidebar();
      } else {
        currentActiveView = view;
        buttons.forEach((b) => b.classList.remove("active"));
        btn.classList.add("active");
        if (!isSidebarVisible) toggleSidebar(true);
        updateSidebarView(view);
      }
    });
  });
}

function toggleSidebar(forceOpen?: boolean) {
  const sidebar = document.getElementById("sidebar");
  if (!sidebar) return;

  isSidebarVisible = forceOpen !== undefined ? forceOpen : !isSidebarVisible;
  sidebar.style.display = isSidebarVisible ? "flex" : "none";
  editor1?.layout();
  editor2?.layout();
  fitAddon?.fit();
}

async function updateSidebarView(view: string) {
  const titleEl = document.getElementById("sidebar-title");
  const contentEl = document.getElementById("sidebar-content");
  if (!titleEl || !contentEl) return;

  switch (view) {
    case "explorer":
      titleEl.textContent = "エクスプローラー";
      loadWorkspaceFiles();
      break;

    case "search":
      titleEl.textContent = "検索 (SEARCH)";
      contentEl.innerHTML = `
        <div style="padding: 4px;">
          <input type="text" id="global-search-input" placeholder="検索語を入力して Enter..." style="width: 100%; padding: 6px 8px; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px; font-size: 12px;" />
          <div id="search-results-list" style="margin-top: 8px; max-height: calc(100vh - 160px); overflow-y: auto;"></div>
        </div>
      `;
      setupSearchInput();
      break;

    case "scm":
      titleEl.textContent = "ソース管理 (GIT)";
      renderScmView(contentEl);
      break;

    case "extensions":
      titleEl.textContent = "拡張機能 (OPEN VSX)";
      renderExtensionsView(contentEl);
      break;

    case "settings":
      titleEl.textContent = "設定 (SETTINGS)";
      contentEl.innerHTML = `
        <div style="padding: 8px; font-size: 12px; display: flex; flex-direction: column; gap: 12px;">
          <div>
            <label style="display: block; margin-bottom: 4px; color: #aaa;">カラーテーマ (Theme):</label>
            <select id="theme-selector" style="width: 100%; padding: 4px; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px;">
              <option value="vscode-dark-plus">VS Code Dark+</option>
              <option value="vs">VS Code Light</option>
              <option value="hc-black">High Contrast</option>
            </select>
          </div>
          <div>
            <label style="display: block; margin-bottom: 4px; color: #aaa;">フォントサイズ (Font Size):</label>
            <input type="number" id="font-size-input" value="14" min="10" max="28" style="width: 100%; padding: 4px; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px;" />
          </div>
          <div>
            <label style="display: block; margin-bottom: 4px; color: #aaa;">タブサイズ (Tab Size):</label>
            <input type="number" id="tab-size-input" value="4" min="2" max="8" style="width: 100%; padding: 4px; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px;" />
          </div>
        </div>
      `;
      setupSettingsHandlers();
      break;
  }
}

// 13. Open VSX Marketplace Extensions Viewlet
async function renderExtensionsView(container: HTMLElement) {
  container.innerHTML = `
    <div style="padding: 4px; display: flex; flex-direction: column; height: 100%;">
      <input type="text" id="openvsx-search-input" placeholder="Open VSX マーケットプレイスを検索 (例: rust, theme, python)..." style="width: 100%; padding: 6px 8px; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px; font-size: 12px; margin-bottom: 8px;" />
      <div id="installed-extensions-header" style="font-size: 11px; font-weight: bold; color: #aaa; margin: 4px 0;">📦 インストール済み (Installed)</div>
      <div id="installed-ext-list"></div>
      <div id="marketplace-extensions-header" style="font-size: 11px; font-weight: bold; color: #aaa; margin: 8px 0 4px 0;">🌐 Open VSX マーケットプレイス (Popular)</div>
      <div id="openvsx-ext-list" style="flex: 1; overflow-y: auto;"></div>
    </div>
  `;

  const installedList = document.getElementById("installed-ext-list");
  const marketplaceList = document.getElementById("openvsx-ext-list");
  const searchInput = document.getElementById("openvsx-search-input") as HTMLInputElement;

  if (installedList) {
    try {
      const exts = await invoke<ExtensionManifest[]>("get_installed_extensions");
      installedList.innerHTML = "";
      exts.forEach((ext) => {
        const card = document.createElement("div");
        card.className = "openvsx-ext-card";
        card.innerHTML = `
          <div class="openvsx-ext-header">
            <span class="openvsx-ext-title">${ext.name}</span>
            <span class="openvsx-ext-id">v${ext.version}</span>
          </div>
          <div class="openvsx-ext-desc">${ext.description}</div>
          <div class="openvsx-ext-footer">
            <span style="font-size: 10px; color: #00ff80;">● 有効 (Active)</span>
          </div>
        `;
        installedList.appendChild(card);
      });
    } catch (e) {
      installedList.innerHTML = `<div style="color: #888; font-size: 11px;">読込エラー: ${e}</div>`;
    }
  }

  async function searchMarketplace(query: string) {
    if (!marketplaceList) return;
    marketplaceList.innerHTML = `<div style="color: #888; font-size: 11px; padding: 4px;">Open VSX を検索中...</div>`;

    try {
      const results = await invoke<OpenVsxExtension[]>("search_openvsx_extensions", { query });
      marketplaceList.innerHTML = "";

      if (results.length === 0) {
        marketplaceList.innerHTML = `<div style="color: #888; font-size: 11px; padding: 4px;">一致する拡張機能は見つかりませんでした</div>`;
        return;
      }

      results.forEach((ext) => {
        const card = document.createElement("div");
        card.className = "openvsx-ext-card";
        const title = ext.display_name || ext.name;
        const id = `${ext.namespace}.${ext.name}`;
        const downloads = ext.download_count ? `${ext.download_count.toLocaleString()} DL` : "";

        card.innerHTML = `
          <div class="openvsx-ext-header">
            <span class="openvsx-ext-title">${title}</span>
            <span class="openvsx-ext-id">${id}</span>
          </div>
          <div class="openvsx-ext-desc">${ext.description || "No description provided."}</div>
          <div class="openvsx-ext-footer">
            <span class="openvsx-ext-downloads">📥 ${downloads} (v${ext.version})</span>
            <button class="btn-install-ext" data-id="${id}">インストール</button>
          </div>
        `;

        const btn = card.querySelector<HTMLButtonElement>(".btn-install-ext");
        if (btn) {
          btn.addEventListener("click", async () => {
            btn.textContent = "インストール中...";
            btn.disabled = true;
            try {
              const res = await invoke<string>("install_openvsx_extension", {
                namespace: ext.namespace,
                name: ext.name,
                version: ext.version,
                description: ext.description || "",
              });
              showStatusMessage(res);
              btn.textContent = "✓ インストール済み";
              btn.style.backgroundColor = "#2ea043";
            } catch (err) {
              alert(`インストール失敗: ${err}`);
              btn.textContent = "インストール";
              btn.disabled = false;
            }
          });
        }

        marketplaceList.appendChild(card);
      });
    } catch (err) {
      marketplaceList.innerHTML = `<div style="color: #ff5555; font-size: 11px;">Open VSX 接続エラー: ${err}</div>`;
    }
  }

  searchMarketplace("");

  if (searchInput) {
    let timeout: any = null;
    searchInput.addEventListener("input", () => {
      clearTimeout(timeout);
      timeout = setTimeout(() => {
        searchMarketplace(searchInput.value);
      }, 400);
    });
  }
}

// 14. Search Feature Integration
function setupSearchInput() {
  const input = document.getElementById("global-search-input") as HTMLInputElement;
  const list = document.getElementById("search-results-list");
  if (!input || !list) return;

  input.addEventListener("keydown", async (e) => {
    if (e.key === "Enter") {
      const q = input.value.trim();
      if (!q) return;

      list.innerHTML = `<div style="color: #888; font-size: 11px;">検索中...</div>`;
      try {
        const matches = await invoke<SearchMatch[]>("search_in_workspace", { query: q, caseSensitive: false });
        list.innerHTML = "";

        if (matches.length === 0) {
          list.innerHTML = `<div style="color: #888; font-size: 11px; padding: 4px;">一致する結果は見つかりませんでした</div>`;
          return;
        }

        matches.forEach((m) => {
          const item = document.createElement("div");
          item.className = "search-result-item";
          item.innerHTML = `
            <div class="search-file-title">📄 ${m.file_path}:${m.line_number}</div>
            <div class="search-match-line">${escapeHtml(m.line_text)}</div>
          `;
          item.onclick = async () => {
            await openFile(m.file_path, m.file_path.split("/").pop() || m.file_path);
            if (editor1) {
              editor1.revealLineInCenter(m.line_number);
              editor1.setPosition({ lineNumber: m.line_number, column: 1 });
            }
          };
          list.appendChild(item);
        });
      } catch (err) {
        list.innerHTML = `<div style="color: #ff5555; font-size: 11px;">検索エラー: ${err}</div>`;
      }
    }
  });
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

// 15. SCM (Git) Integration
async function renderScmView(container: HTMLElement) {
  try {
    const status = await invoke<GitStatusResult>("git_get_status");
    const branchEl = document.getElementById("status-branch");
    if (branchEl) {
      branchEl.textContent = `🌿 ${status.branch}`;
    }

    container.innerHTML = `
      <div style="padding: 4px;">
        <div style="font-size: 11px; color: #888; margin-bottom: 4px;">ブランチ: <strong style="color: #9cdcfe;">${status.branch}</strong></div>
        <textarea id="git-commit-msg" rows="2" placeholder="コミットメッセージを入力..." style="width: 100%; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px; padding: 4px; font-size: 12px;"></textarea>
        <button id="btn-commit" style="margin-top: 6px; width: 100%; padding: 6px; background: #007acc; border: none; color: #fff; border-radius: 4px; cursor: pointer; font-size: 12px;">✔ コミット実行 (Commit)</button>
        <div style="margin-top: 12px; font-size: 11px; font-weight: bold; color: #aaa;">変更されたファイル (${status.changed_files.length}):</div>
        <div id="scm-files-list" style="margin-top: 6px;"></div>
      </div>
    `;

    const list = document.getElementById("scm-files-list");
    if (list) {
      status.changed_files.forEach((f) => {
        const row = document.createElement("div");
        row.className = "scm-file-row";
        row.innerHTML = `
          <span>📄 ${f.trim()}</span>
          <span class="scm-status-tag modified">MODIFIED</span>
        `;
        list.appendChild(row);
      });
    }

    const btnCommit = document.getElementById("btn-commit");
    const commitInput = document.getElementById("git-commit-msg") as HTMLTextAreaElement;
    if (btnCommit && commitInput) {
      btnCommit.onclick = async () => {
        const msg = commitInput.value.trim();
        if (!msg) {
          alert("コミットメッセージを入力してください");
          return;
        }
        try {
          await invoke<string>("git_commit", { message: msg });
          showStatusMessage("Git コミット完了");
          updateSidebarView("scm");
        } catch (err) {
          alert(`コミット失敗: ${err}`);
        }
      };
    }
  } catch (err) {
    container.innerHTML = `<div style="color: #888; padding: 8px;">Git 状態取得エラー: ${err}</div>`;
  }
}

// 16. Settings Handlers
function setupSettingsHandlers() {
  const themeSel = document.getElementById("theme-selector") as HTMLSelectElement;
  const fontSizeInput = document.getElementById("font-size-input") as HTMLInputElement;

  if (themeSel) {
    themeSel.onchange = () => {
      monaco.editor.setTheme(themeSel.value);
    };
  }

  if (fontSizeInput) {
    fontSizeInput.onchange = () => {
      const sz = parseInt(fontSizeInput.value, 10);
      if (sz >= 10 && sz <= 28) {
        editor1?.updateOptions({ fontSize: sz });
        editor2?.updateOptions({ fontSize: sz });
      }
    };
  }
}

// 17. Workspace File Tree Loading & Actions
async function loadWorkspaceFiles() {
  const contentEl = document.getElementById("sidebar-content");
  if (!contentEl) return;

  try {
    const files = await invoke<FileEntry[]>("list_workspace_files");
    contentEl.innerHTML = "";

    files.forEach((file) => {
      const node = document.createElement("div");
      node.className = "tree-node";
      node.style.paddingLeft = `${file.depth * 12 + 8}px`;

      const icon = file.is_dir ? "📁" : "📄";
      node.innerHTML = `
        <div class="tree-node-left">
          <span>${icon}</span>
          <span>${file.name}</span>
        </div>
        <div class="tree-node-actions">
          <button class="node-btn btn-del" title="削除">🗑</button>
        </div>
      `;

      const delBtn = node.querySelector(".btn-del");
      if (delBtn) {
        delBtn.addEventListener("click", async (e) => {
          e.stopPropagation();
          if (confirm(`'${file.name}' を削除してよろしいですか？`)) {
            try {
              await invoke("delete_file", { path: file.path });
              loadWorkspaceFiles();
            } catch (err) {
              alert(`削除エラー: ${err}`);
            }
          }
        });
      }

      if (!file.is_dir) {
        node.addEventListener("click", () => openFile(file.path, file.name));
      }

      contentEl.appendChild(node);
    });
  } catch (e) {
    contentEl.innerHTML = `<div style="color: #888; padding: 8px;">ワークスペース読込中...</div>`;
  }
}

function setupFileActions() {
  const btnNewFile = document.getElementById("btn-new-file");
  const btnNewFolder = document.getElementById("btn-new-folder");
  const btnRefresh = document.getElementById("btn-refresh-tree");

  if (btnNewFile) {
    btnNewFile.addEventListener("click", async () => {
      const filename = prompt("新規ファイル名を入力してください:");
      if (!filename) return;
      try {
        await invoke("create_file", { path: filename });
        await loadWorkspaceFiles();
        openFile(filename, filename);
      } catch (err) {
        alert(`ファイル作成エラー: ${err}`);
      }
    });
  }

  if (btnNewFolder) {
    btnNewFolder.addEventListener("click", async () => {
      const foldername = prompt("新規フォルダ名を入力してください:");
      if (!foldername) return;
      try {
        await invoke("create_directory", { path: foldername });
        await loadWorkspaceFiles();
      } catch (err) {
        alert(`フォルダ作成エラー: ${err}`);
      }
    });
  }

  if (btnRefresh) {
    btnRefresh.addEventListener("click", () => loadWorkspaceFiles());
  }
}

// 18. Draggable Splitter Resizers
function setupResizers() {
  const sidebarResizer = document.getElementById("sidebar-resizer");
  const sidebar = document.getElementById("sidebar");

  if (sidebarResizer && sidebar) {
    let isDragging = false;
    sidebarResizer.addEventListener("mousedown", (e) => {
      isDragging = true;
      sidebarResizer.classList.add("dragging");
      e.preventDefault();
    });

    window.addEventListener("mousemove", (e) => {
      if (!isDragging) return;
      const newWidth = Math.max(160, Math.min(e.clientX - 48, 600));
      sidebar.style.width = `${newWidth}px`;
      editor1?.layout();
      editor2?.layout();
      fitAddon?.fit();
    });

    window.addEventListener("mouseup", () => {
      if (isDragging) {
        isDragging = false;
        sidebarResizer.classList.remove("dragging");
      }
    });
  }

  const terminalResizer = document.getElementById("terminal-resizer");
  const panelPart = document.getElementById("panel-part");

  if (terminalResizer && panelPart) {
    let isDragging = false;
    terminalResizer.addEventListener("mousedown", (e) => {
      isDragging = true;
      terminalResizer.classList.add("dragging");
      e.preventDefault();
    });

    window.addEventListener("mousemove", (e) => {
      if (!isDragging) return;
      const newHeight = Math.max(80, Math.min(window.innerHeight - e.clientY - 22, window.innerHeight * 0.6));
      panelPart.style.height = `${newHeight}px`;
      editor1?.layout();
      editor2?.layout();
      fitAddon?.fit();
    });

    window.addEventListener("mouseup", () => {
      if (isDragging) {
        isDragging = false;
        terminalResizer.classList.remove("dragging");
      }
    });
  }

  const btnClosePanel = document.getElementById("btn-close-panel");
  if (btnClosePanel && panelPart) {
    btnClosePanel.addEventListener("click", () => {
      isTerminalVisible = false;
      panelPart.style.display = "none";
      editor1?.layout();
      editor2?.layout();
    });
  }
}

// 19. QuickPick Modal with Keyboard Navigation
function setupQuickPick() {
  const modal = document.getElementById("quickpick-modal");
  const input = document.getElementById("quickpick-input") as HTMLInputElement;
  const triggerBtn = document.getElementById("trigger-quickopen");

  if (!modal || !input) return;

  if (triggerBtn) {
    triggerBtn.addEventListener("click", () => openQuickPick(false));
  }

  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      modal.classList.add("hidden");
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (quickPickItems.length > 0) {
        quickPickSelectedIndex = (quickPickSelectedIndex + 1) % quickPickItems.length;
        renderQuickPickDom();
      }
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (quickPickItems.length > 0) {
        quickPickSelectedIndex = (quickPickSelectedIndex - 1 + quickPickItems.length) % quickPickItems.length;
        renderQuickPickDom();
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (quickPickItems[quickPickSelectedIndex]) {
        modal.classList.add("hidden");
        quickPickItems[quickPickSelectedIndex].action();
      }
    }
  });
}

function openQuickPick(isCommandMode: boolean) {
  closeGlobalMenu();
  const modal = document.getElementById("quickpick-modal");
  const input = document.getElementById("quickpick-input") as HTMLInputElement;
  if (!modal || !input) return;

  modal.classList.remove("hidden");
  input.value = isCommandMode ? "> " : "";
  input.placeholder = "ファイル名で検索 (または '>' でコマンド)...";
  input.focus();
  quickPickSelectedIndex = 0;
  fetchAndRenderQuickPick(input.value);

  input.oninput = () => {
    quickPickSelectedIndex = 0;
    fetchAndRenderQuickPick(input.value);
  };
}

async function fetchAndRenderQuickPick(query: string) {
  quickPickItems = [];

  const commands = [
    { title: "View: Split Editor Right (エディターを右に分割)", shortcut: "", id: "split_right" },
    { title: "View: Split Editor Down (エディターを下に分割)", shortcut: "", id: "split_down" },
    { title: "File: Save (ファイルの保存)", shortcut: "Ctrl+S", id: "save" },
    { title: "File: New File (新規ファイル作成)", shortcut: "Ctrl+N", id: "new_file" },
    { title: "View: Toggle Side Bar (サイドバー切替)", shortcut: "Ctrl+B", id: "toggle_sidebar" },
    { title: "View: Toggle Terminal (ターミナル切替)", shortcut: "Ctrl+J", id: "toggle_terminal" },
    { title: "Git: Open SCM View (ソース管理を開く)", shortcut: "Ctrl+Shift+G", id: "open_scm" },
    { title: "Git: Switch Branch (ブランチ切り替え)", shortcut: "", id: "switch_branch" },
  ];

  if (query.startsWith(">")) {
    const q = query.slice(1).trim().toLowerCase();
    commands
      .filter((c) => !q || c.title.toLowerCase().includes(q))
      .forEach((c) => {
        quickPickItems.push({
          id: c.id,
          title: c.title,
          shortcut: c.shortcut,
          action: () => executeCommand(c.id),
        });
      });
  } else {
    try {
      const files = await invoke<FileEntry[]>("list_workspace_files");
      const q = query.trim().toLowerCase();
      files
        .filter((f) => !f.is_dir && (!q || f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q)))
        .forEach((f) => {
          quickPickItems.push({
            id: f.path,
            title: `📄 ${f.name}`,
            subtitle: f.path,
            action: () => openFile(f.path, f.name),
          });
        });
    } catch (e) {
      console.error(e);
    }
  }

  renderQuickPickDom();
}

function renderQuickPickDom() {
  const list = document.getElementById("quickpick-list");
  if (!list) return;

  list.innerHTML = "";

  quickPickItems.forEach((item, idx) => {
    const el = document.createElement("div");
    el.className = `quickpick-item ${idx === quickPickSelectedIndex ? "selected" : ""}`;
    el.innerHTML = `
      <div>
        <span>${item.title}</span>
        ${item.subtitle ? `<span style="font-size: 11px; color: #888; margin-left: 8px;">${item.subtitle}</span>` : ""}
      </div>
      ${item.shortcut ? `<span style="color: #888; font-size: 11px;">${item.shortcut}</span>` : ""}
    `;
    el.onclick = () => {
      document.getElementById("quickpick-modal")?.classList.add("hidden");
      item.action();
    };
    list.appendChild(el);
  });
}

function executeCommand(id: string) {
  switch (id) {
    case "split_right":
      document.getElementById("btn-split-right")?.click();
      break;
    case "split_down":
      document.getElementById("btn-split-down")?.click();
      break;
    case "save":
      saveActiveFile();
      break;
    case "new_file":
      document.getElementById("btn-new-file")?.click();
      break;
    case "toggle_sidebar":
      toggleSidebar();
      break;
    case "toggle_terminal":
      toggleTerminal();
      break;
    case "open_scm":
      document.querySelector<HTMLButtonElement>('[data-view="scm"]')?.click();
      break;
    case "switch_branch":
      document.getElementById("status-branch")?.click();
      break;
  }
}

// 20. Global Shortcuts & Status Bar
function setupShortcuts() {
  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.key === "s") {
      e.preventDefault();
      saveActiveFile();
    } else if (e.ctrlKey && e.shiftKey && e.key === "P") {
      e.preventDefault();
      openQuickPick(true);
    } else if (e.ctrlKey && e.key === "p") {
      e.preventDefault();
      openQuickPick(false);
    } else if (e.ctrlKey && e.key === "b") {
      e.preventDefault();
      toggleSidebar();
    } else if (e.ctrlKey && e.key === "j") {
      e.preventDefault();
      toggleTerminal();
    }
  });
}

function updateStatusBar(path: string) {
  const langEl = document.getElementById("status-language");
  if (langEl) {
    langEl.textContent = getLanguageFromPath(path).toUpperCase();
  }
}

function showStatusMessage(msg: string) {
  const status = document.getElementById("global-status");
  if (status) {
    status.textContent = msg;
    setTimeout(() => {
      if (status.textContent === msg) {
        status.textContent = "準備完了";
      }
    }, 4000);
  }
}
