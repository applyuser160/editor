use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, Layout, RichText, ScrollArea, TextStyle, Vec2,
};
use editor_core::TextBuffer;
use editor_git::{GitFileStatus, GitManager};
use editor_markdown::MarkdownPreview;
use editor_plugin::{PluginCapability, PluginCommand, PluginHost, PluginManifest};
use editor_search::{FuzzyFinder, SearchEngine, SearchQuery};
use editor_syntax::{LanguageId, SyntaxEngine, TokenType};
use editor_workspace::{Tab, Workspace};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityView {
    Explorer,
    Search,
    SourceControl,
    Extensions,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuickPickMode {
    Files,    // Ctrl+P
    Commands, // Ctrl+Shift+P
}

#[derive(Debug, Clone)]
struct CommandItem {
    id: String,
    title: String,
    category: String,
    shortcut: Option<String>,
}

struct OxideGuiApp {
    // Core states
    buffer_text: String,
    core_buffer: TextBuffer,
    workspace: Workspace,
    syntax_engine: SyntaxEngine,
    git_manager: GitManager,
    plugin_host: PluginHost,

    // UI States
    active_view: ActivityView,
    show_sidebar: bool,
    show_terminal: bool,
    show_markdown_preview: bool,
    show_minimap: bool,
    show_split_editor: bool,
    search_query: String,
    search_results_count: usize,
    commit_message: String,
    terminal_input: String,
    terminal_logs: Vec<String>,
    current_file_path: Option<PathBuf>,
    status_message: String,

    // QuickPick / Command Palette (Ctrl+P / Ctrl+Shift+P)
    show_quick_pick: bool,
    quick_pick_mode: QuickPickMode,
    quick_pick_query: String,
    available_commands: Vec<CommandItem>,

    // File creation state
    show_new_file_dialog: bool,
    show_new_folder_dialog: bool,
    show_save_as_dialog: bool,
    new_item_name: String,

    // Settings
    font_size: f32,
    tab_size: usize,
    terminal_height: f32,
}

impl OxideGuiApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        Self::setup_japanese_fonts(&cc.egui_ctx);

        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(Color32::from_rgb(212, 212, 212));
        visuals.panel_fill = Color32::from_rgb(30, 30, 30);
        visuals.window_fill = Color32::from_rgb(37, 37, 38);
        cc.egui_ctx.set_visuals(visuals);

        let current_dir = std::env::current_dir().unwrap_or_default();
        let workspace = Workspace::new(Some(current_dir.clone()));
        let git_manager = GitManager::new(current_dir);

        let mut plugin_host = PluginHost::new();
        let _ = plugin_host.register_plugin(PluginManifest {
            name: "rust-analyzer-helper".to_string(),
            version: "0.1.0".to_string(),
            description: "Rust code completion, diagnostics and semantic tokens engine".to_string(),
            entrypoint: "plugin.wasm".to_string(),
            capabilities: vec![PluginCapability::BufferRead, PluginCapability::BufferWrite],
            commands: vec![PluginCommand {
                id: "rust.format".to_string(),
                title: "Format Document (Rust)".to_string(),
                category: Some("Rust".to_string()),
            }],
        });

        let default_code = r#"// 🦀 Microsoft VS Code (microsoft/vscode) の Rust 移植版: Oxide Editor
// 高速・省メモリ・ネイティブGPUレンダリング・完全VS Code互換IDE

fn main() {
    println!("Hello from VS Code on Rust (Oxide)!");
    let mut numbers = vec![1, 2, 3, 4, 5];
    numbers.push(6);
    println!("Numbers: {:?}", numbers);
}
"#;

        let (buffer_text, current_file_path) = if let Some(ref path) = initial_file {
            if let Ok(content) = std::fs::read_to_string(path) {
                (content, Some(path.clone()))
            } else {
                (default_code.to_string(), Some(path.clone()))
            }
        } else {
            (default_code.to_string(), None)
        };

        let core_buffer = TextBuffer::from_str(&buffer_text);

        let available_commands = vec![
            CommandItem {
                id: "workbench.action.files.save".to_string(),
                title: "File: Save (ファイルの保存)".to_string(),
                category: "File".to_string(),
                shortcut: Some("Ctrl+S".to_string()),
            },
            CommandItem {
                id: "workbench.action.files.newFile".to_string(),
                title: "File: New File (新規ファイル作成)".to_string(),
                category: "File".to_string(),
                shortcut: Some("Ctrl+N".to_string()),
            },
            CommandItem {
                id: "workbench.action.toggleSidebarVisibility".to_string(),
                title: "View: Toggle Primary Side Bar (サイドバーの切替)".to_string(),
                category: "View".to_string(),
                shortcut: Some("Ctrl+B".to_string()),
            },
            CommandItem {
                id: "workbench.action.terminal.toggleTerminal".to_string(),
                title: "View: Toggle Terminal (ターミナルの切替)".to_string(),
                category: "View".to_string(),
                shortcut: Some("Ctrl+J".to_string()),
            },
            CommandItem {
                id: "workbench.action.splitEditor".to_string(),
                title: "View: Split Editor (エディターの分割)".to_string(),
                category: "View".to_string(),
                shortcut: Some("Ctrl+\\".to_string()),
            },
            CommandItem {
                id: "workbench.action.toggleMinimap".to_string(),
                title: "View: Toggle Minimap (ミニマップの切替)".to_string(),
                category: "View".to_string(),
                shortcut: None,
            },
            CommandItem {
                id: "markdown.showPreview".to_string(),
                title: "Markdown: Open Preview (プレビュー表示)".to_string(),
                category: "Markdown".to_string(),
                shortcut: Some("Ctrl+K V".to_string()),
            },
            CommandItem {
                id: "git.commit".to_string(),
                title: "Git: Commit (変更をコミット)".to_string(),
                category: "Git".to_string(),
                shortcut: None,
            },
            CommandItem {
                id: "workbench.action.openSettings".to_string(),
                title: "Preferences: Open Settings (設定を開く)".to_string(),
                category: "Preferences".to_string(),
                shortcut: Some("Ctrl+,".to_string()),
            },
        ];

        Self {
            buffer_text,
            core_buffer,
            workspace,
            syntax_engine: SyntaxEngine::new(),
            git_manager,
            plugin_host,
            active_view: ActivityView::Explorer,
            show_sidebar: true,
            show_terminal: true,
            show_markdown_preview: false,
            show_minimap: true,
            show_split_editor: false,
            search_query: String::new(),
            search_results_count: 0,
            commit_message: String::new(),
            terminal_input: String::new(),
            terminal_logs: vec![
                "Oxide Integrated Terminal (VS Code Parity) initialized.".to_string(),
                "PowerShell host ready. コマンドを入力して実行できます。".to_string(),
            ],
            current_file_path,
            status_message: "準備完了".to_string(),
            show_quick_pick: false,
            quick_pick_mode: QuickPickMode::Files,
            quick_pick_query: String::new(),
            available_commands,
            show_new_file_dialog: false,
            show_new_folder_dialog: false,
            show_save_as_dialog: false,
            new_item_name: String::new(),
            font_size: 14.0,
            tab_size: 4,
            terminal_height: 200.0,
        }
    }

    fn setup_japanese_fonts(ctx: &egui::Context) {
        let mut fonts = FontDefinitions::default();

        let candidate_font_paths = [
            r"C:\Windows\Fonts\msgothic.ttc",
            r"C:\Windows\Fonts\meiryo.ttc",
            r"C:\Windows\Fonts\YuGothM.ttc",
            r"C:\Windows\Fonts\msmincho.ttc",
        ];

        for path_str in &candidate_font_paths {
            if let Ok(font_bytes) = std::fs::read(path_str) {
                let font_name = "japanese_system_font".to_string();
                fonts
                    .font_data
                    .insert(font_name.clone(), FontData::from_owned(font_bytes));

                fonts
                    .families
                    .entry(FontFamily::Proportional)
                    .or_default()
                    .push(font_name.clone());

                fonts
                    .families
                    .entry(FontFamily::Monospace)
                    .or_default()
                    .push(font_name);

                break;
            }
        }

        ctx.set_fonts(fonts);
    }

    fn open_file(&mut self, path: PathBuf) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            self.buffer_text = content.clone();
            self.core_buffer = TextBuffer::from_str(&content);
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            self.workspace
                .active_tab_group_mut()
                .open_tab(Tab::new(file_name, Some(path.clone())));
            self.current_file_path = Some(path.clone());
            self.show_markdown_preview = path.extension().map_or(false, |ext| ext == "md");
            self.status_message = format!("開きました: {}", path.display());
        }
    }

    fn save_current_file(&mut self) {
        if let Some(ref path) = self.current_file_path {
            if std::fs::write(path, &self.buffer_text).is_ok() {
                self.core_buffer.set_clean();
                self.status_message = format!("保存完了: {}", path.display());
            } else {
                self.status_message = format!("保存エラー: {}", path.display());
            }
        } else {
            self.show_save_as_dialog = true;
            self.new_item_name = "new_file.rs".to_string();
        }
    }

    fn create_new_file(&mut self, file_name: &str) {
        let root = self.workspace.root_path.clone().unwrap_or_else(|| PathBuf::from("."));
        let target_path = root.join(file_name);

        if let Some(parent) = target_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        if std::fs::write(&target_path, "").is_ok() {
            self.workspace = Workspace::new(self.workspace.root_path.clone());
            self.open_file(target_path.clone());
            self.status_message = format!("ファイルを作成しました: {}", target_path.display());
        } else {
            self.status_message = format!("ファイル作成失敗: {}", file_name);
        }
    }

    fn create_new_folder(&mut self, folder_name: &str) {
        let root = self.workspace.root_path.clone().unwrap_or_else(|| PathBuf::from("."));
        let target_path = root.join(folder_name);

        if std::fs::create_dir_all(&target_path).is_ok() {
            self.workspace = Workspace::new(self.workspace.root_path.clone());
            self.status_message = format!("フォルダを作成しました: {}", target_path.display());
        } else {
            self.status_message = format!("フォルダ作成失敗: {}", folder_name);
        }
    }

    fn execute_terminal_command(&mut self, command_str: &str) {
        let trimmed = command_str.trim();
        if trimmed.is_empty() {
            return;
        }

        self.terminal_logs.push(format!("PS > {}", trimmed));

        if trimmed == "clear" || trimmed == "cls" {
            self.terminal_logs.clear();
            return;
        }

        let root_dir = self.workspace.root_path.clone().unwrap_or_else(|| PathBuf::from("."));

        let output = if cfg!(target_os = "windows") {
            Command::new("powershell")
                .arg("-NoProfile")
                .arg("-Command")
                .arg(trimmed)
                .current_dir(&root_dir)
                .output()
        } else {
            Command::new("sh")
                .arg("-c")
                .arg(trimmed)
                .current_dir(&root_dir)
                .output()
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).to_string();

                if !stdout.is_empty() {
                    for line in stdout.lines() {
                        self.terminal_logs.push(line.to_string());
                    }
                }
                if !stderr.is_empty() {
                    for line in stderr.lines() {
                        self.terminal_logs.push(format!("エラー: {}", line));
                    }
                }
                if stdout.is_empty() && stderr.is_empty() {
                    self.terminal_logs.push("(完了)".to_string());
                }
            }
            Err(e) => {
                self.terminal_logs.push(format!("実行失敗: {}", e));
            }
        }
    }

    fn execute_command_by_id(&mut self, command_id: &str) {
        match command_id {
            "workbench.action.files.save" => self.save_current_file(),
            "workbench.action.files.newFile" => {
                self.show_new_file_dialog = true;
                self.new_item_name = "".to_string();
            }
            "workbench.action.toggleSidebarVisibility" => self.show_sidebar = !self.show_sidebar,
            "workbench.action.terminal.toggleTerminal" => self.show_terminal = !self.show_terminal,
            "workbench.action.splitEditor" => self.show_split_editor = !self.show_split_editor,
            "workbench.action.toggleMinimap" => self.show_minimap = !self.show_minimap,
            "markdown.showPreview" => self.show_markdown_preview = !self.show_markdown_preview,
            "git.commit" => {
                self.active_view = ActivityView::SourceControl;
                self.show_sidebar = true;
            }
            "workbench.action.openSettings" => {
                self.active_view = ActivityView::Settings;
                self.show_sidebar = true;
            }
            _ => {}
        }
    }
}

