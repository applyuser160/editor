use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use editor_core::{Position, Selection, TextBuffer};
use editor_git::GitManager;
use editor_syntax::{LanguageId, SyntaxEngine, TokenType};
use editor_workspace::Workspace;
use std::io::{self, stdout, Write};
use std::path::PathBuf;

enum ActivePane {
    Editor,
    Sidebar,
}

struct EditorApp {
    buffer: TextBuffer,
    workspace: Workspace,
    syntax_engine: SyntaxEngine,
    git_manager: GitManager,
    active_pane: ActivePane,
    cursor_pos: Position,
    scroll_offset: usize,
    status_message: String,
    should_quit: bool,
    current_file: Option<PathBuf>,
}

impl EditorApp {
    fn new(file_path: Option<PathBuf>) -> Self {
        let current_dir = std::env::current_dir().unwrap_or_default();
        let workspace = Workspace::new(Some(current_dir.clone()));
        let git_manager = GitManager::new(current_dir);

        let (buffer, current_file) = if let Some(ref path) = file_path {
            if let Ok(content) = std::fs::read_to_string(path) {
                (TextBuffer::from_str(&content), Some(path.clone()))
            } else {
                (TextBuffer::new(), Some(path.clone()))
            }
        } else {
            let mut buf = TextBuffer::new();
            buf.insert(
                0,
                "// 🦀 Welcome to Oxide Editor!\n// A lightweight, memory-efficient IDE in Rust.\n\nfn main() {\n    println!(\"Hello, Oxide!\");\n}\n",
            );
            (buf, None)
        };

        Self {
            buffer,
            workspace,
            syntax_engine: SyntaxEngine::new(),
            git_manager,
            active_pane: ActivePane::Editor,
            cursor_pos: Position::ZERO,
            scroll_offset: 0,
            status_message: "Ready | [Ctrl+Q] Quit | [Ctrl+S] Save | [Ctrl+Z] Undo | [Tab] Toggle Sidebar".to_string(),
            should_quit: false,
            current_file,
        }
    }

    fn run(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;
        let mut out = stdout();
        execute!(out, EnterAlternateScreen, cursor::Hide)?;

        while !self.should_quit {
            self.render(&mut out)?;
            self.handle_input()?;
        }

        execute!(out, LeaveAlternateScreen, cursor::Show)?;
        terminal::disable_raw_mode()?;
        Ok(())
    }

