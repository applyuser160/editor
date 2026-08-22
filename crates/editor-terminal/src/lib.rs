//! `editor-terminal`: Virtual terminal emulator (PTY) host.

pub struct TerminalSession {
    pub id: usize,
    pub title: String,
}

impl TerminalSession {
    pub fn new(id: usize, title: String) -> Self {
        Self { id, title }
    }
}
