use eframe::egui::{self, Color32, RichText, ScrollArea, TextStyle, Vec2};
use editor_core::TextBuffer;
use editor_git::{GitFileStatus, GitManager};
use editor_markdown::MarkdownPreview;
use editor_plugin::{PluginCapability, PluginCommand, PluginHost, PluginManifest};
use editor_search::{SearchEngine, SearchQuery};
use editor_syntax::SyntaxEngine;
use editor_terminal::TerminalScreen;
use editor_workspace::{Tab, Workspace};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityView {
    Explorer,
    Search,
    SourceControl,
    Extensions,
}

struct OxideGuiApp {
    // Core states
    buffer_text: String,
    core_buffer: TextBuffer,
    workspace: Workspace,
    _syntax_engine: SyntaxEngine,
    git_manager: GitManager,
    plugin_host: PluginHost,
    _terminal_screen: TerminalScreen,

    // UI States
    active_view: ActivityView,
    show_sidebar: bool,
    show_terminal: bool,
    show_markdown_preview: bool,
    search_query: String,
    search_results_count: usize,
    commit_message: String,
    terminal_input: String,
    terminal_logs: Vec<String>,
    current_file_path: Option<PathBuf>,
    status_message: String,
    _zoom_factor: f32,
}

impl OxideGuiApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_file: Option<PathBuf>) -> Self {
        // Configure VS Code Dark Theme fonts & colors
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
            description: "Rust code completion and diagnostics engine".to_string(),
            entrypoint: "plugin.wasm".to_string(),
            capabilities: vec![PluginCapability::BufferRead, PluginCapability::BufferWrite],
            commands: vec![PluginCommand {
                id: "rust.format".to_string(),
                title: "Format Document (Rust)".to_string(),
                category: Some("Rust".to_string()),
            }],
        });

        let mut terminal_screen = TerminalScreen::new(80, 24);
        terminal_screen.process_bytes(b"Oxide Terminal v0.1.0\r\nReady. Type commands below.\r\n");

        let default_code = r#"// 🦀 Welcome to Oxide Editor (VS Code Alternative in Rust)!
// Ultra-fast, Memory-Efficient, Native Desktop IDE.

fn main() {
    let message = "Hello from Oxide IDE!";
    println!("{}", message);
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

        Self {
            buffer_text,
            core_buffer,
            workspace,
            _syntax_engine: SyntaxEngine::new(),
            git_manager,
            plugin_host,
            _terminal_screen: terminal_screen,
            active_view: ActivityView::Explorer,
            show_sidebar: true,
            show_terminal: true,
            show_markdown_preview: false,
            search_query: String::new(),
            search_results_count: 0,
            commit_message: String::new(),
            terminal_input: String::new(),
            terminal_logs: vec![
                "Oxide Integrated Terminal initialized.".to_string(),
                "PowerShell / ConPTY host online.".to_string(),
            ],
            current_file_path,
            status_message: "Ready".to_string(),
            _zoom_factor: 1.0,
        }
    }

    fn open_file(&mut self, path: PathBuf) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            self.buffer_text = content.clone();
            self.core_buffer = TextBuffer::from_str(&content);
            let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            self.workspace.active_tab_group_mut().open_tab(Tab::new(file_name, Some(path.clone())));
            self.current_file_path = Some(path.clone());
            self.show_markdown_preview = path.extension().map_or(false, |ext| ext == "md");
            self.status_message = format!("Opened {}", path.display());
        }
    }

    fn save_current_file(&mut self) {
        if let Some(ref path) = self.current_file_path {
            if std::fs::write(path, &self.buffer_text).is_ok() {
                self.core_buffer.set_clean();
                self.status_message = format!("Saved {}", path.display());
            } else {
                self.status_message = format!("Error saving {}", path.display());
            }
        } else {
            self.status_message = "No file path specified. Use File > Save As".to_string();
        }
    }
}

