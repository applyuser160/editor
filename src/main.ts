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

interface OpenTab {
  path: string;
  name: string;
  model: monaco.editor.ITextModel;
  isDirty: boolean;
}

// Global State
let editor1: monaco.editor.IStandaloneCodeEditor | null = null;
let editor2: monaco.editor.IStandaloneCodeEditor | null = null;
let isSplitActive = false;
let splitOrientation: "horizontal" | "vertical" = "horizontal";

let ptyId: number | null = null;
let xtermInstance: Terminal | null = null;
let fitAddon: FitAddon | null = null;

let quickPickItems: Array<{ id: string; title: string; subtitle?: string; shortcut?: string; action: () => void }> = [];
let quickPickSelectedIndex = 0;

const openTabs: Map<string, OpenTab> = new Map();
let activeFilePath: string | null = null;
let currentActiveView = "explorer";
let isSidebarVisible = true;
let isTerminalVisible = true;

// Initialize when DOM is ready
window.addEventListener("DOMContentLoaded", () => {
  initMonacoEditors();
  setupActivityBar();
  setupResizers();
  setupGridSplitters();
  setupIntegratedTerminal();
  setupQuickPick();
  setupShortcuts();
  setupFileActions();
  setupFileWatcherListener();
  initExtensionHost();
  loadWorkspaceFiles();
});

// 1. Initialize Monaco Editors (Primary & Secondary Split)
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

fn main() {
    let message = "Hello from VS Code on Tauri v2 (Oxide)!";
    println!("{}", message);
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
  };

  editor1 = monaco.editor.create(container1, {
    ...commonOptions,
    model: initialModel,
  });

  editor2 = monaco.editor.create(container2, {
    ...commonOptions,
    model: initialModel,
  });

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

  openTabs.set("welcome.rs", {
    path: "welcome.rs",
    name: "welcome.rs",
    model: initialModel,
    isDirty: false,
  });
  activeFilePath = "welcome.rs";
  updateTabBar();
}

// 2. 2D Grid Splitter Actions
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

// 3. Integrated Real-time Terminal (xterm.js + Portable-PTY)
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

// 4. File Watcher Real-time Sync
async function setupFileWatcherListener() {
  await listen<{ paths: string[]; kind: string }>("fs-change", async (event) => {
    // Refresh workspace file tree
    if (currentActiveView === "explorer") {
      loadWorkspaceFiles();
    } else if (currentActiveView === "scm") {
      const contentEl = document.getElementById("sidebar-content");
      if (contentEl) renderScmView(contentEl);
    }

    // Auto-reload active file if modified externally and not dirty
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

// 5. Extension Host Initialization
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

// 6. Open / Switch Files
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
      }
    });

    openTabs.set(path, { path, name, model, isDirty: false });
    activeFilePath = path;
    editor1.setModel(model);
    if (isSplitActive && editor2) {
      editor2.setModel(model);
    }

    updateTabBar();
    updateStatusBar(path);
    showStatusMessage(`開きました: ${name}`);
  } catch (err) {
    showStatusMessage(`エラー: ファイルを開けませんでした (${err})`);
  }
}

// 7. Save Active File (Ctrl+S)
async function saveActiveFile() {
  if (!activeFilePath || !editor1) return;
  const tab = openTabs.get(activeFilePath);
  if (!tab) return;

  const content = tab.model.getValue();
  try {
    await invoke("write_file_content", { path: tab.path, content });
    tab.isDirty = false;
    updateTabBar();
    showStatusMessage(`保存完了: ${tab.name}`);
  } catch (err) {
    showStatusMessage(`保存失敗: ${err}`);
  }
}

// 8. Tab Bar Rendering
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
    case "sh": return "shell";
    default: return "plaintext";
  }
}

// 9. Activity Bar & All Sidebar Views
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
      titleEl.textContent = "拡張機能 (EXTENSIONS)";
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

// 10. Extensions Viewlet
async function renderExtensionsView(container: HTMLElement) {
  try {
    const exts = await invoke<ExtensionManifest[]>("get_installed_extensions");
    container.innerHTML = `<div style="padding: 6px;" id="ext-list-box"></div>`;
    const box = document.getElementById("ext-list-box");
    if (!box) return;

    exts.forEach((ext) => {
      const card = document.createElement("div");
      card.style.cssText = "padding: 8px; background: #2a2d2e; border-radius: 4px; margin-bottom: 8px;";
      card.innerHTML = `
        <div style="font-weight: bold; color: #fff; display: flex; justify-content: space-between;">
          <span>${ext.name}</span>
          <span style="font-size: 10px; color: #888;">v${ext.version}</span>
        </div>
        <div style="font-size: 11px; color: #aaa; margin-top: 2px;">${ext.description}</div>
        <div style="font-size: 10px; color: #00ff80; margin-top: 4px;">● 有効 (Installed & Active)</div>
      `;
      box.appendChild(card);
    });
  } catch (e) {
    container.innerHTML = `<div style="color: #888; padding: 8px;">拡張機能読込エラー: ${e}</div>`;
  }
}

// 11. Search Feature Integration
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

// 12. SCM (Git) Integration
async function renderScmView(container: HTMLElement) {
  try {
    const status = await invoke<GitStatusResult>("git_get_status");
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

// 13. Settings Handlers
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

// 14. Workspace File Tree Loading & Actions
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

// 15. Draggable Splitter Resizers
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

// 16. QuickPick Modal with Keyboard Navigation (Arrow Keys + Enter)
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
  const modal = document.getElementById("quickpick-modal");
  const input = document.getElementById("quickpick-input") as HTMLInputElement;
  if (!modal || !input) return;

  modal.classList.remove("hidden");
  input.value = isCommandMode ? "> " : "";
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
  const panelPart = document.getElementById("panel-part");
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
      if (panelPart) {
        isTerminalVisible = !isTerminalVisible;
        panelPart.style.display = isTerminalVisible ? "flex" : "none";
        editor1?.layout();
        editor2?.layout();
        fitAddon?.fit();
      }
      break;
    case "open_scm":
      document.querySelector<HTMLButtonElement>('[data-view="scm"]')?.click();
      break;
  }
}

// 17. Global Shortcuts & Status Bar
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
      const panelPart = document.getElementById("panel-part");
      if (panelPart) {
        isTerminalVisible = !isTerminalVisible;
        panelPart.style.display = isTerminalVisible ? "flex" : "none";
        editor1?.layout();
        editor2?.layout();
        fitAddon?.fit();
      }
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
