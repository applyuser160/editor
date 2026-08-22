import { invoke } from "@tauri-apps/api/core";
import * as monaco from "monaco-editor";

interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  depth: number;
}

interface OpenTab {
  path: string;
  name: string;
  model: monaco.editor.ITextModel;
  isDirty: boolean;
}

// Global State
let editorInstance: monaco.editor.IStandaloneCodeEditor | null = null;
const openTabs: Map<string, OpenTab> = new Map();
let activeFilePath: string | null = null;
let currentActiveView = "explorer";
let isSidebarVisible = true;
let isTerminalVisible = true;

// Initialize when DOM is ready
window.addEventListener("DOMContentLoaded", () => {
  initMonacoEditor();
  setupActivityBar();
  setupResizers();
  setupTerminal();
  setupQuickPick();
  setupShortcuts();
  setupFileActions();
  loadWorkspaceFiles();
});

// 1. Initialize Monaco Editor (VS Code Dark+ Theme)
function initMonacoEditor() {
  const container = document.getElementById("editor-container");
  if (!container) return;

  // Clear placeholder
  container.innerHTML = "";

  // Define VS Code Dark+ Theme
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

  editorInstance = monaco.editor.create(container, {
    model: initialModel,
    theme: "vscode-dark-plus",
    fontSize: 14,
    fontFamily: "Consolas, 'Courier New', monospace",
    lineNumbers: "on",
    roundedSelection: false,
    scrollBeyondLastLine: false,
    readOnly: false,
    cursorBlinking: "smooth",
    smoothScrolling: true,
    minimap: {
      enabled: true,
      scale: 1,
      showSlider: "mouseover",
    },
    automaticLayout: true,
    tabSize: 4,
    insertSpaces: true,
  });

  // Track cursor position
  editorInstance.onDidChangeCursorPosition((e) => {
    const statusLineCol = document.getElementById("status-line-col");
    if (statusLineCol) {
      statusLineCol.textContent = `行: ${e.position.lineNumber}, 列: ${e.position.column}`;
    }
  });

  // Track content change (dirty flag)
  initialModel.onDidChangeContent(() => {
    if (activeFilePath && openTabs.has(activeFilePath)) {
      const tab = openTabs.get(activeFilePath)!;
      tab.isDirty = true;
      updateTabBar();
    }
  });

  // Register default tab
  openTabs.set("welcome.rs", {
    path: "welcome.rs",
    name: "welcome.rs",
    model: initialModel,
    isDirty: false,
  });
  activeFilePath = "welcome.rs";
  updateTabBar();
}

// 2. Open / Switch Files in Editor
async function openFile(path: string, name: string) {
  if (!editorInstance) return;

  // Check if already open
  if (openTabs.has(path)) {
    activeFilePath = path;
    const tab = openTabs.get(path)!;
    editorInstance.setModel(tab.model);
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

    openTabs.set(path, {
      path,
      name,
      model,
      isDirty: false,
    });

    activeFilePath = path;
    editorInstance.setModel(model);
    updateTabBar();
    updateStatusBar(path);

    showStatusMessage(`開きました: ${name}`);
  } catch (err) {
    showStatusMessage(`エラー: ファイルを開けませんでした (${err})`);
  }
}

// 3. Save Active File (Ctrl+S)
async function saveActiveFile() {
  if (!activeFilePath || !editorInstance) return;
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

// 4. Tab Bar Rendering
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
      if (editorInstance) {
        const emptyModel = monaco.editor.createModel("", "plaintext");
        editorInstance.setModel(emptyModel);
      }
    }
  }
  updateTabBar();
}

function getLanguageFromPath(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "rs":
      return "rust";
    case "ts":
      return "typescript";
    case "js":
      return "javascript";
    case "json":
      return "json";
    case "md":
      return "markdown";
    case "toml":
      return "ini";
    case "html":
      return "html";
    case "css":
      return "css";
    case "py":
      return "python";
    case "sh":
      return "shell";
    default:
      return "plaintext";
  }
}

// 5. Activity Bar & Sidebar View
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
}