impl eframe::App for OxideGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Global Shortcuts ---
        if ctx.input(|i| i.modifiers.ctrl && i.modifiers.shift && i.key_pressed(egui::Key::P)) {
            self.show_quick_pick = true;
            self.quick_pick_mode = QuickPickMode::Commands;
            self.quick_pick_query = "".to_string();
        } else if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::P)) {
            self.show_quick_pick = true;
            self.quick_pick_mode = QuickPickMode::Files;
            self.quick_pick_query = "".to_string();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.show_quick_pick {
            self.show_quick_pick = false;
        }

        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_current_file();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::B)) {
            self.show_sidebar = !self.show_sidebar;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::J)) {
            self.show_terminal = !self.show_terminal;
        }

        // 1. Top Menu Bar & Command Palette Button
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label(RichText::new("🦀 Oxide (VS Code)").strong().color(Color32::from_rgb(0, 150, 255)));

                ui.menu_button("ファイル (File)", |ui| {
                    if ui.button("新規ファイル作成 (Ctrl+N)").clicked() {
                        self.show_new_file_dialog = true;
                        self.new_item_name = "".to_string();
                        ui.close_menu();
                    }
                    if ui.button("新規フォルダ作成").clicked() {
                        self.show_new_folder_dialog = true;
                        self.new_item_name = "".to_string();
                        ui.close_menu();
                    }
                    if ui.button("保存 (Ctrl+S)").clicked() {
                        self.save_current_file();
                        ui.close_menu();
                    }
                    if ui.button("名前を付けて保存...").clicked() {
                        self.show_save_as_dialog = true;
                        self.new_item_name = self.current_file_path.as_ref().map_or("untitled.rs".to_string(), |p| p.file_name().unwrap_or_default().to_string_lossy().to_string());
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("終了 (Exit)").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("編集 (Edit)", |ui| {
                    if ui.button("元に戻す (Ctrl+Z)").clicked() {
                        if self.core_buffer.undo() {
                            self.buffer_text = self.core_buffer.to_string();
                        }
                        ui.close_menu();
                    }
                    if ui.button("やり直し (Ctrl+Y)").clicked() {
                        if self.core_buffer.redo() {
                            self.buffer_text = self.core_buffer.to_string();
                        }
                        ui.close_menu();
                    }
                });

                ui.menu_button("表示 (View)", |ui| {
                    if ui.button("コマンドパレット (Ctrl+Shift+P)").clicked() {
                        self.show_quick_pick = true;
                        self.quick_pick_mode = QuickPickMode::Commands;
                        self.quick_pick_query = "".to_string();
                        ui.close_menu();
                    }
                    if ui.button("クイックオープン (Ctrl+P)").clicked() {
                        self.show_quick_pick = true;
                        self.quick_pick_mode = QuickPickMode::Files;
                        self.quick_pick_query = "".to_string();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.checkbox(&mut self.show_sidebar, "サイドバー切替 (Ctrl+B)");
                    ui.checkbox(&mut self.show_terminal, "ターミナル切替 (Ctrl+J)");
                    ui.checkbox(&mut self.show_minimap, "ミニマップ表示");
                    ui.checkbox(&mut self.show_split_editor, "エディター分割");
                    ui.checkbox(&mut self.show_markdown_preview, "Markdown プレビュー切替");
                });

                ui.menu_button("ターミナル (Terminal)", |ui| {
                    if ui.button("ターミナル画面クリア").clicked() {
                        self.terminal_logs.clear();
                        ui.close_menu();
                    }
                });

                // Top Search Bar (QuickPick trigger button)
                ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                    if ui.button(RichText::new("🔍 Oxide Editor - ファイルまたはコマンドを検索 (Ctrl+P)").color(Color32::from_rgb(160, 160, 160))).clicked() {
                        self.show_quick_pick = true;
                        self.quick_pick_mode = QuickPickMode::Files;
                        self.quick_pick_query = "".to_string();
                    }
                });
            });
        });

        // 2. Bottom Status Bar (Full Width)
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(Color32::from_rgb(0, 122, 204)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let branch = self.git_manager.current_branch().unwrap_or_else(|_| "main".to_string());
                    ui.label(RichText::new(format!(" 🌿 {}", branch)).color(Color32::WHITE));
                    ui.label(RichText::new("│").color(Color32::from_rgb(100, 180, 255)));

                    ui.label(RichText::new("⚠️ 0  ❌ 0").color(Color32::WHITE));
                    ui.label(RichText::new("│").color(Color32::from_rgb(100, 180, 255)));

                    ui.label(RichText::new(format!("行: {}, 文字数: {}", self.core_buffer.len_lines(), self.core_buffer.len_chars())).color(Color32::WHITE));
                    ui.label(RichText::new("│").color(Color32::from_rgb(100, 180, 255)));

                    let lang_name = self.current_file_path.as_ref().map_or("Rust", |p| {
                        match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
                            "rs" => "Rust",
                            "md" => "Markdown",
                            "json" => "JSON",
                            "toml" => "TOML",
                            _ => "Plain Text",
                        }
                    });
                    ui.label(RichText::new(format!("UTF-8 │ スペース: {} │ {}", self.tab_size, lang_name)).color(Color32::WHITE));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&self.status_message).color(Color32::WHITE));
                    });
                });
            });

        // 3. Activity Bar (Narrow left toolbar)
        egui::SidePanel::left("activity_bar")
            .exact_width(48.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(Color32::from_rgb(45, 45, 45)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
                ui.add_space(4.0);

                let btn_explorer = ui.selectable_label(self.active_view == ActivityView::Explorer, "📁").on_hover_text("エクスプローラー (Ctrl+Shift+E)");
                if btn_explorer.clicked() {
                    if self.active_view == ActivityView::Explorer {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::Explorer;
                        self.show_sidebar = true;
                    }
                }

                let btn_search = ui.selectable_label(self.active_view == ActivityView::Search, "🔍").on_hover_text("検索 (Ctrl+Shift+F)");
                if btn_search.clicked() {
                    if self.active_view == ActivityView::Search {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::Search;
                        self.show_sidebar = true;
                    }
                }

                let btn_git = ui.selectable_label(self.active_view == ActivityView::SourceControl, "🌿").on_hover_text("ソース管理 (Ctrl+Shift+G)");
                if btn_git.clicked() {
                    if self.active_view == ActivityView::SourceControl {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::SourceControl;
                        self.show_sidebar = true;
                    }
                }

                let btn_ext = ui.selectable_label(self.active_view == ActivityView::Extensions, "🧩").on_hover_text("拡張機能 (Ctrl+Shift+X)");
                if btn_ext.clicked() {
                    if self.active_view == ActivityView::Extensions {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::Extensions;
                        self.show_sidebar = true;
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    let btn_settings = ui.selectable_label(self.active_view == ActivityView::Settings, "⚙").on_hover_text("設定 (Ctrl+,)");
                    if btn_settings.clicked() {
                        self.active_view = ActivityView::Settings;
                        self.show_sidebar = true;
                    }
                });
            });

        // 4. Sidebar Panel (Explorer / Search / Git / Extensions / Settings)
        if self.show_sidebar {
            egui::SidePanel::left("sidebar_panel")
                .resizable(true)
                .default_width(230.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(37, 37, 38)))
                .show(ctx, |ui| {
                    match self.active_view {
                        ActivityView::Explorer => {
                            ui.horizontal(|ui| {
                                ui.heading(RichText::new("エクスプローラー").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("🔄").on_hover_text("再読み込み").clicked() {
                                        self.workspace = Workspace::new(self.workspace.root_path.clone());
                                    }
                                    if ui.button("📁+").on_hover_text("新規フォルダ作成").clicked() {
                                        self.show_new_folder_dialog = true;
                                        self.new_item_name = "".to_string();
                                    }
                                    if ui.button("📄+").on_hover_text("新規ファイル作成").clicked() {
                                        self.show_new_file_dialog = true;
                                        self.new_item_name = "".to_string();
                                    }
                                });
                            });
                            ui.separator();

                            ScrollArea::vertical().show(ui, |ui| {
                                let visible = self.workspace.file_tree.flatten_visible();
                                for node in visible {
                                    let indent = "  ".repeat(node.depth);
                                    let icon = if node.is_dir {
                                        if node.is_expanded { "📂 " } else { "📁 " }
                                    } else {
                                        "📄 "
                                    };
                                    let label = format!("{}{}{}", indent, icon, node.name);

                                    if node.is_dir {
                                        if ui.button(label).clicked() {
                                            self.workspace.file_tree.toggle_expand(&node.path);
                                        }
                                    } else {
                                        let is_current = self.current_file_path.as_ref() == Some(&node.path);
                                        if ui.selectable_label(is_current, label).clicked() {
                                            self.open_file(node.path);
                                        }
                                    }
                                }
                            });
                        }
                        ActivityView::Search => {
                            ui.heading(RichText::new("検索 (SEARCH)").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("検索語:");
                                let resp = ui.text_edit_singleline(&mut self.search_query);
                                if resp.changed() {
                                    let q = SearchQuery::new(&self.search_query);
                                    if let Ok(matches) = SearchEngine::search(&self.buffer_text, &q) {
                                        self.search_results_count = matches.len();
                                    }
                                }
                            });
                            ui.label(format!("現在のバッファ内で {} 件見つかりました", self.search_results_count));
                        }
                        ActivityView::SourceControl => {
                            ui.heading(RichText::new("ソース管理 (GIT)").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();

                            ui.label(RichText::new("コミットメッセージ:").size(11.0));
                            ui.text_edit_multiline(&mut self.commit_message);
                            if ui.button("コミット実行").clicked() {
                                if !self.commit_message.is_empty() {
                                    let _ = self.git_manager.commit(&self.commit_message);
                                    self.commit_message.clear();
                                    self.status_message = "Git コミット完了".to_string();
                                }
                            }
                            ui.separator();

                            ui.label(RichText::new("変更されたファイル:").strong());
                            if let Ok(changes) = self.git_manager.get_status() {
                                for change in changes {
                                    let status_str = match change.status {
                                        GitFileStatus::Modified => "変更",
                                        GitFileStatus::Added => "追加",
                                        GitFileStatus::Deleted => "削除",
                                        GitFileStatus::Untracked => "未追跡",
                                        _ => "更新",
                                    };
                                    ui.label(format!("[{}] {}", status_str, change.path.display()));
                                }
                            }
                        }
                        ActivityView::Extensions => {
                            ui.heading(RichText::new("拡張機能 (EXTENSIONS)").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();
                            for plugin in self.plugin_host.list_plugins() {
                                ui.group(|ui| {
                                    ui.label(RichText::new(&plugin.name).strong().color(Color32::WHITE));
                                    ui.label(RichText::new(&plugin.description).size(11.0));
                                    ui.label(RichText::new(format!("v{}", plugin.version)).size(10.0).color(Color32::from_rgb(0, 180, 255)));
                                });
                            }
                        }
                        ActivityView::Settings => {
                            ui.heading(RichText::new("設定 (SETTINGS)").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();

                            ui.label("エディター フォントサイズ:");
                            ui.add(egui::Slider::new(&mut self.font_size, 10.0..=28.0).text("px"));

                            ui.add_space(8.0);
                            ui.label("タブサイズ:");
                            ui.add(egui::Slider::new(&mut self.tab_size, 2..=8).text("spaces"));

                            ui.add_space(8.0);
                            ui.checkbox(&mut self.show_minimap, "ミニマップを表示する");
                        }
                    }
                });
        }

        // 5. Modals / QuickPick (Command Palette & File Search)
        if self.show_quick_pick {
            egui::Window::new("QuickPick")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_TOP, Vec2::new(0.0, 40.0))
                .fixed_size(Vec2::new(600.0, 320.0))
                .frame(egui::Frame::window(&ctx.style()).fill(Color32::from_rgb(37, 37, 38)))
                .show(ctx, |ui| {
                    let prompt = match self.quick_pick_mode {
                        QuickPickMode::Files => "ファイル名で検索 (または '>' でコマンド)...",
                        QuickPickMode::Commands => "> コマンドを入力して実行...",
                    };

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔍").size(16.0));
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.quick_pick_query)
                                .hint_text(prompt)
                                .desired_width(f32::INFINITY),
                        );
                        resp.request_focus();

                        if resp.changed() && self.quick_pick_query.starts_with('>') {
                            self.quick_pick_mode = QuickPickMode::Commands;
                        }
                    });
                    ui.separator();

                    ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        match self.quick_pick_mode {
                            QuickPickMode::Files => {
                                let visible_files = self.workspace.file_tree.flatten_visible();
                                let mut selected_file_path: Option<PathBuf> = None;

                                for node in visible_files {
                                    if node.is_dir {
                                        continue;
                                    }
                                    if self.quick_pick_query.is_empty() || FuzzyFinder::score(&node.name, &self.quick_pick_query).is_some() {
                                        if ui.button(format!("📄 {}", node.path.display())).clicked() {
                                            selected_file_path = Some(node.path.clone());
                                        }
                                    }
                                }

                                if let Some(path) = selected_file_path {
                                    self.open_file(path);
                                    self.show_quick_pick = false;
                                }
                            }
                            QuickPickMode::Commands => {
                                let query = self.quick_pick_query.trim_start_matches('>').trim().to_string();
                                let commands = self.available_commands.clone();
                                let mut selected_command_id: Option<String> = None;

                                for cmd in &commands {
                                    if query.is_empty() || FuzzyFinder::score(&cmd.title, &query).is_some() {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&cmd.category).color(Color32::from_rgb(0, 150, 255)).size(11.0));
                                            if ui.button(&cmd.title).clicked() {
                                                selected_command_id = Some(cmd.id.clone());
                                            }
                                            if let Some(ref sc) = cmd.shortcut {
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.label(RichText::new(sc).color(Color32::from_rgb(120, 120, 120)));
                                                });
                                            }
                                        });
                                    }
                                }

                                if let Some(cmd_id) = selected_command_id {
                                    self.execute_command_by_id(&cmd_id);
                                    self.show_quick_pick = false;
                                }
                            }
                        }
                    });
                });
        }

        // File creation dialogs
        if self.show_new_file_dialog {
            egui::Window::new("新規ファイル作成")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("ファイル名を入力してください（例: src/utils.rs, docs/note.md）:");
                    let resp = ui.text_edit_singleline(&mut self.new_item_name);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) || ui.button("作成").clicked() {
                        if !self.new_item_name.trim().is_empty() {
                            let name = self.new_item_name.clone();
                            self.create_new_file(&name);
                            self.show_new_file_dialog = false;
                        }
                    }
                    if ui.button("キャンセル").clicked() {
                        self.show_new_file_dialog = false;
                    }
                });
        }

        if self.show_new_folder_dialog {
            egui::Window::new("新規フォルダ作成")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("フォルダ名を入力してください（例: crates/editor-ext）:");
                    let resp = ui.text_edit_singleline(&mut self.new_item_name);
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) || ui.button("作成").clicked() {
                        if !self.new_item_name.trim().is_empty() {
                            let name = self.new_item_name.clone();
                            self.create_new_folder(&name);
                            self.show_new_folder_dialog = false;
                        }
                    }
                    if ui.button("キャンセル").clicked() {
                        self.show_new_folder_dialog = false;
                    }
                });
        }

        if self.show_save_as_dialog {
            egui::Window::new("名前を付けて保存")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("保存先ファイル名:");
                    ui.text_edit_singleline(&mut self.new_item_name);
                    if ui.button("保存").clicked() {
                        if !self.new_item_name.trim().is_empty() {
                            let path = PathBuf::from(&self.new_item_name);
                            self.current_file_path = Some(path);
                            self.save_current_file();
                            self.show_save_as_dialog = false;
                        }
                    }
                    if ui.button("キャンセル").clicked() {
                        self.show_save_as_dialog = false;
                    }
                });
        }

        // 6. Central Panel: Split/Editor Area (Top) + Draggable Resizable Terminal (Bottom)
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(30, 30, 30)))
            .show(ctx, |ui| {
                let available_height = ui.available_height();
                let splitter_height = 6.0;

                let actual_term_height = if self.show_terminal {
                    self.terminal_height.clamp(60.0, available_height - 100.0)
                } else {
                    0.0
                };
                let editor_height = (available_height - actual_term_height - if self.show_terminal { splitter_height } else { 0.0 }).max(80.0);

                // --- TOP: Editor Area ---
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), editor_height),
                    Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        // Tab Bar
                        ui.horizontal(|ui| {
                            let file_name = self.current_file_path.as_ref().map_or("Untitled.rs".to_string(), |p| {
                                p.file_name().unwrap_or_default().to_string_lossy().to_string()
                            });
                            let is_dirty = self.core_buffer.is_dirty();
                            let title = format!("{} {}", file_name, if is_dirty { "●" } else { "" });

                            let _ = ui.selectable_label(true, title);

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button(if self.show_split_editor { "🗖 単一表示" } else { "🗗 左右分割" }).clicked() {
                                    self.show_split_editor = !self.show_split_editor;
                                }
                            });
                        });
                        ui.separator();

                        // Main Editor Viewport (Split or Single + Minimap)
                        if self.show_markdown_preview {
                            ui.columns(2, |columns| {
                                columns[0].vertical(|ui| {
                                    ui.heading(RichText::new("Markdown ソース").size(12.0).color(Color32::from_rgb(150, 150, 150)));
                                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        let lang = LanguageId::Markdown;
                                        let syntax = &self.syntax_engine;
                                        let font_size = self.font_size;

                                        let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
                                            let mut job = egui::text::LayoutJob::default();
                                            Self::highlight_text_to_layout_job(string, lang, syntax, font_size, &mut job);
                                            ui.fonts(|f| f.layout_job(job))
                                        };

                                        let edit = ui.add(
                                            egui::TextEdit::multiline(&mut self.buffer_text)
                                                .font(TextStyle::Monospace)
                                                .desired_width(f32::INFINITY)
                                                .lock_focus(true)
                                                .layouter(&mut layouter),
                                        );
                                        if edit.changed() {
                                            self.core_buffer = TextBuffer::from_str(&self.buffer_text);
                                        }
                                    });
                                });

                                columns[1].vertical(|ui| {
                                    ui.heading(RichText::new("ライブ レンダリング プレビュー").size(12.0).color(Color32::from_rgb(0, 200, 255)));
                                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        let rendered_html = MarkdownPreview::render_html(&self.buffer_text);
                                        ui.label(RichText::new(rendered_html).monospace().color(Color32::from_rgb(220, 220, 220)));
                                    });
                                });
                            });
                        } else if self.show_split_editor {
                            // Split Editor (Left / Right)
                            ui.columns(2, |columns| {
                                columns[0].vertical(|ui| {
                                    ui.label(RichText::new("Editor 1").size(11.0).color(Color32::from_rgb(150, 150, 150)));
                                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        let lang = self.current_file_path.as_ref().map_or(LanguageId::Rust, |p| {
                                            LanguageId::from_extension(p.extension().and_then(|e| e.to_str()).unwrap_or(""))
                                        });
                                        let syntax = &self.syntax_engine;
                                        let font_size = self.font_size;

                                        let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
                                            let mut job = egui::text::LayoutJob::default();
                                            Self::highlight_text_to_layout_job(string, lang, syntax, font_size, &mut job);
                                            ui.fonts(|f| f.layout_job(job))
                                        };

                                        let edit = ui.add(
                                            egui::TextEdit::multiline(&mut self.buffer_text)
                                                .font(TextStyle::Monospace)
                                                .desired_width(f32::INFINITY)
                                                .lock_focus(true)
                                                .layouter(&mut layouter),
                                        );
                                        if edit.changed() {
                                            self.core_buffer = TextBuffer::from_str(&self.buffer_text);
                                        }
                                    });
                                });

                                columns[1].vertical(|ui| {
                                    ui.label(RichText::new("Editor 2 (Split)").size(11.0).color(Color32::from_rgb(150, 150, 150)));
                                    ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                        let lang = LanguageId::Rust;
                                        let syntax = &self.syntax_engine;
                                        let font_size = self.font_size;

                                        let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
                                            let mut job = egui::text::LayoutJob::default();
                                            Self::highlight_text_to_layout_job(string, lang, syntax, font_size, &mut job);
                                            ui.fonts(|f| f.layout_job(job))
                                        };

                                        ui.add(
                                            egui::TextEdit::multiline(&mut self.buffer_text)
                                                .font(TextStyle::Monospace)
                                                .desired_width(f32::INFINITY)
                                                .lock_focus(true)
                                                .layouter(&mut layouter),
                                        );
                                    });
                                });
                            });
                        } else {
                            // Full Width Code Editor + Minimap
                            ui.horizontal(|ui| {
                                let editor_width = if self.show_minimap { (ui.available_width() - 90.0).max(100.0) } else { ui.available_width() };

                                ui.allocate_ui_with_layout(
                                    Vec2::new(editor_width, ui.available_height()),
                                    Layout::top_down(egui::Align::LEFT),
                                    |ui| {
                                        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                            let lang = self.current_file_path.as_ref().map_or(LanguageId::Rust, |p| {
                                                LanguageId::from_extension(p.extension().and_then(|e| e.to_str()).unwrap_or(""))
                                            });
                                            let syntax = &self.syntax_engine;
                                            let font_size = self.font_size;

                                            let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
                                                let mut job = egui::text::LayoutJob::default();
                                                Self::highlight_text_to_layout_job(string, lang, syntax, font_size, &mut job);
                                                ui.fonts(|f| f.layout_job(job))
                                            };

                                            let edit = ui.add(
                                                egui::TextEdit::multiline(&mut self.buffer_text)
                                                    .font(TextStyle::Monospace)
                                                    .desired_width(f32::INFINITY)
                                                    .lock_focus(true)
                                                    .layouter(&mut layouter),
                                            );
                                            if edit.changed() {
                                                self.core_buffer = TextBuffer::from_str(&self.buffer_text);
                                            }
                                        });
                                    },
                                );

                                // Minimap Column
                                if self.show_minimap {
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(80.0, ui.available_height()),
                                        Layout::top_down(egui::Align::LEFT),
                                        |ui| {
                                            ui.painter().rect_filled(
                                                ui.available_rect_before_wrap(),
                                                0.0,
                                                Color32::from_rgb(25, 25, 25),
                                            );

                                            ScrollArea::vertical().show(ui, |ui| {
                                                for line in self.buffer_text.lines().take(80) {
                                                    let preview_line: String = line.chars().take(25).collect();
                                                    ui.label(RichText::new(preview_line).size(4.0).color(Color32::from_rgb(120, 150, 180)));
                                                }
                                            });
                                        },
                                    );
                                }
                            });
                        }
                    },
                );

                // --- DRAGGABLE RESIZE SPLITTER ---
                if self.show_terminal {
                    let (splitter_rect, splitter_resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), splitter_height),
                        egui::Sense::drag(),
                    );
                    let is_hovered = splitter_resp.hovered() || splitter_resp.dragged();
                    let splitter_color = if is_hovered {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
                        Color32::from_rgb(0, 122, 204) // VS Code Focus Blue
                    } else {
                        Color32::from_rgb(50, 50, 50)
                    };
                    ui.painter().rect_filled(splitter_rect, 0.0, splitter_color);

                    if splitter_resp.dragged() {
                        let delta_y = splitter_resp.drag_delta().y;
                        self.terminal_height = (self.terminal_height - delta_y).clamp(60.0, available_height - 100.0);
                    }

                    // --- BOTTOM: Integrated Terminal (Resizable) ---
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), actual_term_height),
                        Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("💻 統合ターミナル (TERMINAL)").strong().color(Color32::from_rgb(200, 200, 200)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("×").on_hover_text("ターミナルを閉じる").clicked() {
                                        self.show_terminal = false;
                                    }
                                });
                            });

                            ScrollArea::vertical().stick_to_bottom(true).max_height((actual_term_height - 50.0).max(20.0)).show(ui, |ui| {
                                for log in &self.terminal_logs {
                                    ui.label(RichText::new(log).monospace().color(Color32::from_rgb(0, 255, 128)));
                                }
                            });

                            ui.horizontal(|ui| {
                                ui.label(RichText::new("PS >").monospace().color(Color32::from_rgb(0, 180, 255)));
                                let resp = ui.text_edit_singleline(&mut self.terminal_input);
                                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    let cmd = self.terminal_input.clone();
                                    self.execute_terminal_command(&cmd);
                                    self.terminal_input.clear();
                                    resp.request_focus();
                                }
                            });
                        },
                    );
                }
            });
    }
}