    fn render(&mut self, out: &mut io::Stdout) -> io::Result<()> {
        let (cols, rows) = terminal::size()?;
        let cols = cols as usize;
        let rows = rows as usize;

        let title_rows = 1;
        let status_rows = 1;
        let main_rows = rows.saturating_sub(title_rows + status_rows);

        let sidebar_width = if matches!(self.active_pane, ActivePane::Sidebar) {
            28.min(cols / 3)
        } else {
            22.min(cols / 4)
        };
        let _editor_width = cols.saturating_sub(sidebar_width + 1);

        // 1. Draw Title Bar
        execute!(out, cursor::MoveTo(0, 0), SetBackgroundColor(Color::DarkGrey), SetForegroundColor(Color::White))?;
        let file_name = self.current_file.as_ref().map_or("Untitled.rs".to_string(), |p| p.file_name().unwrap_or_default().to_string_lossy().to_string());
        let dirty_flag = if self.buffer.is_dirty() { " ● (modified)" } else { "" };
        let branch = self.git_manager.current_branch().unwrap_or_else(|_| "main".to_string());
        let title_text = format!(" 🦀 Oxide Editor v0.1.0 | {} {} | 🌿 Branch: {}", file_name, dirty_flag, branch);
        let padded_title = format!("{:<width$}", title_text, width = cols);
        execute!(out, Print(&padded_title[..cols.min(padded_title.len())]))?;

        // 2. Draw Main Area (Sidebar + Editor)
        let visible_tree = self.workspace.file_tree.flatten_visible();

        for row_idx in 0..main_rows {
            let screen_y = (title_rows + row_idx) as u16;
            execute!(out, cursor::MoveTo(0, screen_y))?;

            // Draw Sidebar
            let is_sidebar_active = matches!(self.active_pane, ActivePane::Sidebar);
            let sidebar_bg = if is_sidebar_active { Color::Rgb { r: 37, g: 37, b: 38 } } else { Color::Rgb { r: 30, g: 30, b: 30 } };
            execute!(out, SetBackgroundColor(sidebar_bg), SetForegroundColor(Color::Grey))?;

            let mut sidebar_line = String::new();
            if row_idx == 0 {
                sidebar_line = " 📁 EXPLORER".to_string();
            } else if let Some(node) = visible_tree.get(row_idx - 1) {
                let indent = "  ".repeat(node.depth);
                let icon = if node.is_dir { if node.is_expanded { "📂 " } else { "📁 " } } else { "📄 " };
                sidebar_line = format!(" {}{}{}", indent, icon, node.name);
            }
            let sidebar_display = format!("{:<width$}", sidebar_line, width = sidebar_width);
            execute!(out, Print(&sidebar_display[..sidebar_width.min(sidebar_display.len())]))?;

            // Draw Separator
            execute!(out, SetBackgroundColor(Color::Black), SetForegroundColor(Color::DarkGrey), Print("│"))?;

            // Draw Editor Line
            let doc_line_idx = self.scroll_offset + row_idx;
            execute!(out, SetBackgroundColor(Color::Black), SetForegroundColor(Color::White))?;

            if doc_line_idx < self.buffer.len_lines() {
                if let Some(line_slice) = self.buffer.line(doc_line_idx) {
                    let gutter = format!("{:>4} │ ", doc_line_idx + 1);
                    execute!(out, SetForegroundColor(Color::DarkGrey), Print(&gutter))?;

                    let line_str = line_slice.to_string();
                    let trimmed_line = line_str.trim_end_matches(['\r', '\n']);

                    // Syntax Highlight line
                    let spans = self.syntax_engine.highlight(trimmed_line, LanguageId::Rust);
                    let mut current_idx = 0;

                    for span in spans {
                        if span.start_offset > current_idx && span.start_offset <= trimmed_line.len() {
                            let unhighlighted = &trimmed_line[current_idx..span.start_offset];
                            execute!(out, SetForegroundColor(Color::White), Print(unhighlighted))?;
                        }
                        if span.start_offset < trimmed_line.len() {
                            let end = span.end_offset.min(trimmed_line.len());
                            let token_str = &trimmed_line[span.start_offset..end];
                            let fg_color = match span.token_type {
                                TokenType::Keyword | TokenType::ControlFlow => Color::Cyan,
                                TokenType::Function | TokenType::Method => Color::Yellow,
                                TokenType::Type | TokenType::Struct => Color::Green,
                                TokenType::String => Color::Rgb { r: 206, g: 145, b: 120 },
                                TokenType::Number => Color::Rgb { r: 181, g: 206, b: 168 },
                                TokenType::Comment | TokenType::DocComment => Color::DarkGreen,
                                _ => Color::White,
                            };
                            execute!(out, SetForegroundColor(fg_color), Print(token_str))?;
                            current_idx = end;
                        }
                    }

                    if current_idx < trimmed_line.len() {
                        execute!(out, SetForegroundColor(Color::White), Print(&trimmed_line[current_idx..]))?;
                    }

                    // Clear remainder of editor line
                    execute!(out, Clear(ClearType::UntilNewLine))?;
                }
            } else {
                execute!(out, SetForegroundColor(Color::DarkGrey), Print("   ~ │"), Clear(ClearType::UntilNewLine))?;
            }
        }

        // 3. Draw Status Bar
        execute!(
            out,
            cursor::MoveTo(0, (rows - 1) as u16),
            SetBackgroundColor(Color::Rgb { r: 0, g: 122, b: 204 }),
            SetForegroundColor(Color::White)
        )?;
        let status_info = format!(
            " Ln {}, Col {} | UTF-8 | Rust | {}",
            self.cursor_pos.line + 1,
            self.cursor_pos.column + 1,
            self.status_message
        );
        let padded_status = format!("{:<width$}", status_info, width = cols);
        execute!(out, Print(&padded_status[..cols.min(padded_status.len())]), ResetColor)?;

        // 4. Position Terminal Cursor
        let cursor_screen_x = sidebar_width + 1 + 7 + self.cursor_pos.column;
        let cursor_screen_y = title_rows + (self.cursor_pos.line.saturating_sub(self.scroll_offset));
        if cursor_screen_y < rows - 1 && cursor_screen_x < cols {
            execute!(out, cursor::MoveTo(cursor_screen_x as u16, cursor_screen_y as u16), cursor::Show)?;
        }

        out.flush()?;
        Ok(())
    }

