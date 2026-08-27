import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import * as monaco from "monaco-editor";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import {
  applyProfile,
  COMMAND_LABELS,
  commandForEvent,
  createProfile,
  deleteProfile,
  exportProfile,
  findKeybindingConflicts,
  getKeybindings,
  getProfiles,
  getScopedSettings,
  importProfile,
  keybindingFromEvent,
  migrateLegacySettings,
  resetKeybindings,
  resolveSettings,
  saveKeybindings,
  saveScopedSettings,
  type EditorSettings,
  type Keybinding,
  type SettingScope,
} from "./settings";

interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  depth: number;
}

interface WorkspaceInfo {
  root: string;
  name: string;
}

interface TaskDefinition {
  label: string;
  command: string;
  args: string[];
  is_background: boolean;
  cwd?: string | null;
  depends_on: string[];
}

interface TaskExecutionResult {
  label: string;
  exit_code: number | null;
  output: string;
  problems: Array<{ message: string; severity: string }>;
}

interface TestSuite {
  id: string;
  label: string;
  command: string;
  args: string[];
}

interface WorkspaceTrust {
  root: string;
  trusted: boolean;
}

interface WorkspaceExcludes {
  files: string[];
  search: string[];
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
  main?: string | null;
  activation_events?: string[];
  contributes_languages: string[];
  contributes_themes: string[];
  enabled: boolean;
}

interface OpenVsxExtension {
  namespace: string;
  name: string;
  version: string;
  display_name: string | null;
  description: string | null;
  download_count: number | null;
  icon_url: string | null;
  download_url: string | null;
  url: string | null;
}

interface OpenTab {
  path: string;
  name: string;
  model: monaco.editor.ITextModel;
  isDirty: boolean;
  version: number;
}

interface MenuItemDef {
  type?: "separator";
  label?: string;
  shortcut?: string;
  disabled?: boolean;
  submenu?: MenuItemDef[];
  action?: () => void;
}

interface TerminalSession {
  id: number;
  ptyId: number;
  title: string;
  terminal: Terminal;
  fitAddon: FitAddon;
  containerEl: HTMLElement;
}

// Global State
let workspaceRoot = "";
let workspaceFolders: WorkspaceInfo[] = [];
let workspaceTrust: WorkspaceTrust | null = null;
let editor1: monaco.editor.IStandaloneCodeEditor | null = null;
let editor2: monaco.editor.IStandaloneCodeEditor | null = null;
let activeEditorPane: 1 | 2 = 1;
let pane1FilePath: string | null = "welcome.rs";
let pane2FilePath: string | null = null;
let isSplitActive = false;
let splitOrientation: "horizontal" | "vertical" = "horizontal";

let terminalSessions: TerminalSession[] = [];
let activeTerminalSessionId: number | null = null;
let nextTerminalId = 1;

function getActiveTerminalSession(): TerminalSession | undefined {
  return terminalSessions.find((s) => s.id === activeTerminalSessionId) || terminalSessions[0];
}

let searchCaseSensitive = false;
let searchWholeWord = false;
let searchIsRegex = false;

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