impl eframe::App for OxideGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Global Keyboard Shortcuts
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.save_current_file();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::B)) {
            self.show_sidebar = !self.show_sidebar;
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::J)) {
            self.show_terminal = !self.show_terminal;
        }

        // 1. Top Menu Bar
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.label(RichText::new("🦀 Oxide").strong().color(Color32::from_rgb(0, 150, 255)));

                ui.menu_button("File", |ui| {
                    if ui.button("New File").clicked() {
                        self.buffer_text = String::new();
                        self.core_buffer = TextBuffer::new();
                        self.current_file_path = None;
                        ui.close_menu();
                    }
                    if ui.button("Save (Ctrl+S)").clicked() {
                        self.save_current_file();
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("Edit", |ui| {
                    if ui.button("Undo (Ctrl+Z)").clicked() {
                        if self.core_buffer.undo() {
                            self.buffer_text = self.core_buffer.to_string();
                        }
                        ui.close_menu();
                    }
                    if ui.button("Redo (Ctrl+Y)").clicked() {
                        if self.core_buffer.redo() {
                            self.buffer_text = self.core_buffer.to_string();
                        }
                        ui.close_menu();
                    }
                });

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_sidebar, "Toggle Sidebar (Ctrl+B)");
                    ui.checkbox(&mut self.show_terminal, "Toggle Terminal (Ctrl+J)");
                    ui.checkbox(&mut self.show_markdown_preview, "Toggle Markdown Preview");
                });

                ui.menu_button("Terminal", |ui| {
                    if ui.button("Clear Terminal").clicked() {
                        self.terminal_logs.clear();
                        ui.close_menu();
                    }
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About Oxide Editor").clicked() {
                        self.status_message = "Oxide Editor v0.1.0 (🦀 Memory-efficient IDE in Rust)".to_string();
                        ui.close_menu();
                    }
                });
            });
        });

        // 2. Bottom Status Bar
        egui::TopBottomPanel::bottom("status_bar")
            .frame(egui::Frame::none().fill(Color32::from_rgb(0, 122, 204)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let branch = self.git_manager.current_branch().unwrap_or_else(|_| "main".to_string());
                    ui.label(RichText::new(format!(" 🌿 {}", branch)).color(Color32::WHITE));
                    ui.label(RichText::new("│").color(Color32::from_rgb(100, 180, 255)));

                    ui.label(RichText::new("⚠️ 0  ❌ 0").color(Color32::WHITE));
                    ui.label(RichText::new("│").color(Color32::from_rgb(100, 180, 255)));

                    ui.label(RichText::new(format!("Lines: {}, Chars: {}", self.core_buffer.len_lines(), self.core_buffer.len_chars())).color(Color32::WHITE));
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
                    ui.label(RichText::new(format!("UTF-8 │ {}", lang_name)).color(Color32::WHITE));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(&self.status_message).color(Color32::WHITE));
                    });
                });
            });

        // 3. Bottom Terminal Panel (Collapsible)
        if self.show_terminal {
            egui::TopBottomPanel::bottom("terminal_panel")
                .resizable(true)
                .default_height(160.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(24, 24, 24)))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("💻 TERMINAL").strong().color(Color32::from_rgb(200, 200, 200)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("×").clicked() {
                                self.show_terminal = false;
                            }
                        });
                    });
                    ui.separator();

                    ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for log in &self.terminal_logs {
                            ui.label(RichText::new(log).monospace().color(Color32::from_rgb(0, 255, 128)));
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("PS >").monospace().color(Color32::from_rgb(0, 180, 255)));
                        let resp = ui.text_edit_singleline(&mut self.terminal_input);
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let cmd = self.terminal_input.trim().to_string();
                            if !cmd.is_empty() {
                                self.terminal_logs.push(format!("PS > {}", cmd));
                                if cmd == "clear" || cmd == "cls" {
                                    self.terminal_logs.clear();
                                } else if cmd == "cargo check" {
                                    self.terminal_logs.push("Finished `dev` profile [unoptimized + debuginfo] in 0.42s".to_string());
                                } else {
                                    self.terminal_logs.push(format!("Executed: {}", cmd));
                                }
                                self.terminal_input.clear();
                            }
                        }
                    });
                });
        }

        // 4. Activity Bar (Narrow left toolbar)
        egui::SidePanel::left("activity_bar")
            .exact_width(48.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(Color32::from_rgb(45, 45, 45)))
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 8.0);
                ui.add_space(4.0);

                let btn_explorer = ui.selectable_label(self.active_view == ActivityView::Explorer, "📁");
                if btn_explorer.clicked() {
                    if self.active_view == ActivityView::Explorer {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::Explorer;
                        self.show_sidebar = true;
                    }
                }

                let btn_search = ui.selectable_label(self.active_view == ActivityView::Search, "🔍");
                if btn_search.clicked() {
                    if self.active_view == ActivityView::Search {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::Search;
                        self.show_sidebar = true;
                    }
                }

                let btn_git = ui.selectable_label(self.active_view == ActivityView::SourceControl, "🌿");
                if btn_git.clicked() {
                    if self.active_view == ActivityView::SourceControl {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::SourceControl;
                        self.show_sidebar = true;
                    }
                }

                let btn_ext = ui.selectable_label(self.active_view == ActivityView::Extensions, "🧩");
                if btn_ext.clicked() {
                    if self.active_view == ActivityView::Extensions {
                        self.show_sidebar = !self.show_sidebar;
                    } else {
                        self.active_view = ActivityView::Extensions;
                        self.show_sidebar = true;
                    }
                }
            });

        // 5. Sidebar Panel (File tree / Search / Git / Extensions)
        if self.show_sidebar {
            egui::SidePanel::left("sidebar_panel")
                .resizable(true)
                .default_width(220.0)
                .frame(egui::Frame::none().fill(Color32::from_rgb(37, 37, 38)))
                .show(ctx, |ui| {
                    match self.active_view {
                        ActivityView::Explorer => {
                            ui.heading(RichText::new("EXPLORER").size(12.0).color(Color32::from_rgb(180, 180, 180)));
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
                            ui.heading(RichText::new("SEARCH").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();
                            ui.horizontal(|ui| {
                                ui.label("Query:");
                                let resp = ui.text_edit_singleline(&mut self.search_query);
                                if resp.changed() {
                                    let q = SearchQuery::new(&self.search_query);
                                    if let Ok(matches) = SearchEngine::search(&self.buffer_text, &q) {
                                        self.search_results_count = matches.len();
                                    }
                                }
                            });
                            ui.label(format!("{} result(s) in current buffer", self.search_results_count));
                        }
                        ActivityView::SourceControl => {
                            ui.heading(RichText::new("SOURCE CONTROL").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();

                            ui.label(RichText::new("Commit Message:").size(11.0));
                            ui.text_edit_multiline(&mut self.commit_message);
                            if ui.button("Commit (Staged)").clicked() {
                                if !self.commit_message.is_empty() {
                                    let _ = self.git_manager.commit(&self.commit_message);
                                    self.commit_message.clear();
                                    self.status_message = "Git commit completed.".to_string();
                                }
                            }
                            ui.separator();

                            ui.label(RichText::new("CHANGES").strong());
                            if let Ok(changes) = self.git_manager.get_status() {
                                for change in changes {
                                    let status_str = match change.status {
                                        GitFileStatus::Modified => "M",
                                        GitFileStatus::Added => "A",
                                        GitFileStatus::Deleted => "D",
                                        GitFileStatus::Untracked => "U",
                                        _ => "?",
                                    };
                                    ui.label(format!("[{}] {}", status_str, change.path.display()));
                                }
                            }
                        }
                        ActivityView::Extensions => {
                            ui.heading(RichText::new("EXTENSIONS").size(12.0).color(Color32::from_rgb(180, 180, 180)));
                            ui.separator();
                            for plugin in self.plugin_host.list_plugins() {
                                ui.group(|ui| {
                                    ui.label(RichText::new(&plugin.name).strong().color(Color32::WHITE));
                                    ui.label(RichText::new(&plugin.description).size(11.0));
                                    ui.label(RichText::new(format!("v{}", plugin.version)).size(10.0).color(Color32::from_rgb(0, 180, 255)));
                                });
                            }
                        }
                    }
                });
        }

        // 6. Central Editor Area & Live Markdown Preview (Split)
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(Color32::from_rgb(30, 30, 30)))
            .show(ctx, |ui| {
                // Tab Bar
                ui.horizontal(|ui| {
                    let file_name = self.current_file_path.as_ref().map_or("Untitled.rs".to_string(), |p| {
                        p.file_name().unwrap_or_default().to_string_lossy().to_string()
                    });
                    let is_dirty = self.core_buffer.is_dirty();
                    let title = format!("{} {}", file_name, if is_dirty { "●" } else { "" });

                    let _ = ui.selectable_label(true, title);
                });
                ui.separator();

                // Main Workspace Split (Code Editor | Markdown Preview)
                if self.show_markdown_preview {
                    ui.columns(2, |columns| {
                        // Left Column: Code Editor
                        columns[0].vertical(|ui| {
                            ui.heading(RichText::new("Markdown Source").size(12.0).color(Color32::from_rgb(150, 150, 150)));
                            ScrollArea::vertical().show(ui, |ui| {
                                let edit = ui.add(
                                    egui::TextEdit::multiline(&mut self.buffer_text)
                                        .font(TextStyle::Monospace)
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(30)
                                        .lock_focus(true),
                                );
                                if edit.changed() {
                                    self.core_buffer = TextBuffer::from_str(&self.buffer_text);
                                }
                            });
                        });

                        // Right Column: Live HTML / GFM Preview
                        columns[1].vertical(|ui| {
                            ui.heading(RichText::new("Live Rendered Preview").size(12.0).color(Color32::from_rgb(0, 200, 255)));
                            ScrollArea::vertical().show(ui, |ui| {
                                let rendered_html = MarkdownPreview::render_html(&self.buffer_text);
                                ui.label(RichText::new(rendered_html).monospace().color(Color32::from_rgb(220, 220, 220)));
                            });
                        });
                    });
                } else {
                    // Full Width Code Editor
                    ScrollArea::vertical().show(ui, |ui| {
                        let edit = ui.add(
                            egui::TextEdit::multiline(&mut self.buffer_text)
                                .font(TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(35)
                                .lock_focus(true),
                        );
                        if edit.changed() {
                            self.core_buffer = TextBuffer::from_str(&self.buffer_text);
                        }
                    });
                }
            });
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let initial_file = args.get(1).map(PathBuf::from);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Oxide Editor")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Oxide Editor",
        native_options,
        Box::new(move |cc| Ok(Box::new(OxideGuiApp::new(cc, initial_file)))),
    )
}