    fn handle_input(&mut self) -> io::Result<()> {
        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key_event) = event::read()? {
                // Key bindings
                match (key_event.code, key_event.modifiers) {
                    (KeyCode::Char('q'), KeyModifiers::CONTROL) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        self.should_quit = true;
                    }
                    (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                        if let Some(ref path) = self.current_file {
                            if let Ok(_) = std::fs::write(path, self.buffer.to_string()) {
                                self.buffer.set_clean();
                                self.status_message = format!("Saved to {}", path.display());
                            }
                        } else {
                            self.status_message = "No file path specified. (Buffer in memory)".to_string();
                        }
                    }
                    (KeyCode::Char('z'), KeyModifiers::CONTROL) => {
                        if self.buffer.undo() {
                            self.status_message = "Undo edit".to_string();
                        }
                    }
                    (KeyCode::Char('y'), KeyModifiers::CONTROL) => {
                        if self.buffer.redo() {
                            self.status_message = "Redo edit".to_string();
                        }
                    }
                    (KeyCode::Tab, KeyModifiers::NONE) => {
                        self.active_pane = match self.active_pane {
                            ActivePane::Editor => ActivePane::Sidebar,
                            ActivePane::Sidebar => ActivePane::Editor,
                        };
                        self.status_message = match self.active_pane {
                            ActivePane::Editor => "Focus: Editor".to_string(),
                            ActivePane::Sidebar => "Focus: Sidebar (Explorer)".to_string(),
                        };
                    }
                    (KeyCode::Up, _) => {
                        if self.cursor_pos.line > 0 {
                            self.cursor_pos.line -= 1;
                            if self.cursor_pos.line < self.scroll_offset {
                                self.scroll_offset = self.cursor_pos.line;
                            }
                        }
                    }
                    (KeyCode::Down, _) => {
                        if self.cursor_pos.line + 1 < self.buffer.len_lines() {
                            self.cursor_pos.line += 1;
                            let (_, rows) = terminal::size().unwrap_or((80, 24));
                            let main_rows = (rows as usize).saturating_sub(2);
                            if self.cursor_pos.line >= self.scroll_offset + main_rows {
                                self.scroll_offset += 1;
                            }
                        }
                    }
                    (KeyCode::Left, _) => {
                        if self.cursor_pos.column > 0 {
                            self.cursor_pos.column -= 1;
                        }
                    }
                    (KeyCode::Right, _) => {
                        let line_len = self.buffer.line(self.cursor_pos.line).map_or(0, |l| l.len_chars().saturating_sub(1));
                        if self.cursor_pos.column < line_len {
                            self.cursor_pos.column += 1;
                        }
                    }
                    (KeyCode::Enter, _) => {
                        self.buffer.set_selections(vec![Selection::point(self.cursor_pos)]);
                        self.buffer.insert_at_cursors("\n");
                        self.cursor_pos.line += 1;
                        self.cursor_pos.column = 0;
                    }
                    (KeyCode::Backspace, _) => {
                        self.buffer.set_selections(vec![Selection::point(self.cursor_pos)]);
                        self.buffer.delete_at_cursors(true);
                        if self.cursor_pos.column > 0 {
                            self.cursor_pos.column -= 1;
                        } else if self.cursor_pos.line > 0 {
                            self.cursor_pos.line -= 1;
                            self.cursor_pos.column = self.buffer.line(self.cursor_pos.line).map_or(0, |l| l.len_chars().saturating_sub(1));
                        }
                    }
                    (KeyCode::Char(c), _) => {
                        let text = c.to_string();
                        self.buffer.set_selections(vec![Selection::point(self.cursor_pos)]);
                        self.buffer.insert_at_cursors(&text);
                        self.cursor_pos.column += 1;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let file_path = args.get(1).map(PathBuf::from);

    let mut app = EditorApp::new(file_path);
    app.run()?;

    println!("Oxide Editor closed gracefully. Goodbye!");
    Ok(())
}
