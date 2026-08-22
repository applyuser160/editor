//! `editor-terminal`: Virtual terminal emulator (PTY), ANSI escape parser, and screen grid.

use serde::{Deserialize, Serialize};

/// Color representation for terminal text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalColor {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Rgb(u8, u8, u8),
}

/// Style attributes for terminal cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CellStyle {
    pub fg: Option<TerminalColor>,
    pub bg: Option<TerminalColor>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

/// A single character cell on the terminal screen grid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalCell {
    pub c: char,
    pub style: CellStyle,
}

impl Default for TerminalCell {
    fn default() -> Self {
        Self {
            c: ' ',
            style: CellStyle::default(),
        }
    }
}

/// Terminal screen grid managing visible rows and scrollback buffer.
#[derive(Debug, Clone)]
pub struct TerminalScreen {
    pub cols: usize,
    pub rows: usize,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub current_style: CellStyle,
    pub grid: Vec<Vec<TerminalCell>>,
    pub scrollback: Vec<Vec<TerminalCell>>,
}

impl TerminalScreen {
    pub fn new(cols: usize, rows: usize) -> Self {
        let grid = vec![vec![TerminalCell::default(); cols]; rows];
        Self {
            cols,
            rows,
            cursor_x: 0,
            cursor_y: 0,
            current_style: CellStyle::default(),
            grid,
            scrollback: Vec::new(),
        }
    }

    /// Writes raw output from PTY into the terminal screen, handling ANSI escapes.
    pub fn process_bytes(&mut self, bytes: &[u8]) {
        let mut i = 0;
        let len = bytes.len();

        while i < len {
            let b = bytes[i];

            if b == b'\x1b' && i + 1 < len && bytes[i + 1] == b'[' {
                // ANSI CSI Escape Sequence
                i += 2;
                let start = i;
                while i < len && !bytes[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i < len {
                    let cmd = bytes[i] as char;
                    let param_str = std::str::from_utf8(&bytes[start..i]).unwrap_or_default();
                    self.handle_csi_command(cmd, param_str);
                    i += 1;
                }
                continue;
            }

            match b {
                b'\r' => {
                    self.cursor_x = 0;
                }
                b'\n' => {
                    self.new_line();
                }
                b'\x08' => {
                    // Backspace
                    if self.cursor_x > 0 {
                        self.cursor_x -= 1;
                    }
                }
                b'\t' => {
                    let tab_stop = 8;
                    self.cursor_x = ((self.cursor_x / tab_stop) + 1) * tab_stop;
                    if self.cursor_x >= self.cols {
                        self.new_line();
                    }
                }
                32..=126 => {
                    let c = b as char;
                    if self.cursor_y < self.rows && self.cursor_x < self.cols {
                        self.grid[self.cursor_y][self.cursor_x] = TerminalCell {
                            c,
                            style: self.current_style,
                        };
                        self.cursor_x += 1;
                        if self.cursor_x >= self.cols {
                            self.new_line();
                        }
                    }
                }
                _ => {}
            }

            i += 1;
        }
    }

    fn new_line(&mut self) {
        self.cursor_x = 0;
        if self.cursor_y + 1 < self.rows {
            self.cursor_y += 1;
        } else {
            // Scroll up
            let removed_line = self.grid.remove(0);
            self.scrollback.push(removed_line);
            self.grid.push(vec![TerminalCell::default(); self.cols]);
        }
    }

    fn handle_csi_command(&mut self, cmd: char, params: &str) {
        match cmd {
            'm' => {
                // SGR (Select Graphic Rendition) Color / Style
                if params.is_empty() || params == "0" {
                    self.current_style = CellStyle::default();
                    return;
                }
                for code_str in params.split(';') {
                    let code: u32 = code_str.parse().unwrap_or(0);
                    match code {
                        0 => self.current_style = CellStyle::default(),
                        1 => self.current_style.bold = true,
                        3 => self.current_style.italic = true,
                        4 => self.current_style.underline = true,
                        7 => self.current_style.inverse = true,
                        30 => self.current_style.fg = Some(TerminalColor::Black),
                        31 => self.current_style.fg = Some(TerminalColor::Red),
                        32 => self.current_style.fg = Some(TerminalColor::Green),
                        33 => self.current_style.fg = Some(TerminalColor::Yellow),
                        34 => self.current_style.fg = Some(TerminalColor::Blue),
                        35 => self.current_style.fg = Some(TerminalColor::Magenta),
                        36 => self.current_style.fg = Some(TerminalColor::Cyan),
                        37 => self.current_style.fg = Some(TerminalColor::White),
                        39 => self.current_style.fg = None,
                        _ => {}
                    }
                }
            }
            'H' | 'f' => {
                // Cursor position: ESC [ row ; col H (1-indexed)
                let parts: Vec<&str> = params.split(';').collect();
                let row = parts.first().and_then(|p| p.parse::<usize>().ok()).unwrap_or(1).saturating_sub(1);
                let col = parts.get(1).and_then(|p| p.parse::<usize>().ok()).unwrap_or(1).saturating_sub(1);
                self.cursor_y = row.min(self.rows.saturating_sub(1));
                self.cursor_x = col.min(self.cols.saturating_sub(1));
            }
            'J' => {
                // Erase in display
                if params == "2" || params == "3" {
                    for row in &mut self.grid {
                        for cell in row {
                            *cell = TerminalCell::default();
                        }
                    }
                    self.cursor_x = 0;
                    self.cursor_y = 0;
                }
            }
            _ => {}
        }
    }

    /// Renders screen as plain string for testing/logging.
    pub fn to_plain_string(&self) -> String {
        let mut out = String::new();
        for row in &self.grid {
            let row_str: String = row.iter().map(|cell| cell.c).collect();
            out.push_str(row_str.trim_end());
            out.push('\n');
        }
        out
    }
}

/// A terminal emulator session managing a virtual screen and title.
#[derive(Debug, Clone)]
pub struct TerminalSession {
    pub id: usize,
    pub title: String,
    pub screen: TerminalScreen,
}

impl TerminalSession {
    pub fn new(id: usize, title: String, cols: usize, rows: usize) -> Self {
        Self {
            id,
            title,
            screen: TerminalScreen::new(cols, rows),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_ansi_output() {
        let mut screen = TerminalScreen::new(80, 24);
        let input = b"Hello \x1b[31mRed World\x1b[0m!\r\nSecond Line";
        screen.process_bytes(input);

        assert_eq!(screen.grid[0][0].c, 'H');
        assert_eq!(screen.grid[0][6].c, 'R');
        assert_eq!(screen.grid[0][6].style.fg, Some(TerminalColor::Red));
        assert_eq!(screen.grid[0][15].c, '!');
        assert_eq!(screen.grid[0][15].style.fg, None);

        assert_eq!(screen.grid[1][0].c, 'S');
        assert_eq!(screen.grid[1][10].c, 'e');
    }

    #[test]
    fn test_terminal_cursor_movement_and_scroll() {
        let mut screen = TerminalScreen::new(20, 3);
        screen.process_bytes(b"Line 1\r\nLine 2\r\nLine 3\r\nLine 4");

        // Line 1 scrolled off into scrollback
        assert_eq!(screen.scrollback.len(), 1);
        let line4_str: String = screen.grid[2].iter().map(|c| c.c).collect();
        assert!(line4_str.starts_with("Line 4"));
    }
}