// Path & URI Utilities
function normalizePath(rawPath: string): string {
  let p = rawPath.replace(/\\/g, "/");
  // If relative path and workspaceRoot is known, prepend workspaceRoot
  if (!p.match(/^[a-zA-Z]:\//) && !p.startsWith("/")) {
    if (workspaceRoot) {
      p = `${workspaceRoot.replace(/\\/g, "/")}/${p}`;
    }
  }
  // Remove duplicate slashes
  p = p.replace(/\/+/g, "/");
  return p;
}

function pathToUri(filePath: string): string {
  const normalized = normalizePath(filePath);
  if (normalized.startsWith("/")) {
    return `file://${normalized}`;
  }
  // Windows path like D:/... -> file:///D:/...
  return `file:///${normalized}`;
}

function uriToPath(uriStr: string): string {
  let path = uriStr;
  if (path.startsWith("file:///")) {
    path = path.substring(8);
  } else if (path.startsWith("file://")) {
    path = path.substring(7);
  }
  return normalizePath(path);
}

// Initialize when DOM is ready
window.addEventListener("DOMContentLoaded", async () => {
  try {
    workspaceRoot = await invoke<string>("get_workspace_path");
    await refreshWorkspaceState();
    updateWorkspaceDisplay({
      root: workspaceRoot,
      name: workspaceRoot.split(/[\\/]/).filter(Boolean).pop() || "workspace",
    });
    await confirmWorkspaceTrust();
  } catch (e) {
    console.warn("Failed to get workspace path:", e);
  }

  initLanguageServerIntegration();
  initMonacoEditors();
  migrateLegacySettings();
  applyStoredSettings();
  setupVSCodeMenus();
  setupActivityBar();
  setupResizers();
  setupGridSplitters();
  setupIntegratedTerminal();
  setupBranchSwitcher();
  setupStatusBarInteractions();
  setupQuickPick();
  setupShortcuts();
  setupFileActions();
  setupFileWatcherListener();
  initExtensionHost();
  await loadWorkspaceFiles();
  await restoreSessionState();
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
        const uri = pathToUri(activeFilePath || model.uri.path);
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
        const uri = pathToUri(activeFilePath || model.uri.path);
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
        const uri = pathToUri(activeFilePath || model.uri.path);
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
            const targetTab = openTabs.get(normalizePath(defMatch.file_path));
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
        const uri = pathToUri(activeFilePath || model.uri.path);
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

    // Signature Help Provider (Ctrl+Shift+Space)
    monaco.languages.registerSignatureHelpProvider(lang, {
      signatureHelpTriggerCharacters: ["(", ","],
      provideSignatureHelp: async (model, position) => {
        const uri = pathToUri(activeFilePath || model.uri.path);
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/signatureHelp",
            params: {
              textDocument: { uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            },
          });
          if (res && res.signatures && res.signatures.length > 0) {
            return {
              value: {
                signatures: res.signatures.map((s: any) => ({
                  label: s.label,
                  documentation: s.documentation?.value || s.documentation,
                  parameters: s.parameters?.map((p: any) => ({ label: p.label })) || [],
                })),
                activeSignature: res.activeSignature || 0,
                activeParameter: res.activeParameter || 0,
              },
              dispose: () => {},
            };
          }
        } catch (e) {}
        return null;
      },
    });

    // Code Action Provider (Ctrl+.)
    monaco.languages.registerCodeActionProvider(lang, {
      provideCodeActions: async (model, range) => {
        const uri = pathToUri(activeFilePath || model.uri.path);
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/codeAction",
            params: {
              textDocument: { uri },
              range: {
                start: { line: range.startLineNumber - 1, character: range.startColumn - 1 },
                end: { line: range.endLineNumber - 1, character: range.endColumn - 1 },
              },
              context: { diagnostics: [] },
            },
          });
          if (Array.isArray(res)) {
            return {
              actions: res.map((a: any) => ({
                title: a.title,
                kind: a.kind,
                isPreferred: Boolean(a.isPreferred),
              })),
              dispose: () => {},
            };
          }
        } catch (e) {}
        return { actions: [], dispose: () => {} };
      },
    });

    // Rename Provider (F2)
    monaco.languages.registerRenameProvider(lang, {
      provideRenameEdits: async (model, position, newName) => {
        const uri = pathToUri(activeFilePath || model.uri.path);
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/rename",
            params: {
              textDocument: { uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
              newName,
            },
          });
          if (res && res.changes) {
            const edits: monaco.languages.IWorkspaceTextEdit[] = [];
            for (const [fileUri, textEdits] of Object.entries(res.changes)) {
              (textEdits as any[]).forEach((te) => {
                edits.push({
                  resource: monaco.Uri.parse(fileUri),
                  textEdit: {
                    range: new monaco.Range(
                      te.range.start.line + 1,
                      te.range.start.character + 1,
                      te.range.end.line + 1,
                      te.range.end.character + 1
                    ),
                    text: te.newText,
                  },
                  versionId: undefined,
                });
              });
            }
            return { edits };
          }
        } catch (e) {}
        return { edits: [] };
      },
    });

    // Reference Provider (Shift+F12)
    monaco.languages.registerReferenceProvider(lang, {
      provideReferences: async (model, position) => {
        const uri = pathToUri(activeFilePath || model.uri.path);
        try {
          const res: any = await invoke("lsp_send_request", {
            lang,
            method: "textDocument/references",
            params: {
              textDocument: { uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
              context: { includeDeclaration: true },
            },
          });
          if (Array.isArray(res)) {
            return res.map((r: any) => ({
              uri: monaco.Uri.parse(r.uri),
              range: new monaco.Range(
                r.range.start.line + 1,
                r.range.start.character + 1,
                r.range.end.line + 1,
                r.range.end.character + 1
              ),
            }));
          }
        } catch (e) {}
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
      workspaceRoot,
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

  editor1.onDidFocusEditorText(() => {
    activeEditorPane = 1;
    closeGlobalMenu();
    if (pane1FilePath) updateStatusBar(pane1FilePath);
  });
  editor2.onDidFocusEditorText(() => {
    activeEditorPane = 2;
    closeGlobalMenu();
    if (pane2FilePath) updateStatusBar(pane2FilePath);
  });
  editor1.onMouseDown(() => closeGlobalMenu());
  editor2.onMouseDown(() => closeGlobalMenu());

  editor1.onDidChangeCursorPosition((e) => {
    if (activeEditorPane === 1) {
      const statusLineCol = document.getElementById("status-line-col");
      if (statusLineCol) {
        statusLineCol.textContent = `行: ${e.position.lineNumber}, 列: ${e.position.column}`;
      }
    }
  });

  editor2.onDidChangeCursorPosition((e) => {
    if (activeEditorPane === 2) {
      const statusLineCol = document.getElementById("status-line-col");
      if (statusLineCol) {
        statusLineCol.textContent = `行: ${e.position.lineNumber}, 列: ${e.position.column}`;
      }
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

  editor1.onMouseDown(() => closeGlobalMenu());
  editor2.onMouseDown(() => closeGlobalMenu());

  const welcomePath = normalizePath("welcome.rs");
  openTabs.set(welcomePath, {
    path: welcomePath,
    name: "welcome.rs",
    model: initialModel,
    isDirty: false,
    version: 1,
  });
  activeFilePath = welcomePath;
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
    const uri = pathToUri(activeFilePath || model.uri.path);
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
      { label: "ファイルを開く...", shortcut: "Ctrl+O", action: () => openNativeFileDialog() },
      { label: "フォルダーを開く...", shortcut: "Ctrl+K Ctrl+O", action: () => openNativeFolderDialog() },
      { label: "ファイルでワークスペースを開く...", action: () => openNativeFileDialog() },
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
      { label: "名前を付けてワークスペースを保存...", action: () => saveNativeFileDialog() },
      { label: "ワークスペースを複製", shortcut: "Ctrl+W Ctrl+A", action: () => showStatusMessage("ワークスペースを複製しました") },
      { type: "separator" },
      { label: "保存", shortcut: "Ctrl+S", action: () => saveActiveFile() },
      { label: "名前を付けて保存...", shortcut: "Ctrl+Shift+S", action: () => saveNativeFileDialog() },
      { label: "すべて保存", shortcut: "Ctrl+K S", action: () => saveAllFiles() },
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
      { label: "ターミナルをクリア", action: () => getActiveTerminalSession()?.terminal.clear() },
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

  const backdropEl = document.getElementById("menu-backdrop");

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
    backdropEl?.classList.remove("hidden");
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

  backdropEl?.addEventListener("pointerdown", () => closeGlobalMenu());
  backdropEl?.addEventListener("click", () => closeGlobalMenu());

  // Capture-phase pointerdown to dismiss menus when clicking anywhere outside
  window.addEventListener(
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

  window.addEventListener(
    "mousedown",
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

  const menuKeys = ["file", "edit", "selection", "view", "go", "run", "terminal", "help"];

  window.addEventListener("keydown", (e) => {
    if (e.key === "Alt" || e.key === "F10") {
      e.preventDefault();
      if (isTopMenuOpen) {
        closeGlobalMenu();
      } else {
        const firstBtn = menuButtons[0];
        if (firstBtn) {
          const key = firstBtn.getAttribute("data-menu") || "file";
          openMenu(key, firstBtn);
        }
      }
      return;
    }

    if (!isTopMenuOpen) return;

    if (e.key === "Escape") {
      closeGlobalMenu();
      return;
    }

    if (e.key === "ArrowRight") {
      e.preventDefault();
      const currentIdx = menuKeys.indexOf(currentOpenMenuKey || "file");
      const nextIdx = (currentIdx + 1) % menuKeys.length;
      const nextKey = menuKeys[nextIdx];
      const nextBtn = Array.from(menuButtons).find((b) => b.getAttribute("data-menu") === nextKey);
      if (nextBtn) openMenu(nextKey, nextBtn);
      return;
    }

    if (e.key === "ArrowLeft") {
      e.preventDefault();
      const currentIdx = menuKeys.indexOf(currentOpenMenuKey || "file");
      const prevIdx = (currentIdx - 1 + menuKeys.length) % menuKeys.length;
      const prevKey = menuKeys[prevIdx];
      const prevBtn = Array.from(menuButtons).find((b) => b.getAttribute("data-menu") === prevKey);
      if (prevBtn) openMenu(prevKey, prevBtn);
      return;
    }

    const items = dropdownEl?.querySelectorAll<HTMLElement>(".menu-dropdown-item:not(.disabled)");
    if (!items || items.length === 0) return;

    const focused = Array.from(items).findIndex((el) => el.classList.contains("focused"));

    if (e.key === "ArrowDown") {
      e.preventDefault();
      items.forEach((el) => el.classList.remove("focused"));
      const next = focused === -1 ? 0 : (focused + 1) % items.length;
      items[next].classList.add("focused");
      items[next].scrollIntoView({ block: "nearest" });
      return;
    }

    if (e.key === "ArrowUp") {
      e.preventDefault();
      items.forEach((el) => el.classList.remove("focused"));
      const prev = focused === -1 ? items.length - 1 : (focused - 1 + items.length) % items.length;
      items[prev].classList.add("focused");
      items[prev].scrollIntoView({ block: "nearest" });
      return;
    }

    if (e.key === "Enter") {
      e.preventDefault();
      if (focused !== -1 && items[focused]) {
        items[focused].click();
      }
      return;
    }
  });
}

function closeGlobalMenu() {
  isTopMenuOpen = false;
  currentOpenMenuKey = null;
  const dropdownEl = document.getElementById("global-menu-dropdown");
  dropdownEl?.classList.add("hidden");
  const backdropEl = document.getElementById("menu-backdrop");
  backdropEl?.classList.add("hidden");
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
    getActiveTerminalSession()?.fitAddon.fit();
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
  const pane1 = document.getElementById("editor-pane-1");
  const pane2 = document.getElementById("editor-pane-2");
  const gridResizer = document.getElementById("grid-resizer");
  const editorGrid = document.getElementById("editor-grid");

  if (btnSplitRight && pane1 && pane2 && gridResizer && editorGrid) {
    btnSplitRight.addEventListener("click", () => {
      isSplitActive = true;
      splitOrientation = "horizontal";
      editorGrid.style.flexDirection = "row";
      pane1.style.flex = "0 0 50%";
      pane2.style.flex = "0 0 50%";
      gridResizer.className = "resizer horizontal";
      pane2.classList.remove("hidden");
      gridResizer.classList.remove("hidden");
      activeEditorPane = 2;
      editor1?.layout();
      editor2?.layout();
      showStatusMessage("エディターを左右に分割しました");
    });
  }

  if (btnSplitDown && pane1 && pane2 && gridResizer && editorGrid) {
    btnSplitDown.addEventListener("click", () => {
      isSplitActive = true;
      splitOrientation = "vertical";
      editorGrid.style.flexDirection = "column";
      pane1.style.flex = "0 0 50%";
      pane2.style.flex = "0 0 50%";
      gridResizer.className = "resizer vertical";
      pane2.classList.remove("hidden");
      gridResizer.classList.remove("hidden");
      activeEditorPane = 2;
      editor1?.layout();
      editor2?.layout();
      showStatusMessage("エディターを上下に分割しました");
    });
  }

  if (btnCloseSplit && pane1 && pane2 && gridResizer) {
    btnCloseSplit.addEventListener("click", () => {
      isSplitActive = false;
      pane2.classList.add("hidden");
      gridResizer.classList.add("hidden");
      pane1.style.flex = "1 1 100%";
      activeEditorPane = 1;
      editor1?.layout();
      showStatusMessage("エディター分割を閉じました");
    });
  }
}

// 6. Integrated Real-time Terminal (Multiple Sessions) & Panel Tabs (Issue #28, #31)
async function setupIntegratedTerminal() {
  const btnAdd = document.getElementById("btn-add-terminal");
  const btnKill = document.getElementById("btn-kill-terminal");

  btnAdd?.addEventListener("click", () => createNewTerminalSession());
  btnKill?.addEventListener("click", () => {
    if (activeTerminalSessionId !== null) {
      killTerminalSession(activeTerminalSessionId);
    }
  });

  setupPanelTabs();
  await createNewTerminalSession();

  window.addEventListener("resize", () => {
    const active = terminalSessions.find((s) => s.id === activeTerminalSessionId);
    active?.fitAddon.fit();
  });
}

function setupPanelTabs() {
  const tabTerm = document.getElementById("panel-tab-terminal");
  const tabOut = document.getElementById("panel-tab-output");
  const tabProb = document.getElementById("panel-tab-problems");
  const termContainer = document.getElementById("terminal-container");
  const outContainer = document.getElementById("output-container");
  const probContainer = document.getElementById("problems-container");
  const termActions = document.getElementById("terminal-actions");

  const selectTab = (type: "terminal" | "output" | "problems") => {
    tabTerm?.classList.toggle("active", type === "terminal");
    tabOut?.classList.toggle("active", type === "output");
    tabProb?.classList.toggle("active", type === "problems");

    if (termContainer) termContainer.style.display = type === "terminal" ? "block" : "none";
    if (outContainer) outContainer.classList.toggle("hidden", type !== "output");
    if (probContainer) {
      probContainer.classList.toggle("hidden", type !== "problems");
      if (type === "problems") renderProblemsPanel();
    }
    if (termActions) termActions.style.display = type === "terminal" ? "flex" : "none";
  };

  tabTerm?.addEventListener("click", () => selectTab("terminal"));
  tabOut?.addEventListener("click", () => selectTab("output"));
  tabProb?.addEventListener("click", () => selectTab("problems"));

  // Initial tab selection
  selectTab("terminal");
}

function renderProblemsPanel() {
  const treeEl = document.getElementById("problems-tree");
  if (!treeEl) return;

  treeEl.innerHTML = "";
  const allMarkers = monaco.editor.getModelMarkers({});

  if (allMarkers.length === 0) {
    treeEl.innerHTML = `<div style="color: #888; padding: 8px;">ワークスペースに検出された問題はありません。</div>`;
    return;
  }

  allMarkers.forEach((m) => {
    const item = document.createElement("div");
    item.className = "problem-item";
    const isErr = m.severity === monaco.MarkerSeverity.Error;
    const icon = isErr ? "❌" : "⚠️";
    const iconClass = isErr ? "error" : "warning";

    const path = uriToPath(m.resource.toString());
    const fileName = path.split("/").pop() || path;

    item.innerHTML = `
      <span class="problem-icon ${iconClass}">${icon}</span>
      <span style="color: #fff; font-weight: 500;">${fileName} [${m.startLineNumber}, ${m.startColumn}]:</span>
      <span style="color: #ccc;">${m.message}</span>
    `;

    item.onclick = async () => {
      await openFile(path, fileName);
      if (editor1) {
        editor1.revealLineInCenter(m.startLineNumber);
        editor1.setPosition({ lineNumber: m.startLineNumber, column: m.startColumn });
      }
    };

    treeEl.appendChild(item);
  });
}

async function createNewTerminalSession() {
  const container = document.getElementById("terminal-container");
  const tabsContainer = document.getElementById("terminal-session-tabs");
  if (!container) return;

  const sessionId = nextTerminalId++;
  const sessionDiv = document.createElement("div");
  sessionDiv.id = `terminal-instance-${sessionId}`;
  sessionDiv.style.width = "100%";
  sessionDiv.style.height = "100%";
  container.appendChild(sessionDiv);

  const term = new Terminal({
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

  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(sessionDiv);

  try {
    const cols = term.cols || 80;
    const rows = term.rows || 24;
    const currentPtyId = await invoke<number>("spawn_pty", { cols, rows });

    await listen<string>(`pty-data-${currentPtyId}`, (event) => {
      term.write(event.payload);
    });

    term.onData((data) => {
      invoke("write_pty", { id: currentPtyId, data });
    });

    term.onResize((size) => {
      invoke("resize_pty", { id: currentPtyId, cols: size.cols, rows: size.rows });
    });

    const session: TerminalSession = {
      id: sessionId,
      ptyId: currentPtyId,
      title: `${sessionId}: powershell`,
      terminal: term,
      fitAddon: fit,
      containerEl: sessionDiv,
    };

    terminalSessions.push(session);
    switchTerminalSession(sessionId);
  } catch (err) {
    term.writeln(`\x1b[31mFailed to spawn PTY: ${err}\x1b[0m`);
  }
}

function switchTerminalSession(sessionId: number) {
  activeTerminalSessionId = sessionId;
  terminalSessions.forEach((s) => {
    if (s.id === sessionId) {
      s.containerEl.style.display = "block";
      setTimeout(() => s.fitAddon.fit(), 50);
      s.terminal.focus();
    } else {
      s.containerEl.style.display = "none";
    }
  });

  renderTerminalSessionTabs();
}

function killTerminalSession(sessionId: number) {
  const idx = terminalSessions.findIndex((s) => s.id === sessionId);
  if (idx === -1) return;

  const session = terminalSessions[idx];
  session.terminal.dispose();
  session.containerEl.remove();
  terminalSessions.splice(idx, 1);

  if (terminalSessions.length > 0) {
    const nextSession = terminalSessions[Math.min(idx, terminalSessions.length - 1)];
    switchTerminalSession(nextSession.id);
  } else {
    activeTerminalSessionId = null;
    renderTerminalSessionTabs();
    createNewTerminalSession();
  }
}

function renderTerminalSessionTabs() {
  const tabsContainer = document.getElementById("terminal-session-tabs");
  if (!tabsContainer) return;

  tabsContainer.innerHTML = "";
  terminalSessions.forEach((s) => {
    const tabEl = document.createElement("span");
    tabEl.className = `terminal-tab ${s.id === activeTerminalSessionId ? "active" : ""}`;
    tabEl.textContent = s.title;
    tabEl.onclick = () => switchTerminalSession(s.id);
    tabsContainer.appendChild(tabEl);
  });
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
async function openFile(rawPath: string, name?: string, targetPane?: 1 | 2) {
  if (!editor1) return;

  const path = normalizePath(rawPath);
  const fileName = name || path.split("/").pop() || path;
  const pane = targetPane ?? activeEditorPane;

  if (openTabs.has(path)) {
    activeFilePath = path;
    const tab = openTabs.get(path)!;
    if (pane === 1 || !isSplitActive) {
      editor1.setModel(tab.model);
      pane1FilePath = path;
    } else if (editor2) {
      editor2.setModel(tab.model);
      pane2FilePath = path;
    }
    updateTabBar();
    updateStatusBar(path);
    applyStoredSettings();
    loadWorkspaceFiles();
    return;
  }

  try {
    const content = await invoke<string>("read_file_content", { path });
    const language = getLanguageFromPath(path);
    const modelUri = monaco.Uri.parse(pathToUri(path));

    let model = monaco.editor.getModel(modelUri);
    if (!model) {
      model = monaco.editor.createModel(content, language, modelUri);
    } else {
      model.setValue(content);
    }

    const tabData: OpenTab = {
      path,
      name: fileName,
      model,
      isDirty: false,
      version: 1,
    };

    model.onDidChangeContent(() => {
      if (openTabs.has(path)) {
        const tab = openTabs.get(path)!;
        tab.isDirty = true;
        tab.version++;
        updateTabBar();

        // Notify LSP of change with incrementing version
        invoke("lsp_send_notification", {
          lang: language,
          method: "textDocument/didChange",
          params: {
            textDocument: { uri: pathToUri(path), version: tab.version },
            contentChanges: [{ text: model.getValue() }],
          },
        }).catch(() => {});
      }
    });

    openTabs.set(path, tabData);
    activeFilePath = path;
    if (pane === 1 || !isSplitActive) {
      editor1.setModel(model);
      pane1FilePath = path;
    } else if (editor2) {
      editor2.setModel(model);
      pane2FilePath = path;
    }

    // Ensure LSP is running and send didOpen
    ensureLspServerStarted(language);
    invoke("lsp_send_notification", {
      lang: language,
      method: "textDocument/didOpen",
      params: {
        textDocument: {
          uri: pathToUri(path),
          languageId: language,
          version: 1,
          text: content,
        },
      },
    }).catch(() => {});

    updateTabBar();
    updateStatusBar(path);
    applyStoredSettings();
    loadWorkspaceFiles();
    showStatusMessage(`開きました: ${fileName}`);
  } catch (err: any) {
    const errMsg = String(err);
    if (errMsg.includes("BINARY_FILE")) {
      showStatusMessage(`⚠️ バイナリファイルのため開けません: ${fileName}`);
    } else {
      showStatusMessage(`エラー: ファイルを開けませんでした (${err})`);
    }
  }
}

// 10. Save Active File (Ctrl+S)
async function saveActiveFile() {
  if (!activeFilePath || !editor1) return;
  const tab = openTabs.get(activeFilePath);
  if (!tab) return;

  await saveFileTab(tab);
}

async function saveAllFiles() {
  const dirtyTabs = Array.from(openTabs.values()).filter((t) => t.isDirty);
  if (dirtyTabs.length === 0) {
    showStatusMessage("保存する変更はありません");
    return;
  }

  for (const tab of dirtyTabs) {
    await saveFileTab(tab);
  }
  showStatusMessage(`すべてのファイルを保存しました (${dirtyTabs.length}件)`);
}

async function saveFileTab(tab: OpenTab) {
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
        textDocument: { uri: pathToUri(tab.path) },
        text: content,
      },
    }).catch(() => {});
  } catch (err) {
    showStatusMessage(`保存失敗: ${err}`);
  }
}

function promptSaveIfDirty(tab: OpenTab): Promise<"save" | "dontsave" | "cancel"> {
  return new Promise((resolve) => {
    if (!tab.isDirty) {
      resolve("dontsave");
      return;
    }

    const modal = document.getElementById("confirm-modal");
    const titleEl = document.getElementById("confirm-modal-title");
    const msgEl = document.getElementById("confirm-modal-message");
    const btnSave = document.getElementById("confirm-btn-save");
    const btnDontSave = document.getElementById("confirm-btn-dontsave");
    const btnCancel = document.getElementById("confirm-btn-cancel");

    if (!modal || !titleEl || !msgEl || !btnSave || !btnDontSave || !btnCancel) {
      const ok = confirm(`'${tab.name}' への変更を保存しますか？`);
      resolve(ok ? "save" : "dontsave");
      return;
    }

    titleEl.textContent = `'${tab.name}' への変更を保存しますか？`;
    msgEl.textContent = "保存しない場合、変更内容はすべて失われます。";
    modal.classList.remove("hidden");

    const cleanup = () => {
      modal.classList.add("hidden");
      btnSave.onclick = null;
      btnDontSave.onclick = null;
      btnCancel.onclick = null;
    };

    btnSave.onclick = async () => {
      cleanup();
      await saveFileTab(tab);
      resolve("save");
    };

    btnDontSave.onclick = () => {
      cleanup();
      resolve("dontsave");
    };

    btnCancel.onclick = () => {
      cleanup();
      resolve("cancel");
    };
  });
}

function getFileIcon(filename: string): string {
  const ext = filename.split(".").pop()?.toLowerCase() || "";
  switch (ext) {
    case "rs": return "🦀";
    case "ts": return "📘";
    case "js": return "🟨";
    case "json": return "⚙";
    case "toml": return "📦";
    case "md": return "📝";
    case "html": return "🌐";
    case "css": return "🎨";
    case "py": return "🐍";
    case "go": return "🔷";
    case "lock": return "🔒";
    default: return "📄";
  }
}

function updateBreadcrumbs(filePath: string | null) {
  const breadcrumbEl = document.getElementById("breadcrumb-bar");
  if (!breadcrumbEl) return;

  if (!filePath) {
    breadcrumbEl.innerHTML = `<span class="breadcrumb-item">ファイルが開かれていません</span>`;
    document.title = "Oxide Editor";
    return;
  }

  const normalized = normalizePath(filePath);
  const parts = normalized.split("/").filter(Boolean);
  
  breadcrumbEl.innerHTML = "";
  
  parts.forEach((part, index) => {
    const isLast = index === parts.length - 1;
    const itemEl = document.createElement("span");
    itemEl.className = `breadcrumb-item clickable ${isLast ? "active-file" : "folder"}`;
    
    const icon = isLast ? getFileIcon(part) : "📁";
    itemEl.innerHTML = `<span class="breadcrumb-icon">${icon}</span> <span>${part}</span>`;
    
    // Build path up to this segment
    const segmentPath = parts.slice(0, index + 1).join("/");
    itemEl.onclick = (e) => {
      e.stopPropagation();
      showBreadcrumbPicker(segmentPath, isLast, e.clientX, e.clientY);
    };

    breadcrumbEl.appendChild(itemEl);

    if (!isLast) {
      const sep = document.createElement("span");
      sep.className = "breadcrumb-separator";
      sep.textContent = "›";
      breadcrumbEl.appendChild(sep);
    }
  });

  const fileName = parts[parts.length - 1] || "Oxide Editor";
  document.title = `${fileName} - Oxide Editor`;
}

async function showBreadcrumbPicker(targetPath: string, isFile: boolean, x: number, y: number) {
  const dropdown = document.getElementById("global-menu-dropdown");
  if (!dropdown) return;

  // If clicking on a file, show sibling files in same parent dir
  const dirPath = isFile ? targetPath.substring(0, targetPath.lastIndexOf("/")) : targetPath;

  try {
    const allFiles = await invoke<FileEntry[]>("list_workspace_files");
    const normalizedDir = normalizePath(dirPath);
    
    // Filter files and direct subfolders in this directory
    const siblings = allFiles.filter((f) => {
      const p = normalizePath(f.path);
      const parent = p.substring(0, p.lastIndexOf("/"));
      return parent === normalizedDir || (!normalizedDir && !p.includes("/"));
    });

    dropdown.innerHTML = "";
    dropdown.className = "vs-dropdown";
    dropdown.style.left = `${Math.min(x, window.innerWidth - 240)}px`;
    dropdown.style.top = `${Math.min(y + 12, window.innerHeight - 300)}px`;
    dropdown.classList.remove("hidden");
    isTopMenuOpen = true;

    const items: MenuItemDef[] = siblings.map((s) => ({
      label: `${s.is_dir ? "📁" : getFileIcon(s.name)} ${s.name}`,
      action: () => {
        if (!s.is_dir) {
          openFile(s.path, s.name);
        }
      },
    }));

    if (items.length === 0) {
      items.push({ label: "(項目なし)", disabled: true });
    }

    renderMenuLevel(items, dropdown);
  } catch (e) {
    console.error(e);
  }
}

// Closed tabs history stack for Ctrl+Shift+T
const closedTabsStack: Array<{ path: string; name: string }> = [];
let draggedTabPath: string | null = null;

// 11. Tab Bar Rendering, Drag & Drop, & Context Menu (Issue #32)
function updateTabBar() {
  const tabBar = document.getElementById("tab-bar");
  if (!tabBar) return;

  tabBar.innerHTML = "";

  openTabs.forEach((tab, path) => {
    const tabEl = document.createElement("div");
    const isActive = path === activeFilePath;
    tabEl.className = `tab ${isActive ? "active" : ""}`;
    tabEl.draggable = true;

    // HTML5 Drag & Drop
    tabEl.addEventListener("dragstart", (e) => {
      draggedTabPath = path;
      e.dataTransfer?.setData("text/plain", path);
    });

    tabEl.addEventListener("dragover", (e) => {
      e.preventDefault();
      tabEl.classList.add("drag-over");
    });

    tabEl.addEventListener("dragleave", () => {
      tabEl.classList.remove("drag-over");
    });

    tabEl.addEventListener("drop", (e) => {
      e.preventDefault();
      tabEl.classList.remove("drag-over");
      if (draggedTabPath && draggedTabPath !== path) {
        // Reorder tabs map
        const entries = Array.from(openTabs.entries());
        const fromIdx = entries.findIndex(([p]) => p === draggedTabPath);
        const toIdx = entries.findIndex(([p]) => p === path);
        if (fromIdx !== -1 && toIdx !== -1) {
          const [moved] = entries.splice(fromIdx, 1);
          entries.splice(toIdx, 0, moved);
          openTabs.clear();
          entries.forEach(([p, t]) => openTabs.set(p, t));
          updateTabBar();
        }
      }
    });

    const iconEl = document.createElement("span");
    iconEl.className = "tab-icon";
    iconEl.textContent = getFileIcon(tab.name);
    tabEl.appendChild(iconEl);

    const titleEl = document.createElement("span");
    titleEl.className = "tab-title";
    titleEl.textContent = `${tab.name}${tab.isDirty ? " ●" : ""}`;
    tabEl.appendChild(titleEl);

    const closeBtn = document.createElement("span");
    closeBtn.className = "tab-close";
    closeBtn.textContent = "×";
    closeBtn.title = "閉じる";
    closeBtn.onclick = async (e) => {
      e.stopPropagation();
      await closeTab(path);
    };
    tabEl.appendChild(closeBtn);

    tabEl.onclick = () => openFile(tab.path, tab.name);

    // Middle-click to close (mouse wheel)
    tabEl.onauxclick = async (e) => {
      if (e.button === 1) {
        e.preventDefault();
        e.stopPropagation();
        await closeTab(path);
      }
    };

    tabEl.oncontextmenu = (e) => {
      e.preventDefault();
      e.stopPropagation();
      showTabContextMenu(e.clientX, e.clientY, path);
    };
    tabBar.appendChild(tabEl);
  });

  updateBreadcrumbs(activeFilePath);
  saveSessionState();
}

async function closeTab(rawPath: string): Promise<boolean> {
  const path = normalizePath(rawPath);
  if (!openTabs.has(path)) return false;

  const tab = openTabs.get(path)!;
  const choice = await promptSaveIfDirty(tab);
  if (choice === "cancel") {
    return false;
  }

  // Record into closed tabs history stack
  closedTabsStack.push({ path: tab.path, name: tab.name });

  // Send didClose to LSP
  const language = getLanguageFromPath(tab.path);
  invoke("lsp_send_notification", {
    lang: language,
    method: "textDocument/didClose",
    params: {
      textDocument: { uri: pathToUri(tab.path) },
    },
  }).catch(() => {});

  const keys = Array.from(openTabs.keys());
  const closedIndex = keys.indexOf(path);

  tab.model.dispose();
  openTabs.delete(path);

  if (activeFilePath === path) {
    const remainingKeys = Array.from(openTabs.keys());
    if (remainingKeys.length > 0) {
      const nextIndex = Math.min(Math.max(0, closedIndex - 1), remainingKeys.length - 1);
      const nextPath = remainingKeys[nextIndex];
      openFile(nextPath, openTabs.get(nextPath)!.name);
    } else {
      activeFilePath = null;
      pane1FilePath = null;
      pane2FilePath = null;
      if (editor1) {
        const emptyModel = monaco.editor.createModel("", "plaintext");
        editor1.setModel(emptyModel);
      }
      if (editor2 && isSplitActive) {
        const emptyModel = monaco.editor.createModel("", "plaintext");
        editor2.setModel(emptyModel);
      }
    }
  }
  updateTabBar();
  return true;
}

// Restore Last Closed Tab (Ctrl+Shift+T)
async function restoreLastClosedTab() {
  if (closedTabsStack.length === 0) {
    showToast("復元可能な閉じたタブはありません", "info");
    return;
  }
  const last = closedTabsStack.pop()!;
  await openFile(last.path, last.name);
  showToast(`復元しました: ${last.name}`, "info");
}

// Session State Persistence (Issue #34)
function workspaceSessionKey(key: string) {
  return `oxide_workspace:${encodeURIComponent(workspaceRoot)}:${key}`;
}

function saveSessionState() {
  const tabPaths = Array.from(openTabs.keys());
  localStorage.setItem(workspaceSessionKey("tabs"), JSON.stringify(tabPaths));
  if (activeFilePath) {
    localStorage.setItem(workspaceSessionKey("active-tab"), activeFilePath);
  }
}

async function restoreSessionState() {
  try {
    const savedTabsStr = localStorage.getItem(workspaceSessionKey("tabs"));
    const activeTab = localStorage.getItem(workspaceSessionKey("active-tab"));
    if (savedTabsStr) {
      const tabPaths: string[] = JSON.parse(savedTabsStr);
      for (const p of tabPaths) {
        const name = p.split("/").pop() || p;
        await openFile(p, name);
      }
      if (activeTab && openTabs.has(activeTab)) {
        openFile(activeTab, openTabs.get(activeTab)!.name);
      }
    }
  } catch (e) {
    console.error("Failed to restore session state:", e);
  }
}

async function closeOtherTabs(targetRawPath: string) {
  const keepPath = normalizePath(targetRawPath);
  const paths = Array.from(openTabs.keys()).filter((p) => p !== keepPath);
  for (const p of paths) {
    const closed = await closeTab(p);
    if (!closed) break;
  }
  if (activeFilePath !== keepPath) {
    const keepTab = openTabs.get(keepPath);
    if (keepTab) openFile(keepTab.path, keepTab.name);
  } else {
    updateTabBar();
  }
}

async function closeTabsToTheRight(targetRawPath: string) {
  const targetPath = normalizePath(targetRawPath);
  const keys = Array.from(openTabs.keys());
  const idx = keys.indexOf(targetPath);
  if (idx !== -1) {
    for (let i = idx + 1; i < keys.length; i++) {
      const p = keys[i];
      const closed = await closeTab(p);
      if (!closed) break;
    }
  }
}

async function closeAllTabs() {
  const paths = Array.from(openTabs.keys());
  for (const p of paths) {
    const closed = await closeTab(p);
    if (!closed) break;
  }
}

function showTabContextMenu(x: number, y: number, targetPath: string) {
  const dropdown = document.getElementById("global-menu-dropdown");
  if (!dropdown) return;

  dropdown.innerHTML = "";
  dropdown.className = "vs-dropdown";
  dropdown.style.left = `${Math.min(x, window.innerWidth - 240)}px`;
  dropdown.style.top = `${Math.min(y, window.innerHeight - 250)}px`;
  dropdown.classList.remove("hidden");
  isTopMenuOpen = true;

  const items = [
    { label: "閉じる (Close)", shortcut: "Ctrl+W", action: () => closeTab(targetPath) },
    { label: "他のタブを閉じる (Close Others)", action: () => closeOtherTabs(targetPath) },
    { label: "右側のタブを閉じる (Close to the Right)", action: () => closeTabsToTheRight(targetPath) },
    { label: "すべて閉じる (Close All)", shortcut: "Ctrl+K Ctrl+W", action: () => closeAllTabs() },
    { type: "separator" },
    {
      label: "パスをコピー (Copy Path)",
      action: () => {
        navigator.clipboard.writeText(targetPath);
        showStatusMessage("パスをクリップボードにコピーしました");
      },
    },
    {
      label: "相対パスをコピー (Copy Relative Path)",
      action: () => {
        const rel = targetPath.split(/[/\\]/).pop() || targetPath;
        navigator.clipboard.writeText(rel);
        showStatusMessage("相対パスをクリップボードにコピーしました");
      },
    },
  ];

  items.forEach((item) => {
    if (item.type === "separator") {
      const sep = document.createElement("div");
      sep.className = "menu-dropdown-separator";
      dropdown.appendChild(sep);
      return;
    }

    const itemEl = document.createElement("div");
    itemEl.className = "menu-dropdown-item";
    itemEl.innerHTML = `
      <div class="item-label-group">
        <span>${item.label}</span>
      </div>
      ${item.shortcut ? `<span class="item-shortcut">${item.shortcut}</span>` : ""}
    `;
    itemEl.addEventListener("click", (e) => {
      e.stopPropagation();
      closeGlobalMenu();
      item.action?.();
    });
    dropdown.appendChild(itemEl);
  });
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
  getActiveTerminalSession()?.fitAddon.fit();
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
        <div class="search-input-container">
          <div class="search-row">
            <input type="text" id="global-search-input" placeholder="検索 (Search)..." />
            <div class="search-toggles">
              <button id="toggle-case" class="search-toggle-btn ${searchCaseSensitive ? "active" : ""}" title="大文字と小文字を区別 (Alt+C)">Aa</button>
              <button id="toggle-word" class="search-toggle-btn ${searchWholeWord ? "active" : ""}" title="単語全体に一致 (Alt+W)">\\b</button>
              <button id="toggle-regex" class="search-toggle-btn ${searchIsRegex ? "active" : ""}" title="正規表現を使用 (Alt+R)">.*</button>
            </div>
          </div>
          <div class="search-row">
            <input type="text" id="global-replace-input" placeholder="置換 (Replace)..." />
          </div>
          <div class="search-actions-row">
            <button id="btn-search-exec" class="btn btn-secondary" style="font-size: 11px; padding: 3px 8px;">検索</button>
            <button id="btn-replace-all" class="btn btn-primary" style="font-size: 11px; padding: 3px 8px;">すべて置換</button>
          </div>
          <div id="search-results-list" style="margin-top: 8px; max-height: calc(100vh - 240px); overflow-y: auto;"></div>
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
      renderSettingsView(contentEl);
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
            <span style="font-size: 10px; color: ${ext.enabled ? "#00ff80" : "#888"};">● ${ext.enabled ? "有効 (Active)" : "無効 (Disabled)"}</span>
            <button class="btn-toggle-ext" data-id="${ext.id}" data-enabled="${ext.enabled}">${ext.enabled ? "無効化" : "有効化"}</button>
          </div>
        `;
        const toggleButton = card.querySelector<HTMLButtonElement>(".btn-toggle-ext");
        toggleButton?.addEventListener("click", async (event) => {
          event.stopPropagation();
          toggleButton.disabled = true;
          try {
            await invoke("set_extension_enabled", { id: ext.id, enabled: !ext.enabled });
            await renderExtensionsView(container);
          } catch (error) {
            showToast(`拡張機能の状態を変更できませんでした: ${error}`, "error");
            toggleButton.disabled = false;
          }
        });

        card.addEventListener("click", () => {
          const fakeOpenVsxExt: OpenVsxExtension = {
            namespace: ext.id.split('.')[0] || '',
            name: ext.name,
            version: ext.version,
            display_name: ext.name,
            description: ext.description,
            download_count: null,
            icon_url: null,
            download_url: null,
            url: null
          };
          openExtensionDetail(fakeOpenVsxExt, true);
        });
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

        card.addEventListener("click", (e) => {
          if ((e.target as HTMLElement).tagName === "BUTTON") return;
          openExtensionDetail(ext, false);
        });

        const btn = card.querySelector<HTMLButtonElement>(".btn-install-ext");
        if (btn) {
          btn.addEventListener("click", async (e) => {
            e.stopPropagation();
            btn.textContent = "インストール中...";
            btn.disabled = true;
            try {
              const res = await invoke<string>("install_openvsx_extension", {
                namespace: ext.namespace,
                name: ext.name,
                version: ext.version,
                description: ext.description || "",
                downloadUrl: ext.download_url || null,
              });
              showStatusMessage(res);
              btn.textContent = "✓ インストール済み";
              btn.style.backgroundColor = "#2ea043";

              // リアルプロセス起動 (CPU/メモリ消費連動)
              if (ext.name.includes("rust")) ensureLspServerStarted("rust");
              if (ext.name.includes("python")) ensureLspServerStarted("python");
              if (ext.name.includes("go")) ensureLspServerStarted("go");
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
  const replaceInput = document.getElementById("global-replace-input") as HTMLInputElement;
  const list = document.getElementById("search-results-list");
  const toggleCase = document.getElementById("toggle-case");
  const toggleWord = document.getElementById("toggle-word");
  const toggleRegex = document.getElementById("toggle-regex");
  const btnSearch = document.getElementById("btn-search-exec");
  const btnReplaceAll = document.getElementById("btn-replace-all");

  if (!input || !list) return;

  const runSearch = async () => {
    const q = input.value.trim();
    if (!q) {
      list.innerHTML = "";
      return;
    }

    list.innerHTML = `<div style="color: #888; font-size: 11px;">検索中...</div>`;
    try {
      const matches = await invoke<SearchMatch[]>("search_in_workspace", {
        query: q,
        caseSensitive: searchCaseSensitive,
        wholeWord: searchWholeWord,
        isRegex: searchIsRegex,
      });
      list.innerHTML = "";

      if (matches.length === 0) {
        list.innerHTML = `<div style="color: #888; font-size: 11px; padding: 4px;">一致する結果は見つかりませんでした</div>`;
        return;
      }

      list.innerHTML = `<div style="color: #aaa; font-size: 11px; margin-bottom: 4px;">${matches.length} 件の一致:</div>`;

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
  };

  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") runSearch();
  });
  btnSearch?.addEventListener("click", () => runSearch());

  toggleCase?.addEventListener("click", () => {
    searchCaseSensitive = !searchCaseSensitive;
    toggleCase.classList.toggle("active", searchCaseSensitive);
    runSearch();
  });

  toggleWord?.addEventListener("click", () => {
    searchWholeWord = !searchWholeWord;
    toggleWord.classList.toggle("active", searchWholeWord);
    runSearch();
  });

  toggleRegex?.addEventListener("click", () => {
    searchIsRegex = !searchIsRegex;
    toggleRegex.classList.toggle("active", searchIsRegex);
    runSearch();
  });

  btnReplaceAll?.addEventListener("click", async () => {
    const q = input.value.trim();
    const rep = replaceInput ? replaceInput.value : "";
    if (!q) return;

    if (confirm(`ワークスペース全体で '${q}' を '${rep}' に置換しますか？`)) {
      try {
        const count = await invoke<number>("replace_in_workspace", {
          query: q,
          replaceText: rep,
          caseSensitive: searchCaseSensitive,
          wholeWord: searchWholeWord,
          isRegex: searchIsRegex,
        });
        showStatusMessage(`一括置換完了: ${count} 箇所を置換しました`);
        await loadWorkspaceFiles();
        runSearch();
      } catch (e) {
        alert(`置換エラー: ${e}`);
      }
    }
  });
}

function applyStoredSettings() {
  const settings = resolveSettings(workspaceRoot, getActiveLanguage());
  monaco.editor.setTheme(settings.theme);
  editor1?.updateOptions({
    fontSize: settings.fontSize,
    tabSize: settings.tabSize,
    minimap: { enabled: settings.minimap },
  });
  editor2?.updateOptions({
    fontSize: settings.fontSize,
    tabSize: settings.tabSize,
    minimap: { enabled: settings.minimap },
  });
  const statusIndent = document.getElementById("status-indent");
  if (statusIndent) statusIndent.textContent = `スペース: ${settings.tabSize}`;
}

function getActiveLanguage(): string {
  const editor = activeEditorPane === 2 && isSplitActive ? editor2 : editor1;
  return editor?.getModel()?.getLanguageId() || "plaintext";
}

function renderSettingsView(container: HTMLElement, selectedScope: SettingScope = "user") {
  const language = getActiveLanguage();
  const scopedSettings = getScopedSettings(selectedScope, workspaceRoot, language);
  const resolvedSettings = resolveSettings(workspaceRoot, language);
  const profiles = getProfiles();

  container.innerHTML = `
    <div class="settings-view">
      <section class="settings-section">
        <h3>設定スコープ</h3>
        <select id="settings-scope">
          <option value="user" ${selectedScope === "user" ? "selected" : ""}>ユーザー</option>
          <option value="workspace" ${selectedScope === "workspace" ? "selected" : ""}>ワークスペース</option>
          <option value="language" ${selectedScope === "language" ? "selected" : ""}>言語別 (${escapeHtml(language)})</option>
        </select>
        <p class="settings-hint">デフォルト → ユーザー → ワークスペース → 言語別の順に上書き</p>
        <label>カラーテーマ</label>
        <select id="theme-selector">
          <option value="vscode-dark-plus" ${resolvedSettings.theme === "vscode-dark-plus" ? "selected" : ""}>VS Code Dark+</option>
          <option value="vs" ${resolvedSettings.theme === "vs" ? "selected" : ""}>VS Code Light</option>
          <option value="hc-black" ${resolvedSettings.theme === "hc-black" ? "selected" : ""}>High Contrast</option>
        </select>
        <label>フォントサイズ</label>
        <input type="number" id="font-size-input" value="${resolvedSettings.fontSize}" min="10" max="28" />
        <label>タブサイズ</label>
        <input type="number" id="tab-size-input" value="${resolvedSettings.tabSize}" min="2" max="8" />
        <label class="settings-checkbox">
          <input type="checkbox" id="minimap-checkbox" ${resolvedSettings.minimap ? "checked" : ""} />
          ミニマップを表示
        </label>
        <label>スコープ設定JSON</label>
        <textarea id="settings-json" rows="9" spellcheck="false">${escapeHtml(JSON.stringify(scopedSettings, null, 2))}</textarea>
        <button id="apply-settings-json">JSONを適用</button>
      </section>

      <section class="settings-section">
        <h3>キーボードショートカット</h3>
        <input id="keybinding-search" type="search" placeholder="コマンドまたはキーを検索" />
        <div id="keybinding-conflicts"></div>
        <div id="keybinding-list"></div>
        <label>キーバインドJSON</label>
        <textarea id="keybindings-json" rows="12" spellcheck="false">${escapeHtml(JSON.stringify(getKeybindings(), null, 2))}</textarea>
        <div class="settings-actions">
          <button id="apply-keybindings-json">JSONを適用</button>
          <button id="reset-keybindings">既定値に戻す</button>
        </div>
      </section>

      <section class="settings-section">
        <h3>プロファイル</h3>
        <input id="profile-name" type="text" placeholder="プロファイル名" />
        <button id="create-profile">現在の構成から作成</button>
        <select id="profile-selector">
          <option value="">プロファイルを選択</option>
          ${profiles.map((profile) => `<option value="${escapeHtml(profile.id)}">${escapeHtml(profile.name)}</option>`).join("")}
        </select>
        <div class="settings-actions">
          <button id="apply-profile">適用</button>
          <button id="export-profile">エクスポート</button>
          <button id="delete-profile">削除</button>
        </div>
        <label class="settings-file-label">
          プロファイルをインポート
          <input id="import-profile" type="file" accept="application/json,.json" />
        </label>
        <p class="settings-hint">テーマ、ユーザー設定、キーバインド、拡張機能一覧を一括管理</p>
      </section>
    </div>
  `;

  setupSettingsHandlers(container, selectedScope);
}

function setupSettingsHandlers(container: HTMLElement, selectedScope: SettingScope) {
  const language = getActiveLanguage();
  const scopeSelector = container.querySelector<HTMLSelectElement>("#settings-scope");
  const settingsJson = container.querySelector<HTMLTextAreaElement>("#settings-json");
  const themeSelector = container.querySelector<HTMLSelectElement>("#theme-selector");
  const fontSizeInput = container.querySelector<HTMLInputElement>("#font-size-input");
  const tabSizeInput = container.querySelector<HTMLInputElement>("#tab-size-input");
  const minimapCheckbox = container.querySelector<HTMLInputElement>("#minimap-checkbox");

  scopeSelector?.addEventListener("change", () => {
    renderSettingsView(container, scopeSelector.value as SettingScope);
  });

  const saveSetting = (key: keyof EditorSettings, value: EditorSettings[keyof EditorSettings]) => {
    const settings = getScopedSettings(selectedScope, workspaceRoot, language);
    saveScopedSettings(selectedScope, workspaceRoot, language, { ...settings, [key]: value });
    applyStoredSettings();
    renderSettingsView(container, selectedScope);
  };

  themeSelector?.addEventListener("change", () => saveSetting("theme", themeSelector.value as EditorSettings["theme"]));
  fontSizeInput?.addEventListener("change", () => saveSetting("fontSize", Number(fontSizeInput.value)));
  tabSizeInput?.addEventListener("change", () => saveSetting("tabSize", Number(tabSizeInput.value)));
  minimapCheckbox?.addEventListener("change", () => saveSetting("minimap", minimapCheckbox.checked));

  container.querySelector("#apply-settings-json")?.addEventListener("click", () => {
    try {
      saveScopedSettings(selectedScope, workspaceRoot, language, JSON.parse(settingsJson?.value || "{}") as unknown);
      applyStoredSettings();
      renderSettingsView(container, selectedScope);
      showStatusMessage("設定JSONを適用しました");
    } catch (error) {
      alert(`設定JSONエラー: ${error}`);
    }
  });

  const renderKeybindings = (query = "") => {
    const keybindings = getKeybindings();
    const normalizedQuery = query.trim().toLowerCase();
    const list = container.querySelector("#keybinding-list");
    const conflictsContainer = container.querySelector("#keybinding-conflicts");
    if (!list || !conflictsContainer) return;

    const conflicts = findKeybindingConflicts(keybindings);
    conflictsContainer.innerHTML = conflicts.length
      ? conflicts
          .map(
            (conflict) =>
              `<div class="keybinding-conflict">${escapeHtml(conflict.key)}: ${conflict.commands
                .map((command) => escapeHtml(COMMAND_LABELS[command] || command))
                .join(" / ")}</div>`,
          )
          .join("")
      : `<div class="keybinding-ok">競合はありません</div>`;

    list.innerHTML = keybindings
      .filter((binding) => {
        const label = COMMAND_LABELS[binding.command] || binding.command;
        return !normalizedQuery || label.toLowerCase().includes(normalizedQuery) || binding.key.toLowerCase().includes(normalizedQuery);
      })
      .map(
        (binding) => `
          <label class="keybinding-row">
            <span>${escapeHtml(COMMAND_LABELS[binding.command] || binding.command)}</span>
            <input data-command="${binding.command}" value="${escapeHtml(binding.key)}" readonly />
          </label>
        `,
      )
      .join("");

    list.querySelectorAll<HTMLInputElement>("input[data-command]").forEach((input) => {
      input.addEventListener("keydown", (event) => {
        event.preventDefault();
        event.stopPropagation();
        if (event.key === "Escape") {
          input.blur();
          return;
        }
        const key = keybindingFromEvent(event);
        if (!key || ["Ctrl", "Shift", "Alt", "Meta"].includes(key)) return;
        const updated = keybindings.map((binding) =>
          binding.command === input.dataset.command ? { ...binding, key } : binding,
        );
        saveKeybindings(updated);
        const json = container.querySelector<HTMLTextAreaElement>("#keybindings-json");
        if (json) json.value = JSON.stringify(updated, null, 2);
        renderKeybindings((container.querySelector<HTMLInputElement>("#keybinding-search")?.value || ""));
      });
    });
  };

  const keybindingSearch = container.querySelector<HTMLInputElement>("#keybinding-search");
  keybindingSearch?.addEventListener("input", () => renderKeybindings(keybindingSearch.value));
  renderKeybindings();

  container.querySelector("#apply-keybindings-json")?.addEventListener("click", () => {
    const json = container.querySelector<HTMLTextAreaElement>("#keybindings-json");
    try {
      const keybindings = saveKeybindings(JSON.parse(json?.value || "[]") as unknown);
      if (json) json.value = JSON.stringify(keybindings, null, 2);
      renderKeybindings(keybindingSearch?.value);
      showStatusMessage("キーバインドJSONを適用しました");
    } catch (error) {
      alert(`キーバインドJSONエラー: ${error}`);
    }
  });

  container.querySelector("#reset-keybindings")?.addEventListener("click", () => {
    const keybindings = resetKeybindings();
    const json = container.querySelector<HTMLTextAreaElement>("#keybindings-json");
    if (json) json.value = JSON.stringify(keybindings, null, 2);
    renderKeybindings(keybindingSearch?.value);
  });

  const getSelectedProfile = () => {
    const id = container.querySelector<HTMLSelectElement>("#profile-selector")?.value;
    return getProfiles().find((profile) => profile.id === id);
  };

  container.querySelector("#create-profile")?.addEventListener("click", async () => {
    try {
      const name = container.querySelector<HTMLInputElement>("#profile-name")?.value || "";
      let extensions: string[] = [];
      try {
        extensions = (await invoke<ExtensionManifest[]>("get_installed_extensions")).map((extension) => extension.id);
      } catch {
        extensions = [];
      }
      createProfile(name, extensions);
      renderSettingsView(container, selectedScope);
      showStatusMessage("プロファイルを作成しました");
    } catch (error) {
      alert(`プロファイル作成エラー: ${error}`);
    }
  });

  container.querySelector("#apply-profile")?.addEventListener("click", () => {
    const profile = getSelectedProfile();
    if (!profile) return;
    applyProfile(profile);
    applyStoredSettings();
    renderSettingsView(container, selectedScope);
    showStatusMessage(`プロファイルを適用しました: ${profile.name}`);
  });

  container.querySelector("#delete-profile")?.addEventListener("click", () => {
    const profile = getSelectedProfile();
    if (!profile) return;
    deleteProfile(profile.id);
    renderSettingsView(container, selectedScope);
  });

  container.querySelector("#export-profile")?.addEventListener("click", () => {
    const profile = getSelectedProfile();
    if (!profile) return;
    downloadTextFile(`${profile.name.replace(/[^\w.-]+/g, "_") || "oxide-profile"}.json`, exportProfile(profile));
  });

  container.querySelector<HTMLInputElement>("#import-profile")?.addEventListener("change", async (event) => {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    try {
      const profile = importProfile(await file.text());
      renderSettingsView(container, selectedScope);
      showStatusMessage(`プロファイルをインポートしました: ${profile.name}`);
    } catch (error) {
      alert(`プロファイル読込エラー: ${error}`);
    }
  });
}

function downloadTextFile(fileName: string, content: string) {
  const url = URL.createObjectURL(new Blob([content], { type: "application/json" }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = fileName;
  anchor.click();
  URL.revokeObjectURL(url);
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

// 15. SCM (Git) Integration (Push/Pull/Stage/Unstage)
async function renderScmView(container: HTMLElement) {
  try {
    const status = await invoke<GitStatusResult>("git_get_status");
    const branchEl = document.getElementById("status-branch");
    if (branchEl) {
      branchEl.textContent = `🌿 ${status.branch}`;
    }

    container.innerHTML = `
      <div style="padding: 6px;">
        <div class="scm-header-actions">
          <button id="btn-git-pull" class="scm-action-btn" title="変更を取得 (Pull)">⬇ Pull</button>
          <button id="btn-git-push" class="scm-action-btn" title="変更を送信 (Push)">⬆ Push</button>
          <button id="btn-git-sync" class="scm-action-btn" title="同期 (Sync)">🔄 Sync</button>
        </div>
        <div style="font-size: 11px; color: #888; margin-bottom: 4px;">ブランチ: <strong style="color: #9cdcfe;">${status.branch}</strong></div>
        <textarea id="git-commit-msg" rows="2" placeholder="コミットメッセージを入力..." style="width: 100%; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px; padding: 4px; font-size: 12px;"></textarea>
        <button id="btn-commit" style="margin-top: 6px; width: 100%; padding: 6px; background: #007acc; border: none; color: #fff; border-radius: 4px; cursor: pointer; font-size: 12px; font-weight: 500;">✔ コミット実行 (Commit)</button>
        <div style="margin-top: 12px; font-size: 11px; font-weight: bold; color: #aaa;">変更されたファイル (${status.changed_files.length}):</div>
        <div id="scm-files-list" style="margin-top: 6px;"></div>
      </div>
    `;

    document.getElementById("btn-git-pull")?.addEventListener("click", async () => {
      showToast("Git Pull 実行中...", "info");
      try {
        const res = await invoke<string>("git_pull");
        showToast(res || "Git Pull 完了", "info");
        await updateGitStatus();
        await loadWorkspaceFiles();
      } catch (err) {
        showToast(`Pull 失敗: ${err}`, "error");
      }
    });

    document.getElementById("btn-git-push")?.addEventListener("click", async () => {
      showToast("Git Push 実行中...", "info");
      try {
        const res = await invoke<string>("git_push");
        showToast(res || "Git Push 完了", "info");
        await updateGitStatus();
      } catch (err) {
        showToast(`Push 失敗: ${err}`, "error");
      }
    });

    document.getElementById("btn-git-sync")?.addEventListener("click", async () => {
      showToast("Git Sync 実行中...", "info");
      try {
        await invoke("git_pull");
        const pushRes = await invoke<string>("git_push");
        showToast(pushRes || "Git 同期完了", "info");
        await updateGitStatus();
        await loadWorkspaceFiles();
      } catch (err) {
        showToast(`Sync 失敗: ${err}`, "error");
      }
    });

    const list = document.getElementById("scm-files-list");
    if (list) {
      status.changed_files.forEach((rawEntry) => {
        const trimmed = rawEntry.trim();
        const statusCode = trimmed.substring(0, 2).trim();
        const filePath = trimmed.substring(2).trim();

        let tagClass = "modified";
        let tagText = "M";
        if (statusCode.includes("?")) { tagClass = "untracked"; tagText = "U"; }
        else if (statusCode.includes("A")) { tagClass = "added"; tagText = "A"; }
        else if (statusCode.includes("D")) { tagClass = "deleted"; tagText = "D"; }

        const isStaged = !statusCode.startsWith(" ") && !statusCode.includes("?");

        const row = document.createElement("div");
        row.className = "scm-file-row";
        row.innerHTML = `
          <div style="display: flex; align-items: center; gap: 6px; overflow: hidden;">
            <span style="font-size: 11px; font-weight: bold; padding: 1px 4px; border-radius: 2px;" class="scm-status-tag ${tagClass}">${tagText}</span>
            <span style="white-space: nowrap; text-overflow: ellipsis; overflow: hidden; cursor: pointer;">${filePath}</span>
          </div>
          <button class="scm-stage-btn" title="${isStaged ? "ステージ解除 (-)" : "ステージに追加 (+)"}">${isStaged ? "−" : "+"}</button>
        `;

        row.querySelector("span:nth-child(2)")?.addEventListener("click", () => {
          openFile(filePath, filePath.split("/").pop() || filePath);
        });

        row.querySelector(".scm-stage-btn")?.addEventListener("click", async (e) => {
          e.stopPropagation();
          try {
            if (isStaged) {
              await invoke("git_unstage_file", { path: filePath });
              showToast(`ステージ解除: ${filePath}`, "info");
            } else {
              await invoke("git_stage_file", { path: filePath });
              showToast(`ステージ追加: ${filePath}`, "info");
            }
            updateSidebarView("scm");
          } catch (err) {
            showToast(`ステージング失敗: ${err}`, "error");
          }
        });

        list.appendChild(row);
      });
    }

    const btnCommit = document.getElementById("btn-commit");
    const commitInput = document.getElementById("git-commit-msg") as HTMLTextAreaElement;
    if (btnCommit && commitInput) {
      btnCommit.onclick = async () => {
        const msg = commitInput.value.trim();
        if (!msg) {
          showToast("コミットメッセージを入力してください", "warning");
          return;
        }
        try {
          await invoke<string>("git_commit", { message: msg });
          showToast("Git コミット完了", "info");
          updateSidebarView("scm");
        } catch (err) {
          showToast(`コミット失敗: ${err}`, "error");
        }
      };
    }
  } catch (err) {
    container.innerHTML = `<div style="color: #888; padding: 8px;">Git 状態取得エラー: ${err}</div>`;
  }
}

// Toast Notifications (Issue #30)
function showToast(message: string, type: "info" | "warning" | "error" = "info", duration = 4000) {
  const container = document.getElementById("toast-container");
  if (!container) return;

  const toast = document.createElement("div");
  toast.className = `toast-item ${type}`;

  const icon = type === "error" ? "❌" : type === "warning" ? "⚠️" : "ℹ️";
  toast.innerHTML = `
    <div style="display: flex; align-items: center; gap: 8px;">
      <span>${icon}</span>
      <span>${message}</span>
    </div>
    <span style="cursor: pointer; color: #888; font-size: 14px;">×</span>
  `;

  toast.querySelector("span:last-child")?.addEventListener("click", () => toast.remove());

  container.appendChild(toast);

  setTimeout(() => {
    toast.style.opacity = "0";
    toast.style.transition = "opacity 0.3s ease";
    setTimeout(() => toast.remove(), 300);
  }, duration);
}

function formatCurrentDocument() {
  editor1?.getAction("editor.action.formatDocument")?.run();
}

// 17. Workspace File Tree Loading & Actions
const collapsedFolders: Set<string> = new Set();

async function loadWorkspaceFiles() {
  // サイドバーの内容はアクティブなビューだけが描画する。検索結果から
  // ファイルを開いても、検索ビューが選択中なら結果一覧をエクスプローラーの
  // ツリーで置き換えない。
  if (currentActiveView !== "explorer") return;

  const contentEl = document.getElementById("sidebar-content");
  if (!contentEl) return;

  try {
    const files = await invoke<FileEntry[]>("list_workspace_files");
    contentEl.innerHTML = "";

    files.forEach((file) => {
      const normPath = file.path.replace(/\\/g, "/");
      const parts = normPath.split("/");
      let isHidden = false;
      let checkPath = "";
      for (let i = 0; i < parts.length - 1; i++) {
        checkPath = checkPath ? `${checkPath}/${parts[i]}` : parts[i];
        if (collapsedFolders.has(checkPath)) {
          isHidden = true;
          break;
        }
      }

      if (isHidden) return;

      const node = document.createElement("div");
      const isCurrentFile = activeFilePath && (
        file.path === activeFilePath ||
        normPath === activeFilePath ||
        activeFilePath.endsWith("/" + normPath) ||
        activeFilePath.endsWith("\\" + file.path)
      );
      node.className = `tree-node ${file.is_dir ? "tree-folder" : "tree-file"} ${isCurrentFile ? "active" : ""}`;
      node.style.paddingLeft = `${file.depth * 14 + 6}px`;

      const isCollapsed = collapsedFolders.has(file.path) || collapsedFolders.has(normPath);
      const arrowIcon = file.is_dir ? (isCollapsed ? "▶" : "▼") : "";
      const typeIcon = file.is_dir 
        ? (isCollapsed ? "📁" : "📂")
        : getFileIcon(file.name);

      node.innerHTML = `
        <div class="tree-node-left">
          ${file.is_dir ? `<span class="tree-arrow">${arrowIcon}</span>` : `<span class="tree-arrow-placeholder"></span>`}
          <span class="tree-icon">${typeIcon}</span>
          <span class="tree-label">${file.name}</span>
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

      if (file.is_dir) {
        node.addEventListener("click", () => {
          if (collapsedFolders.has(file.path) || collapsedFolders.has(normPath)) {
            collapsedFolders.delete(file.path);
            collapsedFolders.delete(normPath);
          } else {
            collapsedFolders.add(file.path);
            collapsedFolders.add(normPath);
          }
          loadWorkspaceFiles();
        });
      } else {
        node.addEventListener("click", () => openFile(file.path, file.name));
      }

      node.oncontextmenu = (e) => {
        e.preventDefault();
        e.stopPropagation();
        showExplorerContextMenu(e.clientX, e.clientY, file);
      };

      contentEl.appendChild(node);
    });
  } catch (e) {
    contentEl.innerHTML = `<div style="color: #888; padding: 8px;">ワークスペース読込中...</div>`;
  }
}

function showExplorerContextMenu(x: number, y: number, file: FileEntry) {
  const dropdown = document.getElementById("global-menu-dropdown");
  if (!dropdown) return;

  dropdown.innerHTML = "";
  dropdown.className = "vs-dropdown";
  dropdown.style.left = `${Math.min(x, window.innerWidth - 240)}px`;
  dropdown.style.top = `${Math.min(y, window.innerHeight - 280)}px`;
  dropdown.classList.remove("hidden");
  isTopMenuOpen = true;

  const items: MenuItemDef[] = [
    {
      label: "名前の変更 (Rename)",
      shortcut: "F2",
      action: () => promptRenameFile(file.path),
    },
    {
      label: "削除 (Delete)",
      shortcut: "Delete",
      action: async () => {
        if (confirm(`'${file.name}' を削除してよろしいですか？`)) {
          try {
            await invoke("delete_file", { path: file.path });
            await loadWorkspaceFiles();
            showStatusMessage(`削除完了: ${file.name}`);
          } catch (err) {
            alert(`削除エラー: ${err}`);
          }
        }
      },
    },
    { type: "separator" },
    {
      label: "パスのコピー (Copy Path)",
      action: () => {
        const full = normalizePath(file.path);
        navigator.clipboard.writeText(full);
        showStatusMessage("絶対パスをコピーしました");
      },
    },
    {
      label: "相対パスのコピー (Copy Relative Path)",
      action: () => {
        navigator.clipboard.writeText(file.path);
        showStatusMessage("相対パスをコピーしました");
      },
    },
    { type: "separator" },
    {
      label: "エクスプローラーで表示 (Reveal in OS)",
      action: async () => {
        try {
          await invoke("reveal_in_os_explorer", { path: file.path });
        } catch (e) {
          console.error(e);
        }
      },
    },
  ];

  if (file.is_dir) {
    items.unshift(
      {
        label: "ここに新しいファイル...",
        action: async () => {
          const name = prompt(`'${file.path}' 内の新規ファイル名:`);
          if (!name) return;
          const target = `${file.path.replace(/\\/g, "/")}/${name}`;
          try {
            await invoke("create_file", { path: target });
            await loadWorkspaceFiles();
            await openFile(target, name);
          } catch (e) {
            alert(`ファイル作成失敗: ${e}`);
          }
        },
      },
      {
        label: "ここに新しいフォルダー...",
        action: async () => {
          const name = prompt(`'${file.path}' 内の新規フォルダー名:`);
          if (!name) return;
          const target = `${file.path.replace(/\\/g, "/")}/${name}`;
          try {
            await invoke("create_directory", { path: target });
            await loadWorkspaceFiles();
          } catch (e) {
            alert(`フォルダー作成失敗: ${e}`);
          }
        },
      },
      { type: "separator" }
    );
  }

  renderMenuLevel(items, dropdown);
}

async function promptRenameFile(oldPath: string) {
  const normOld = normalizePath(oldPath);
  const oldName = normOld.split("/").pop() || oldPath;
  const newName = prompt("新しいファイル名/パスを入力してください:", oldName);
  if (!newName || newName === oldName) return;

  const parentDir = normOld.substring(0, normOld.lastIndexOf("/"));
  const newPath = parentDir ? `${parentDir}/${newName}` : newName;

  try {
    await invoke("rename_file", { oldPath: oldPath, newPath: newPath });
    
    // もし開いていたタブがあればパスを更新
    if (openTabs.has(normOld)) {
      const tab = openTabs.get(normOld)!;
      openTabs.delete(normOld);
      tab.path = newPath;
      tab.name = newName;
      openTabs.set(newPath, tab);
      if (activeFilePath === normOld) {
        activeFilePath = newPath;
      }
      updateTabBar();
    }

    await loadWorkspaceFiles();
    showStatusMessage(`名前変更完了: ${newName}`);
  } catch (err) {
    alert(`名前変更失敗: ${err}`);
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
      getActiveTerminalSession()?.fitAddon.fit();
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
      getActiveTerminalSession()?.fitAddon.fit();
    });

    window.addEventListener("mouseup", () => {
      if (isDragging) {
        isDragging = false;
        terminalResizer.classList.remove("dragging");
      }
    });
  }

  const gridResizer = document.getElementById("grid-resizer");
  const pane1 = document.getElementById("editor-pane-1");
  const pane2 = document.getElementById("editor-pane-2");
  const editorGrid = document.getElementById("editor-grid");

  if (gridResizer && pane1 && pane2 && editorGrid) {
    let isDraggingGrid = false;
    gridResizer.addEventListener("mousedown", (e) => {
      isDraggingGrid = true;
      gridResizer.classList.add("dragging");
      e.preventDefault();
    });

    window.addEventListener("mousemove", (e) => {
      if (!isDraggingGrid) return;
      const rect = editorGrid.getBoundingClientRect();
      if (splitOrientation === "horizontal") {
        const offset = e.clientX - rect.left;
        const pct = Math.max(15, Math.min(85, (offset / rect.width) * 100));
        pane1.style.flex = `0 0 ${pct}%`;
        pane2.style.flex = `0 0 ${100 - pct}%`;
      } else {
        const offset = e.clientY - rect.top;
        const pct = Math.max(15, Math.min(85, (offset / rect.height) * 100));
        pane1.style.flex = `0 0 ${pct}%`;
        pane2.style.flex = `0 0 ${100 - pct}%`;
      }
      editor1?.layout();
      editor2?.layout();
    });

    window.addEventListener("mouseup", () => {
      if (isDraggingGrid) {
        isDraggingGrid = false;
        gridResizer.classList.remove("dragging");
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

function closeQuickPick() {
  const modal = document.getElementById("quickpick-modal");
  modal?.classList.add("hidden");
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
    { title: "View: Close Split Editor (分割を閉じる)", shortcut: "", id: "close_split" },
    { title: "File: Save (ファイルの保存)", shortcut: "Ctrl+S", id: "save" },
    { title: "File: Save All (すべて保存)", shortcut: "Ctrl+K S", id: "save_all" },
    { title: "File: New File (新規ファイル作成)", shortcut: "Ctrl+N", id: "new_file" },
    { title: "File: Open File Dialog (ネイティブファイルを開く)", shortcut: "Ctrl+O", id: "open_file_dialog" },
    { title: "File: Open Folder Dialog (ネイティブフォルダーを開く)", shortcut: "Ctrl+K Ctrl+O", id: "open_folder_dialog" },
    { title: "File: Open Recent Workspace (最近使ったワークスペースを開く)", shortcut: "", id: "open_recent_workspace" },
    { title: "Workspace: Add Folder (フォルダーを追加)", shortcut: "", id: "add_workspace_folder" },
    { title: "Workspace: Remove Active Folder (アクティブルートを削除)", shortcut: "", id: "remove_workspace_folder" },
    { title: "Workspace: Select Folder (アクティブルートを選択)", shortcut: "", id: "select_workspace_folder" },
    { title: "Workspace: Manage Trust (ワークスペース信頼を管理)", shortcut: "", id: "manage_workspace_trust" },
    { title: "Workspace: Configure Excludes (除外規則を設定)", shortcut: "", id: "configure_workspace_excludes" },
    { title: "File: Save As Dialog (名前を付けて保存)", shortcut: "Ctrl+Shift+S", id: "save_as_dialog" },
    { title: "View: Toggle Side Bar (サイドバー切替)", shortcut: "Ctrl+B", id: "toggle_sidebar" },
    { title: "View: Toggle Terminal (ターミナル切替)", shortcut: "Ctrl+J", id: "toggle_terminal" },
    { title: "Terminal: New Terminal (新規ターミナル作成)", shortcut: "Ctrl+Shift+`", id: "new_terminal" },
    { title: "Tasks: Run Task (タスクの実行)", shortcut: "", id: "run_workspace_task" },
    { title: "Testing: Run Tests (テストを実行)", shortcut: "", id: "run_workspace_tests" },
    { title: "Git: Open SCM View (ソース管理を開く)", shortcut: "Ctrl+Shift+G", id: "open_scm" },
    { title: "Git: Switch Branch (ブランチ切り替え)", shortcut: "", id: "switch_branch" },
    { title: "View: Show Explorer (エクスプローラーを開く)", shortcut: "Ctrl+Shift+E", id: "open_explorer" },
    { title: "View: Show Search (検索を開く)", shortcut: "Ctrl+Shift+F", id: "open_search" },
    { title: "View: Show Extensions (拡張機能を開く)", shortcut: "Ctrl+Shift+X", id: "open_extensions" },
    { title: "Preferences: Open Settings (設定を開く)", shortcut: "Ctrl+,", id: "open_settings" },
    { title: "Editor: Go to Definition (定義へ移動)", shortcut: "F12", id: "goto_def" },
    { title: "Editor: Format Document (ドキュメントのフォーマット)", shortcut: "Shift+Alt+F", id: "format_doc" },
    { title: "Editor: Close Active Tab (タブを閉じる)", shortcut: "Ctrl+W", id: "close_tab" },
  ];

  if (query.startsWith(">")) {
    // 1. Command Mode (">")
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
  } else if (query.startsWith(":")) {
    // 2. Go to Line Mode (":")
    const lineNumStr = query.slice(1).trim();
    const lineNum = parseInt(lineNumStr, 10);
    if (!isNaN(lineNum) && lineNum > 0) {
      quickPickItems.push({
        id: `line_${lineNum}`,
        title: `📍 行 ${lineNum} へ移動`,
        action: () => {
          if (editor1) {
            editor1.revealLineInCenter(lineNum);
            editor1.setPosition({ lineNumber: lineNum, column: 1 });
          }
        },
      });
    } else {
      quickPickItems.push({
        id: "line_hint",
        title: "行番号を入力してください (例: :42)",
        action: () => {},
      });
    }
  } else if (query.startsWith("@")) {
    // 3. Document Symbol Mode ("@")
    const symQuery = query.slice(1).trim().toLowerCase();
    if (activeFilePath && openTabs.has(activeFilePath)) {
      const model = openTabs.get(activeFilePath)!.model;
      const text = model.getValue();
      const lines = text.split("\n");
      const symRegex = /^\s*(fn|function|pub fn|class|struct|enum|interface|type|const|let|var|def)\s+([A-Za-z0-9_]+)/;
      
      lines.forEach((line, idx) => {
        const m = line.match(symRegex);
        if (m) {
          const kind = m[1];
          const name = m[2];
          if (!symQuery || name.toLowerCase().includes(symQuery)) {
            quickPickItems.push({
              id: `sym_${idx}`,
              title: `🔹 ${name} (${kind})`,
              subtitle: `行 ${idx + 1}: ${line.trim()}`,
              action: () => {
                if (editor1) {
                  editor1.revealLineInCenter(idx + 1);
                  editor1.setPosition({ lineNumber: idx + 1, column: 1 });
                }
              },
            });
          }
        }
      });
      if (quickPickItems.length === 0) {
        quickPickItems.push({ id: "no_sym", title: "シンボルが見つかりませんでした", action: () => {} });
      }
    }
  } else {
    // 4. File Search Mode (No prefix)
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
    case "close_split":
      document.getElementById("btn-close-split")?.click();
      break;
    case "save":
      saveActiveFile();
      break;
    case "save_all":
      saveAllFiles();
      break;
    case "new_file":
      document.getElementById("btn-new-file")?.click();
      break;
    case "restore_closed_tab":
      restoreLastClosedTab();
      break;
    case "run":
      showStatusMessage("▶ 実行中 (Run)");
      break;
    case "rename_file":
      if (activeFilePath) {
        const tab = openTabs.get(activeFilePath);
        if (tab) promptRenameFile(tab.path);
      }
      break;
    case "go_to_line": {
      const lineStr = prompt("移動先の行番号を入力してください:");
      if (!lineStr) break;
      const lineNumber = parseInt(lineStr, 10);
      const editor = activeEditorPane === 2 && isSplitActive ? editor2 : editor1;
      if (!isNaN(lineNumber) && editor) {
        editor.revealLineInCenter(lineNumber);
        editor.setPosition({ lineNumber, column: 1 });
      }
      break;
    }
    case "open_file_dialog":
      openNativeFileDialog();
      break;
    case "open_folder_dialog":
      openNativeFolderDialog();
      break;
    case "open_recent_workspace":
      openRecentWorkspacePicker();
      break;
    case "add_workspace_folder":
      addWorkspaceFolder();
      break;
    case "remove_workspace_folder":
      removeActiveWorkspaceFolder();
      break;
    case "select_workspace_folder":
      openWorkspaceFolderPicker();
      break;
    case "manage_workspace_trust":
      toggleWorkspaceTrust();
      break;
    case "configure_workspace_excludes":
      configureWorkspaceExcludes();
      break;
    case "save_as_dialog":
      saveNativeFileDialog();
      break;
    case "toggle_sidebar":
      toggleSidebar();
      break;
    case "toggle_terminal":
      toggleTerminal();
      break;
    case "new_terminal":
      createNewTerminalSession();
      toggleTerminal(true);
      break;
    case "run_workspace_task":
      openWorkspaceTaskPicker();
      break;
    case "run_workspace_tests":
      openWorkspaceTestPicker();
      break;
    case "open_scm":
      document.querySelector<HTMLButtonElement>('[data-view="scm"]')?.click();
      break;
    case "switch_branch":
      document.getElementById("status-branch")?.click();
      break;
    case "open_explorer":
      document.querySelector<HTMLButtonElement>('[data-view="explorer"]')?.click();
      break;
    case "open_search":
      document.querySelector<HTMLButtonElement>('[data-view="search"]')?.click();
      break;
    case "open_extensions":
      document.querySelector<HTMLButtonElement>('[data-view="extensions"]')?.click();
      break;
    case "open_settings":
      document.querySelector<HTMLButtonElement>('[data-view="settings"]')?.click();
      break;
    case "quick_open":
      openQuickPick(false);
      break;
    case "command_palette":
      openQuickPick(true);
      break;
    case "goto_def":
      performGoToDefinition();
      break;
    case "format_doc":
      formatCurrentDocument();
      break;
    case "close_tab":
      if (activeFilePath) closeTab(activeFilePath);
      break;
  }
}

// Native Dialog Helpers (Issue #15)
async function openNativeFileDialog() {
  try {
    const selected = await openDialog({
      multiple: false,
      directory: false,
    });
    if (selected) {
      const p = typeof selected === "string" ? selected : selected;
      const name = p.split(/[\\/]/).pop() || p;
      await openFile(p, name);
    }
  } catch (e) {
    console.error("Open file dialog error:", e);
  }
}

async function openWorkspaceTestPicker() {
  try {
    const suites = await invoke<TestSuite[]>("list_workspace_test_suites");
    if (suites.length === 0) {
      showToast("テストランナーを検出できませんでした", "info");
      return;
    }
    quickPickItems = suites.map((suite) => ({
      id: `test:${suite.id}`,
      title: `Testing: ${suite.label}`,
      subtitle: [suite.command, ...suite.args].join(" "),
      action: () => runWorkspaceTestSuite(suite),
    }));
    quickPickSelectedIndex = 0;
    document.getElementById("quickpick-modal")?.classList.remove("hidden");
    const input = document.getElementById("quickpick-input") as HTMLInputElement | null;
    if (input) {
      input.value = "";
      input.placeholder = "実行するテストスイートを選択";
      input.focus();
    }
    renderQuickPickDom();
  } catch (error) {
    console.error("Failed to discover tests:", error);
    showToast(`テストを検出できませんでした: ${error}`, "error");
  }
}

async function runWorkspaceTestSuite(suite: TestSuite) {
  try {
    const result = await invoke<TaskExecutionResult>("run_workspace_test_suite", { id: suite.id });
    const output = document.getElementById("output-container");
    if (output) output.textContent = result.output || `${suite.label}: 出力はありません`;
    document.getElementById("panel-tab-output")?.click();
    showStatusMessage(`${suite.label}: ${result.exit_code === 0 ? "成功" : `終了コード ${result.exit_code ?? "不明"}`}`);
  } catch (error) {
    console.error("Failed to run tests:", error);
    showToast(`テストを実行できませんでした: ${error}`, "error");
  }
}

async function openWorkspaceTaskPicker() {
  try {
    const tasks = await invoke<TaskDefinition[]>("list_workspace_tasks");
    if (tasks.length === 0) {
      showToast(".oxide/tasks.json または .vscode/tasks.json に実行可能なタスクがありません", "info");
      return;
    }
    quickPickItems = tasks.map((task) => ({
      id: `task:${task.label}`,
      title: `$(tools) ${task.label}`,
      subtitle: [task.command, ...task.args].join(" "),
      action: () => runWorkspaceTask(task.label),
    }));
    quickPickSelectedIndex = 0;
    const modal = document.getElementById("quickpick-modal");
    const input = document.getElementById("quickpick-input") as HTMLInputElement | null;
    modal?.classList.remove("hidden");
    if (input) {
      input.value = "";
      input.placeholder = "実行するタスクを選択";
      input.focus();
    }
    renderQuickPickDom();
  } catch (error) {
    console.error("Failed to load workspace tasks:", error);
    showToast(`タスクを読み込めませんでした: ${error}`, "error");
  }
}

async function runWorkspaceTask(label: string) {
  try {
    const result = await invoke<TaskExecutionResult>("run_workspace_task", { label });
    const output = document.getElementById("output-container");
    if (output) output.textContent = result.output || `${label}: 出力はありません`;
    document.getElementById("panel-tab-output")?.click();
    showStatusMessage(`${label}: ${result.exit_code === 0 ? "成功" : `終了コード ${result.exit_code ?? "不明"}`}`);
  } catch (error) {
    console.error("Failed to run workspace task:", error);
    showToast(`タスクを実行できませんでした: ${error}`, "error");
  }
}

function updateWorkspaceDisplay(workspace: WorkspaceInfo) {
  const workspaceName = document.getElementById("workspace-name");
  const isTrusted = workspaceTrust?.root === workspace.root && workspaceTrust.trusted;
  const trustLabel = isTrusted ? "信頼済み" : "未信頼";
  if (workspaceName) {
    workspaceName.textContent = `${workspace.name} (${trustLabel})`;
    workspaceName.title = `${workspace.root}\nワークスペース: ${trustLabel}`;
  }
  document.title = `${workspace.name} - Oxide Editor`;
}

async function refreshWorkspaceState() {
  const [folders, trust] = await Promise.all([
    invoke<WorkspaceInfo[]>("get_workspace_folders"),
    invoke<WorkspaceTrust>("get_workspace_trust"),
  ]);
  workspaceFolders = folders;
  workspaceTrust = trust;
}

async function confirmWorkspaceTrust() {
  if (workspaceTrust?.trusted) return;
  const trusted = confirm(
    "このフォルダーは未信頼です。ターミナル、タスク、言語サーバー、拡張機能の実行はブロックされています。信頼しますか？",
  );
  if (trusted) {
    workspaceTrust = await invoke<WorkspaceTrust>("set_workspace_trust", { trusted: true });
    updateWorkspaceDisplay({ root: workspaceRoot, name: workspaceRoot.split(/[\\/]/).filter(Boolean).pop() || "workspace" });
    showStatusMessage("ワークスペースを信頼しました");
  }
}

async function switchWorkspace(workspace: WorkspaceInfo) {
  saveSessionState();
  try {
    await invoke<number>("lsp_stop_all");
  } catch (e) {
    console.warn("Failed to stop language servers while switching workspace:", e);
  }
  workspaceRoot = workspace.root;
  await refreshWorkspaceState();
  activeLspServers.clear();
  collapsedFolders.clear();

  openTabs.forEach((tab) => tab.model.dispose());
  openTabs.clear();
  activeFilePath = null;
  pane1FilePath = null;
  pane2FilePath = null;
  updateTabBar();
  updateWorkspaceDisplay(workspace);

  await loadWorkspaceFiles();
  await updateGitStatus();
  await restoreSessionState();
  await confirmWorkspaceTrust();
  showStatusMessage(`ワークスペースを開きました: ${workspace.root}`);
}

async function openNativeFolderDialog() {
  try {
    const selected = await openDialog({
      directory: true,
      multiple: false,
    });
    if (selected && typeof selected === "string") {
      const workspace = await invoke<WorkspaceInfo>("set_workspace_root", { path: selected });
      await switchWorkspace(workspace);
    }
  } catch (e) {
    console.error("Open folder dialog error:", e);
    showToast(`ワークスペースを開けませんでした: ${e}`, "error");
  }
}

async function openRecentWorkspacePicker() {
  try {
    const recent = await invoke<WorkspaceInfo[]>("list_recent_workspaces");
    if (recent.length === 0) {
      showToast("最近使用したワークスペースはありません", "info");
      return;
    }

    quickPickItems = recent.map((workspace) => ({
      id: `recent:${workspace.root}`,
      title: `📁 ${workspace.name}`,
      subtitle: workspace.root,
      action: async () => {
        const selected = await invoke<WorkspaceInfo>("set_workspace_root", { path: workspace.root });
        await switchWorkspace(selected);
      },
    }));
    quickPickSelectedIndex = 0;
    const modal = document.getElementById("quickpick-modal");
    const input = document.getElementById("quickpick-input") as HTMLInputElement | null;
    modal?.classList.remove("hidden");
    if (input) {
      input.value = "";
      input.placeholder = "最近使用したワークスペースを選択";
      input.focus();
    }
    renderQuickPickDom();
  } catch (e) {
    console.error("Failed to load recent workspaces:", e);
    showToast("最近使用したワークスペースを読み込めませんでした", "error");
  }
}

async function toggleWorkspaceTrust() {
  try {
    const currentTrust = workspaceTrust || (await invoke<WorkspaceTrust>("get_workspace_trust"));
    const nextTrusted = !currentTrust.trusted;
    if (
      nextTrusted &&
      !confirm(
        "このフォルダーを信頼すると、ターミナル、タスク、言語サーバー、拡張機能がこのフォルダーのコードを実行できるようになります。続行しますか？",
      )
    ) {
      return;
    }

    workspaceTrust = await invoke<WorkspaceTrust>("set_workspace_trust", { trusted: nextTrusted });
    updateWorkspaceDisplay({
      root: workspaceRoot,
      name: workspaceRoot.split(/[\\/]/).filter(Boolean).pop() || "workspace",
    });
    showStatusMessage(nextTrusted ? "ワークスペースを信頼しました" : "ワークスペースの信頼を取り消しました");
  } catch (error) {
    console.error("Failed to update workspace trust:", error);
    showToast(`ワークスペースの信頼を更新できませんでした: ${error}`, "error");
  }
}

async function addWorkspaceFolder() {
  try {
    const selected = await openDialog({ directory: true, multiple: false });
    if (!selected || typeof selected !== "string") return;

    workspaceFolders = await invoke<WorkspaceInfo[]>("add_workspace_folder", { path: selected });
    showStatusMessage(`ワークスペースフォルダーを追加しました: ${selected}`);
  } catch (error) {
    console.error("Failed to add workspace folder:", error);
    showToast(`ワークスペースフォルダーを追加できませんでした: ${error}`, "error");
  }
}

async function removeActiveWorkspaceFolder() {
  if (workspaceFolders.length <= 1) {
    showToast("最後のワークスペースフォルダーは削除できません", "error");
    return;
  }
  if (!confirm(`アクティブなワークスペースフォルダーを削除しますか？\n${workspaceRoot}`)) return;

  try {
    workspaceFolders = await invoke<WorkspaceInfo[]>("remove_workspace_folder", { path: workspaceRoot });
    const root = await invoke<string>("get_workspace_path");
    const nextWorkspace = workspaceFolders.find((folder) => folder.root === root);
    if (!nextWorkspace) throw new Error("次のアクティブなワークスペースが見つかりません");
    await switchWorkspace(nextWorkspace);
  } catch (error) {
    console.error("Failed to remove workspace folder:", error);
    showToast(`ワークスペースフォルダーを削除できませんでした: ${error}`, "error");
  }
}

async function openWorkspaceFolderPicker() {
  try {
    await refreshWorkspaceState();
    quickPickItems = workspaceFolders.map((folder) => ({
      id: `workspace-folder:${folder.root}`,
      title: `フォルダー: ${folder.name}`,
      subtitle: folder.root === workspaceRoot ? `${folder.root} (アクティブ)` : folder.root,
      action: async () => {
        const selected = await invoke<WorkspaceInfo>("select_workspace_folder", { path: folder.root });
        await switchWorkspace(selected);
      },
    }));
    quickPickSelectedIndex = 0;
    const modal = document.getElementById("quickpick-modal");
    const input = document.getElementById("quickpick-input") as HTMLInputElement | null;
    modal?.classList.remove("hidden");
    if (input) {
      input.value = "";
      input.placeholder = "アクティブにするワークスペースフォルダーを選択";
      input.focus();
    }
    renderQuickPickDom();
  } catch (error) {
    console.error("Failed to select workspace folder:", error);
    showToast(`ワークスペースフォルダーを読み込めませんでした: ${error}`, "error");
  }
}

function parseExcludePatterns(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((pattern) => pattern.trim())
    .filter(Boolean);
}

async function configureWorkspaceExcludes() {
  try {
    const current = await invoke<WorkspaceExcludes>("get_workspace_excludes");
    const files = prompt(
      "ファイルツリーから除外するパターンをカンマまたは改行で区切って入力してください。例: generated, *.tmp",
      current.files.join(", "),
    );
    if (files === null) return;
    const search = prompt(
      "検索・置換から追加で除外するパターンをカンマまたは改行で区切って入力してください。例: *.snapshot",
      current.search.join(", "),
    );
    if (search === null) return;

    await invoke<WorkspaceExcludes>("set_workspace_excludes", {
      files: parseExcludePatterns(files),
      search: parseExcludePatterns(search),
    });
    await loadWorkspaceFiles();
    showStatusMessage("ワークスペースの除外規則を更新しました");
  } catch (error) {
    console.error("Failed to update workspace excludes:", error);
    showToast(`ワークスペースの除外規則を更新できませんでした: ${error}`, "error");
  }
}

async function saveNativeFileDialog() {
  if (!activeFilePath || !openTabs.has(activeFilePath)) return;
  const tab = openTabs.get(activeFilePath)!;
  try {
    const savePath = await saveDialog({
      defaultPath: tab.name,
    });
    if (savePath) {
      await invoke("write_file_content", { path: savePath, content: tab.model.getValue() });
      tab.isDirty = false;
      updateTabBar();
      showStatusMessage(`保存完了: ${savePath}`);
      await loadWorkspaceFiles();
    }
  } catch (e) {
    console.error("Save as dialog error:", e);
  }
}

// Interactive Status Bar (Issue #17)
function setupStatusBarInteractions() {
  const statusLanguage = document.getElementById("status-language");
  const statusIndent = document.getElementById("status-indent");
  const statusEncoding = document.getElementById("status-encoding");
  const statusEol = document.getElementById("status-eol");
  const statusLineCol = document.getElementById("status-line-col");
  const statusProblems = document.getElementById("status-problems");

  statusLanguage?.addEventListener("click", () => {
    quickPickItems = [
      "rust", "typescript", "javascript", "python", "go", "html", "css", "json", "markdown", "plaintext"
    ].map((lang) => ({
      id: lang,
      title: lang.toUpperCase(),
      action: () => {
        if (activeFilePath && openTabs.has(activeFilePath)) {
          const tab = openTabs.get(activeFilePath)!;
          monaco.editor.setModelLanguage(tab.model, lang);
          updateStatusBar(activeFilePath);
          applyStoredSettings();
        }
      },
    }));
    openQuickPick(false);
    renderQuickPickDom();
  });

  statusIndent?.addEventListener("click", () => {
    quickPickItems = [
      { id: "s2", title: "インデント: スペース 2", action: () => { editor1?.updateOptions({ tabSize: 2 }); if (statusIndent) statusIndent.textContent = "スペース: 2"; } },
      { id: "s4", title: "インデント: スペース 4", action: () => { editor1?.updateOptions({ tabSize: 4 }); if (statusIndent) statusIndent.textContent = "スペース: 4"; } },
      { id: "t4", title: "インデント: タブ 4", action: () => { editor1?.updateOptions({ tabSize: 4, insertSpaces: false }); if (statusIndent) statusIndent.textContent = "タブ: 4"; } },
    ];
    openQuickPick(false);
    renderQuickPickDom();
  });

  statusEncoding?.addEventListener("click", () => {
    quickPickItems = [
      { id: "utf8", title: "UTF-8 (エンコード付きで再度開く)", action: () => showStatusMessage("エンコーディング: UTF-8") },
      { id: "sjis", title: "Shift-JIS", action: () => showStatusMessage("エンコーディング: Shift-JIS") },
      { id: "euc", title: "EUC-JP", action: () => showStatusMessage("エンコーディング: EUC-JP") },
    ];
    openQuickPick(false);
    renderQuickPickDom();
  });

  statusEol?.addEventListener("click", () => {
    quickPickItems = [
      { id: "crlf", title: "CRLF (\\r\\n)", action: () => { if (statusEol) statusEol.textContent = "CRLF"; } },
      { id: "lf", title: "LF (\\n)", action: () => { if (statusEol) statusEol.textContent = "LF"; } },
    ];
    openQuickPick(false);
    renderQuickPickDom();
  });

  statusLineCol?.addEventListener("click", () => {
    openQuickPick(false);
    const input = document.getElementById("quickpick-input") as HTMLInputElement;
    if (input) {
      input.value = ":";
      fetchAndRenderQuickPick(":");
    }
  });

  statusProblems?.addEventListener("click", () => {
    toggleTerminal(true);
  });
}

// 20. Global Shortcuts & Status Bar
function setupShortcuts() {
  window.addEventListener("keydown", (e) => {
    const target = e.target as HTMLElement | null;
    const isTextInput =
      (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || target?.isContentEditable) &&
      !target?.classList.contains("inputarea");
    const command = isTextInput ? null : commandForEvent(e);
    if (command) {
      e.preventDefault();
      executeCommand(command);
      return;
    }
    if (e.key === "Escape") {
      closeGlobalMenu();
      closeQuickPick();
      document.getElementById("confirm-modal")?.classList.add("hidden");
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

// Prevent default browser context menu globally
window.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement;
  if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) {
    return;
  }
  e.preventDefault();
});


function openExtensionDetail(ext: OpenVsxExtension, isInstalled: boolean) {
  const modal = document.getElementById("ext-detail-modal");
  if (!modal) return;
  
  const icon = document.getElementById("ext-detail-icon") as HTMLImageElement;
  const title = document.getElementById("ext-detail-title");
  const id = document.getElementById("ext-detail-id");
  const desc = document.getElementById("ext-detail-desc");
  const readme = document.getElementById("ext-detail-readme");
  
  const installBtn = document.getElementById("ext-detail-install-btn") as HTMLButtonElement;
  const uninstallBtn = document.getElementById("ext-detail-uninstall-btn") as HTMLButtonElement;
  const closeBtn = document.getElementById("ext-detail-close") as HTMLButtonElement;
  
  icon.src = ext.icon_url || "https://via.placeholder.com/72?text=Ext";
  if (title) title.textContent = ext.display_name || ext.name;
  if (id) id.textContent = `${ext.namespace}.${ext.name} v${ext.version}`;
  if (desc) desc.textContent = ext.description || "";
  if (readme) readme.innerHTML = "Fetching README...";
  
  if (isInstalled) {
    installBtn.classList.add("hidden");
    uninstallBtn.classList.remove("hidden");
  } else {
    installBtn.classList.remove("hidden");
    uninstallBtn.classList.add("hidden");
    installBtn.textContent = "インストール";
    installBtn.disabled = false;
  }
  
  installBtn.onclick = async () => {
    installBtn.textContent = "インストール中...";
    installBtn.disabled = true;
    try {
      const res = await invoke<string>("install_openvsx_extension", {
        namespace: ext.namespace,
        name: ext.name,
        version: ext.version,
        description: ext.description || "",
        downloadUrl: ext.download_url || null,
      });
      showStatusMessage(res);
      installBtn.textContent = "✓ インストール済み";
      uninstallBtn.classList.remove("hidden");
      installBtn.classList.add("hidden");
      
      if (ext.name.includes("rust")) ensureLspServerStarted("rust");
      if (ext.name.includes("python")) ensureLspServerStarted("python");
      if (ext.name.includes("go")) ensureLspServerStarted("go");
    } catch (err) {
      alert(`エラー: ${err}`);
      installBtn.textContent = "インストール";
      installBtn.disabled = false;
    }
  };
  
  uninstallBtn.onclick = async () => {
    uninstallBtn.textContent = "アンインストール中...";
    uninstallBtn.disabled = true;
    try {
      const res = await invoke<string>("uninstall_extension", {
        id: `${ext.namespace}.${ext.name}`
      });
      showStatusMessage(res);
      installBtn.classList.remove("hidden");
      uninstallBtn.classList.add("hidden");
    } catch (err) {
      alert(`アンインストール失敗: ${err}`);
      uninstallBtn.textContent = "アンインストール";
      uninstallBtn.disabled = false;
    }
  };
  
  closeBtn.onclick = () => {
    modal.classList.add("hidden");
  };
  
  modal.classList.remove("hidden");
  
  if (ext.url) {
    fetch(ext.url).then(r => r.json()).then(data => {
      if (readme) {
        readme.innerHTML = `<div style="padding: 10px;">
          <h3>${ext.display_name || ext.name}</h3>
          <p>${ext.description || ''}</p>
          <hr>
          <p>Repository: ${data.repository || 'N/A'}</p>
          <p>License: ${data.license || 'N/A'}</p>
          <p>Downloads: ${ext.download_count}</p>
        </div>`;
      }
    }).catch(() => {
      if (readme) readme.innerHTML = "Failed to load details.";
    });
  } else {
    if (readme) readme.innerHTML = "No additional details available.";
  }
}