impl OxideGuiApp {
    fn highlight_text_to_layout_job(
        text: &str,
        language: LanguageId,
        syntax: &SyntaxEngine,
        font_size: f32,
        job: &mut egui::text::LayoutJob,
    ) {
        let spans = syntax.highlight(text, language);
        let default_format = egui::TextFormat {
            font_id: egui::FontId::monospace(font_size),
            color: Color32::from_rgb(212, 212, 212),
            ..Default::default()
        };

        if spans.is_empty() {
            job.append(text, 0.0, default_format);
            return;
        }

        let mut current_idx = 0;
        for span in spans {
            if span.start_offset > current_idx
                && span.start_offset <= text.len()
                && text.is_char_boundary(current_idx)
                && text.is_char_boundary(span.start_offset)
            {
                let unhighlighted = &text[current_idx..span.start_offset];
                job.append(unhighlighted, 0.0, default_format.clone());
            }

            if span.start_offset < text.len() && text.is_char_boundary(span.start_offset) {
                let end = span.end_offset.min(text.len());
                if text.is_char_boundary(end) {
                    let token_text = &text[span.start_offset..end];

                    let color = match span.token_type {
                        TokenType::Keyword | TokenType::ControlFlow => Color32::from_rgb(86, 156, 214),
                        TokenType::Function | TokenType::Method => Color32::from_rgb(220, 220, 170),
                        TokenType::Type | TokenType::Struct | TokenType::Enum | TokenType::Trait => {
                            Color32::from_rgb(78, 201, 176)
                        }
                        TokenType::String => Color32::from_rgb(206, 145, 120),
                        TokenType::Number => Color32::from_rgb(181, 206, 168),
                        TokenType::Comment | TokenType::DocComment => Color32::from_rgb(106, 153, 85),
                        TokenType::Macro => Color32::from_rgb(197, 134, 192),
                        TokenType::Operator | TokenType::Punctuation => Color32::from_rgb(212, 212, 212),
                        _ => Color32::from_rgb(156, 220, 254),
                    };

                    let format = egui::TextFormat {
                        font_id: egui::FontId::monospace(font_size),
                        color,
                        ..Default::default()
                    };

                    job.append(token_text, 0.0, format);
                    current_idx = end;
                }
            }
        }

        if current_idx < text.len() && text.is_char_boundary(current_idx) {
            job.append(&text[current_idx..], 0.0, default_format);
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let initial_file = args.get(1).map(PathBuf::from);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Oxide Editor (VS Code on Rust)")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 480.0])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Oxide Editor",
        native_options,
        Box::new(move |cc| Ok(Box::new(OxideGuiApp::new(cc, initial_file)))),
    )
}
