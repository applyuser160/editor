//! `editor-core`: Memory-efficient text buffer and cursor engine backed by Ropey.

use ropey::Rope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a text buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BufferId(pub Uuid);

impl Default for BufferId {
    fn default() -> Self {
        Self(Uuid::new_v4())
    }
}

/// A line and column position (0-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A selection or cursor position in the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: Position,
    pub cursor: Position,
}

impl Selection {
    pub fn point(pos: Position) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
        }
    }

    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.cursor
    }
}

/// An edit action applied to the buffer for Undo/Redo history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditAction {
    Insert {
        char_offset: usize,
        text: String,
    },
    Delete {
        char_offset: usize,
        text: String,
    },
}

/// Core TextBuffer backed by a Rope data structure for O(log N) operations.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    id: BufferId,
    rope: Rope,
    selections: Vec<Selection>,
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,
    is_dirty: bool,
}

impl TextBuffer {
    /// Creates a new empty text buffer.
    pub fn new() -> Self {
        Self {
            id: BufferId::default(),
            rope: Rope::new(),
            selections: vec![Selection::point(Position::new(0, 0))],
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_dirty: false,
        }
    }

    /// Creates a buffer initialized with text content.
    pub fn from_str(text: &str) -> Self {
        Self {
            id: BufferId::default(),
            rope: Rope::from_str(text),
            selections: vec![Selection::point(Position::new(0, 0))],
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_dirty: false,
        }
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    pub fn set_selections(&mut self, selections: Vec<Selection>) {
        self.selections = selections;
    }

    /// Converts a Line/Column position to a 0-indexed character offset.
    pub fn position_to_char_offset(&self, pos: Position) -> usize {
        if pos.line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }
        let line_char_idx = self.rope.line_to_char(pos.line);
        let line = self.rope.line(pos.line);
        let col = pos.column.min(line.len_chars());
        line_char_idx + col
    }

    /// Converts a 0-indexed character offset to a Line/Column position.
    pub fn char_offset_to_position(&self, char_offset: usize) -> Position {
        let offset = char_offset.min(self.rope.len_chars());
        let line = self.rope.char_to_line(offset);
        let line_start_char = self.rope.line_to_char(line);
        let col = offset - line_start_char;
        Position::new(line, col)
    }

    /// Inserts text at a specific character offset.
    pub fn insert(&mut self, char_offset: usize, text: &str) {
        let offset = char_offset.min(self.rope.len_chars());
        self.rope.insert(offset, text);
        self.undo_stack.push(EditAction::Insert {
            char_offset: offset,
            text: text.to_string(),
        });
        self.redo_stack.clear();
        self.is_dirty = true;
    }

    /// Deletes text of given character length at offset.
    pub fn delete(&mut self, char_offset: usize, char_len: usize) {
        let start = char_offset.min(self.rope.len_chars());
        let end = (char_offset + char_len).min(self.rope.len_chars());
        if start >= end {
            return;
        }

        let slice = self.rope.slice(start..end);
        let deleted_text = slice.to_string();

        self.rope.remove(start..end);
        self.undo_stack.push(EditAction::Delete {
            char_offset: start,
            text: deleted_text,
        });
        self.redo_stack.clear();
        self.is_dirty = true;
    }

    /// Performs an Undo operation.
    pub fn undo(&mut self) -> bool {
        if let Some(action) = self.undo_stack.pop() {
            match &action {
                EditAction::Insert { char_offset, text } => {
                    let len = text.chars().count();
                    self.rope.remove(*char_offset..*char_offset + len);
                    self.redo_stack.push(action);
                }
                EditAction::Delete { char_offset, text } => {
                    self.rope.insert(*char_offset, text);
                    self.redo_stack.push(action);
                }
            }
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    /// Performs a Redo operation.
    pub fn redo(&mut self) -> bool {
        if let Some(action) = self.redo_stack.pop() {
            match &action {
                EditAction::Insert { char_offset, text } => {
                    self.rope.insert(*char_offset, text);
                    self.undo_stack.push(action);
                }
                EditAction::Delete { char_offset, text } => {
                    let len = text.chars().count();
                    self.rope.remove(*char_offset..*char_offset + len);
                    self.undo_stack.push(action);
                }
            }
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    /// Returns the entire text as a String.
    pub fn to_string(&self) -> String {
        self.rope.to_string()
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_insert_and_undo_redo() {
        let mut buffer = TextBuffer::new();
        buffer.insert(0, "Hello, Oxide!");
        assert_eq!(buffer.to_string(), "Hello, Oxide!");
        assert_eq!(buffer.len_lines(), 1);

        buffer.undo();
        assert_eq!(buffer.to_string(), "");

        buffer.redo();
        assert_eq!(buffer.to_string(), "Hello, Oxide!");
    }

    #[test]
    fn test_position_conversion() {
        let buffer = TextBuffer::from_str("Line 1\nLine 2\nLine 3");
        let pos = buffer.char_offset_to_position(7);
        assert_eq!(pos, Position::new(1, 0));

        let offset = buffer.position_to_char_offset(Position::new(1, 0));
        assert_eq!(offset, 7);
    }
}
