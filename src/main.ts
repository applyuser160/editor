import { invoke } from "@tauri-apps/api/core";

interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  depth: number;
}

// State
let currentActiveView = "explorer";
let isSidebarVisible = true;
let isTerminalVisible = true;

// Initialize on DOM loaded
window.addEventListener("DOMContentLoaded", () => {
  setupActivityBar();
  setupResizers();
  setupTerminal();
  setupQuickPick();
  setupShortcuts();
  loadWorkspaceFiles();
});

// 1. Activity Bar Navigation
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
          <div style="margin-bottom: 8px;">フォントサイズ: 14px</div>
          <div style="margin-bottom: 8px;">タブサイズ: 4 spaces</div>
          <div>ミニマップ: 有効</div>
        </div>
      `;
      break;
  }
}

// 2. Load Workspace Files via Tauri IPC
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

async function openFile(path: string, name: string) {
  try {
    const content = await invoke<string>("read_file_content", { path });
    const codeView = document.getElementById("code-view");
    if (codeView) {
      codeView.textContent = content;
    }

    const tabTitle = document.querySelector(".tab-title");
    if (tabTitle) {
      tabTitle.textContent = name;
    }

    const status = document.getElementById("global-status");
    if (status) {
      status.textContent = `開きました: ${name}`;
    }
  } catch (err) {
    console.error("Failed to open file:", err);
  }
}

// 3. Draggable Splitter Resizers
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

// 4. Terminal Command Execution via Tauri IPC
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

// 5. QuickPick & Command Palette (Ctrl+P / Ctrl+Shift+P)
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

function renderQuickPickItems(query: string) {
  const list = document.getElementById("quickpick-list");
  if (!list) return;

  list.innerHTML = "";

  const commands = [
    { title: "File: Save (ファイルの保存)", shortcut: "Ctrl+S", id: "save" },
    { title: "File: New File (新規ファイル作成)", shortcut: "Ctrl+N", id: "new_file" },
    { title: "View: Toggle Side Bar (サイドバー切替)", shortcut: "Ctrl+B", id: "toggle_sidebar" },
    { title: "View: Toggle Terminal (ターミナル切替)", shortcut: "Ctrl+J", id: "toggle_terminal" },
    { title: "Git: Commit (変更をコミット)", shortcut: "", id: "git_commit" },
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
    const item = document.createElement("div");
    item.className = "quickpick-item";
    item.innerHTML = `<span>📄 sample.rs</span><span style="color: #888;">src/sample.rs</span>`;
    item.onclick = () => {
      document.getElementById("quickpick-modal")?.classList.add("hidden");
      openFile("sample.rs", "sample.rs");
    };
    list.appendChild(item);
  }
}

function executeCommand(id: string) {
  const panelPart = document.getElementById("panel-part");
  switch (id) {
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

// 6. Keyboard Shortcuts
function setupShortcuts() {
  window.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.shiftKey && e.key === "P") {
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