function updateSidebarView(view: string) {
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
          <input type="text" id="search-input" placeholder="検索語を入力..." style="width: 100%; padding: 4px 8px; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px;" />
          <div style="margin-top: 8px; color: #888; font-size: 11px;">プロジェクト内のファイルを検索します</div>
        </div>
      `;
      break;
    case "scm":
      titleEl.textContent = "ソース管理 (GIT)";
      contentEl.innerHTML = `
        <div style="padding: 4px;">
          <div style="font-size: 11px; color: #888; margin-bottom: 4px;">コミットメッセージ:</div>
          <textarea id="git-commit-msg" rows="2" style="width: 100%; background: #3c3c3c; border: 1px solid #555; color: #fff; border-radius: 4px; padding: 4px;"></textarea>
          <button style="margin-top: 6px; width: 100%; padding: 4px; background: #007acc; border: none; color: #fff; border-radius: 4px; cursor: pointer;">コミット実行</button>
        </div>
      `;
      break;
    case "extensions":
      titleEl.textContent = "拡張機能 (EXTENSIONS)";
      contentEl.innerHTML = `
        <div style="padding: 4px;">
          <div style="font-weight: bold; margin-bottom: 4px;">rust-analyzer</div>
          <div style="font-size: 11px; color: #888;">Rust code completion and diagnostics</div>
        </div>
      `;
      break;
    case "settings":
      titleEl.textContent = "設定 (SETTINGS)";
      contentEl.innerHTML = `
        <div style="padding: 4px;">
          <div style="margin-bottom: 8px;">エディタフォント: Consolas (14px)</div>
          <div style="margin-bottom: 8px;">テーマ: VS Code Dark+</div>
          <div>ミニマップ: 有効</div>
        </div>
      `;
      break;
  }
}

// 6. Workspace File Tree Loading & Actions
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
      node.innerHTML = `<span>${icon}</span><span>${file.name}</span>`;

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

// 7. Draggable Resizers
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
    });
  }
}

// 8. Terminal Command Execution
function setupTerminal() {
  const input = document.getElementById("term-input") as HTMLInputElement;
  const output = document.getElementById("terminal-output");

  if (input && output) {
    input.addEventListener("keydown", async (e) => {
      if (e.key === "Enter") {
        const cmd = input.value.trim();
        if (!cmd) return;

        appendTermLine(`PS > ${cmd}`);
        input.value = "";

        if (cmd === "clear" || cmd === "cls") {
          output.innerHTML = "";
          return;
        }

        try {
          const res = await invoke<string>("execute_terminal_command", { command: cmd });
          if (res) {
            res.split("\n").forEach((line) => {
              if (line.trim()) appendTermLine(line);
            });
          }
        } catch (err) {
          appendTermLine(`エラー: ${err}`);
        }
      }
    });
  }
}

function appendTermLine(text: string) {
  const output = document.getElementById("terminal-output");
  if (!output) return;

  const line = document.createElement("div");
  line.className = "term-line";
  line.textContent = text;
  output.appendChild(line);

  const container = document.getElementById("terminal-container");
  if (container) container.scrollTop = container.scrollHeight;
}

// 9. QuickPick Modal (Ctrl+P / Ctrl+Shift+P)
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
  renderQuickPickItems(input.value);

  input.oninput = () => {
    renderQuickPickItems(input.value);
  };
}

async function renderQuickPickItems(query: string) {
  const list = document.getElementById("quickpick-list");
  if (!list) return;

  list.innerHTML = "";

  const commands = [
    { title: "File: Save (ファイルの保存)", shortcut: "Ctrl+S", id: "save" },
    { title: "File: New File (新規ファイル作成)", shortcut: "Ctrl+N", id: "new_file" },
    { title: "View: Toggle Side Bar (サイドバー切替)", shortcut: "Ctrl+B", id: "toggle_sidebar" },
    { title: "View: Toggle Terminal (ターミナル切替)", shortcut: "Ctrl+J", id: "toggle_terminal" },
    { title: "Git: Refresh (状態更新)", shortcut: "", id: "git_refresh" },
  ];

  if (query.startsWith(">")) {
    const q = query.slice(1).trim().toLowerCase();
    commands
      .filter((c) => !q || c.title.toLowerCase().includes(q))
      .forEach((c) => {
        const item = document.createElement("div");
        item.className = "quickpick-item";
        item.innerHTML = `<span>${c.title}</span><span style="color: #888;">${c.shortcut}</span>`;
        item.onclick = () => {
          document.getElementById("quickpick-modal")?.classList.add("hidden");
          executeCommand(c.id);
        };
        list.appendChild(item);
      });
  } else {
    // File search mode
    try {
      const files = await invoke<FileEntry[]>("list_workspace_files");
      const q = query.trim().toLowerCase();
      files
        .filter((f) => !f.is_dir && (!q || f.name.toLowerCase().includes(q) || f.path.toLowerCase().includes(q)))
        .forEach((f) => {
          const item = document.createElement("div");
          item.className = "quickpick-item";
          item.innerHTML = `<span>📄 ${f.name}</span><span style="color: #888;">${f.path}</span>`;
          item.onclick = () => {
            document.getElementById("quickpick-modal")?.classList.add("hidden");
            openFile(f.path, f.name);
          };
          list.appendChild(item);
        });
    } catch (e) {
      console.error(e);
    }
  }
}

function executeCommand(id: string) {
  const panelPart = document.getElementById("panel-part");
  switch (id) {
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
      }
      break;
  }
}

// 10. Global Shortcuts & Status Bar
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
