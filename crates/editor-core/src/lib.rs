//! `editor-core`: Memory-efficient text buffer, multi-cursor, and transaction engine backed by Ropey.

use ropey::{Rope, RopeSlice};
use serde::{Deserialize, Serialize};
use std::cmp::{max, min};
use std::io::{self, Read};
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
    pub const ZERO: Self = Self { line: 0, column: 0 };

    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

/// A selection or cursor in the buffer, defined by an anchor and cursor position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: Position,
    pub cursor: Position,
}

impl Selection {
    /// Creates a collapsed cursor at a specific position.
    pub fn point(pos: Position) -> Self {
        Self {
            anchor: pos,
            cursor: pos,
        }
    }

    /// Creates a range selection.
    pub fn range(anchor: Position, cursor: Position) -> Self {
        Self { anchor, cursor }
    }

    /// Returns true if the selection has zero width (a simple caret cursor).
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.cursor
    }

    /// Returns the start position in document order.
    pub fn start(&self) -> Position {
        min(self.anchor, self.cursor)
    }

    /// Returns the end position in document order.
    pub fn end(&self) -> Position {
        max(self.anchor, self.cursor)
    }
}

/// Single atomic edit operation on the text buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditOp {
    Insert {
        char_offset: usize,
        text: String,
    },
    Delete {
        char_offset: usize,
        text: String,
    },
}

impl EditOp {
    /// Creates an inverted edit operation for undo purposes.
    pub fn invert(&self) -> Self {
        match self {
            Self::Insert { char_offset, text } => Self::Delete {
                char_offset: *char_offset,
                text: text.clone(),
            },
            Self::Delete { char_offset, text } => Self::Insert {
                char_offset: *char_offset,
                text: text.clone(),
            },
        }
    }
}

/// A transaction consisting of multiple edit operations applied atomically.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Transaction {
    pub ops: Vec<EditOp>,
    pub selections_before: Vec<Selection>,
    pub selections_after: Vec<Selection>,
}

impl Transaction {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn invert(&self) -> Self {
        let inverted_ops = self.ops.iter().rev().map(|op| op.invert()).collect();
        Self {
            ops: inverted_ops,
            selections_before: self.selections_after.clone(),
            selections_after: self.selections_before.clone(),
        }
    }
}

/// Core TextBuffer backed by a Rope data structure for O(log N) operations.
#[derive(Debug, Clone)]
pub struct TextBuffer {
    id: BufferId,
    rope: Rope,
    selections: Vec<Selection>,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
    is_dirty: bool,
}

impl TextBuffer {
    /// Creates a new empty text buffer.
    pub fn new() -> Self {
        Self {
            id: BufferId::default(),
            rope: Rope::new(),
            selections: vec![Selection::point(Position::ZERO)],
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
            selections: vec![Selection::point(Position::ZERO)],
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_dirty: false,
        }
    }

    /// Loads text from any reader into the buffer using streaming.
    pub fn from_reader<R: Read>(mut reader: R) -> io::Result<Self> {
        let rope = Rope::from_reader(&mut reader)?;
        Ok(Self {
            id: BufferId::default(),
            rope,
            selections: vec![Selection::point(Position::ZERO)],
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            is_dirty: false,
        })
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

    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn set_clean(&mut self) {
        self.is_dirty = false;
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    pub fn main_selection(&self) -> Selection {
        self.selections.first().copied().unwrap_or(Selection::point(Position::ZERO))
    }

    /// Sets the selection list and normalizes them (sorting & merging overlaps).
    pub fn set_selections(&mut self, mut selections: Vec<Selection>) {
        if selections.is_empty() {
            selections.push(Selection::point(Position::ZERO));
        }
        self.normalize_selections(&mut selections);
        self.selections = selections;
    }

    /// Adds a new cursor/selection to the existing list.
    pub fn add_selection(&mut self, selection: Selection) {
        let mut sels = self.selections.clone();
        sels.push(selection);
        self.set_selections(sels);
    }

    /// Normalizes and merges overlapping selections.
    fn normalize_selections(&self, selections: &mut Vec<Selection>) {
        if selections.len() <= 1 {
            return;
        }

        // Sort by start position
        selections.sort_by_key(|s| s.start());

        let mut merged: Vec<Selection> = Vec::with_capacity(selections.len());
        for sel in selections.drain(..) {
            if let Some(last) = merged.last_mut() {
                if last.end() >= sel.start() {
                    // Merge overlapping ranges
                    let new_start = min(last.start(), sel.start());
                    let new_end = max(last.end(), sel.end());
                    *last = Selection::range(new_start, new_end);
                    continue;
                }
            }
            merged.push(sel);
        }
        *selections = merged;
    }

    /// Converts a Line/Column position to a 0-indexed character offset.
    pub fn position_to_char_offset(&self, pos: Position) -> usize {
        if pos.line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }
        let line_char_idx = self.rope.line_to_char(pos.line);
        let line = self.rope.line(pos.line);
        let col = min(pos.column, line.len_chars());
        line_char_idx + col
    }

    /// Converts a 0-indexed character offset to a Line/Column position.
    pub fn char_offset_to_position(&self, char_offset: usize) -> Position {
        let offset = min(char_offset, self.rope.len_chars());
        let line = self.rope.char_to_line(offset);
        let line_start_char = self.rope.line_to_char(line);
        let col = offset - line_start_char;
        Position::new(line, col)
    }

    /// Retrieves a line slice as a RopeSlice.
    pub fn line(&self, line_idx: usize) -> Option<RopeSlice<'_>> {
        if line_idx < self.rope.len_lines() {
            Some(self.rope.line(line_idx))
        } else {
            None
        }
    }

    /// Applies a transaction to the buffer, updating rope and undo stack.
    pub fn apply_transaction(&mut self, transaction: Transaction) {
        if transaction.is_empty() {
            return;
        }

        for op in &transaction.ops {
            match op {
                EditOp::Insert { char_offset, text } => {
                    let offset = min(*char_offset, self.rope.len_chars());
                    self.rope.insert(offset, text);
                }
                EditOp::Delete { char_offset, text } => {
                    let start = min(*char_offset, self.rope.len_chars());
                    let len = text.chars().count();
                    let end = min(start + len, self.rope.len_chars());
                    if start < end {
                        self.rope.remove(start..end);
                    }
                }
            }
        }

        if !transaction.selections_after.is_empty() {
            self.set_selections(transaction.selections_after.clone());
        }

        self.undo_stack.push(transaction);
        self.redo_stack.clear();
        self.is_dirty = true;
    }

    /// Inserts text at all active cursors/selections simultaneously.
    pub fn insert_at_cursors(&mut self, text: &str) {
        let mut tx = Transaction::new();
        tx.selections_before = self.selections.clone();

        // Sort selections by document position
        let mut sorted_sels = self.selections.clone();
        sorted_sels.sort_by_key(|s| s.start());

        let text_char_len = text.chars().count();
        let mut new_selections = Vec::new();
        let mut accumulated_shift: isize = 0;

        for sel in &sorted_sels {
            let start_offset = self.position_to_char_offset(sel.start());
            let end_offset = self.position_to_char_offset(sel.end());
            let deleted_len = end_offset - start_offset;

            let new_char_offset = (start_offset as isize + accumulated_shift + text_char_len as isize) as usize;
            let new_pos = self.char_offset_to_position(new_char_offset);
            new_selections.push(Selection::point(new_pos));

            accumulated_shift += text_char_len as isize - deleted_len as isize;
        }

        // Generate EditOps in reverse order (bottom-to-top) so offset coordinates remain stable during sequential apply
        for sel in sorted_sels.iter().rev() {
            let start_offset = self.position_to_char_offset(sel.start());
            let end_offset = self.position_to_char_offset(sel.end());

            if start_offset < end_offset {
                let deleted_text = self.rope.slice(start_offset..end_offset).to_string();
                tx.ops.push(EditOp::Delete {
                    char_offset: start_offset,
                    text: deleted_text,
                });
            }

            tx.ops.push(EditOp::Insert {
                char_offset: start_offset,
                text: text.to_string(),
            });
        }

        tx.selections_after = new_selections;
        self.apply_transaction(tx);
    }

    /// Deletes characters at all active cursors (Backspace or Delete).
    pub fn delete_at_cursors(&mut self, backspace: bool) {
        let mut tx = Transaction::new();
        tx.selections_before = self.selections.clone();

        let mut sorted_sels = self.selections.clone();
        sorted_sels.sort_by_key(|s| s.start());

        let mut new_selections = Vec::new();

        for sel in sorted_sels.iter().rev() {
            let start_offset = self.position_to_char_offset(sel.start());
            let end_offset = self.position_to_char_offset(sel.end());

            if start_offset < end_offset {
                // Delete selected range
                let deleted = self.rope.slice(start_offset..end_offset).to_string();
                tx.ops.push(EditOp::Delete {
                    char_offset: start_offset,
                    text: deleted,
                });
                let pos = self.char_offset_to_position(start_offset);
                new_selections.push(Selection::point(pos));
            } else if backspace && start_offset > 0 {
                // Backspace single char
                let del_start = start_offset - 1;
                let deleted = self.rope.slice(del_start..start_offset).to_string();
                tx.ops.push(EditOp::Delete {
                    char_offset: del_start,
                    text: deleted,
                });
                let pos = self.char_offset_to_position(del_start);
                new_selections.push(Selection::point(pos));
            } else if !backspace && start_offset < self.rope.len_chars() {
                // Forward delete single char
                let del_end = start_offset + 1;
                let deleted = self.rope.slice(start_offset..del_end).to_string();
                tx.ops.push(EditOp::Delete {
                    char_offset: start_offset,
                    text: deleted,
                });
                let pos = self.char_offset_to_position(start_offset);
                new_selections.push(Selection::point(pos));
            }
        }

        new_selections.reverse();
        tx.selections_after = new_selections;
        self.apply_transaction(tx);
    }

    /// Performs an Undo operation.
    pub fn undo(&mut self) -> bool {
        if let Some(tx) = self.undo_stack.pop() {
            let inverted = tx.invert();
            for op in &inverted.ops {
                match op {
                    EditOp::Insert { char_offset, text } => {
                        let offset = min(*char_offset, self.rope.len_chars());
                        self.rope.insert(offset, text);
                    }
                    EditOp::Delete { char_offset, text } => {
                        let start = min(*char_offset, self.rope.len_chars());
                        let len = text.chars().count();
                        let end = min(start + len, self.rope.len_chars());
                        if start < end {
                            self.rope.remove(start..end);
                        }
                    }
                }
            }
            if !inverted.selections_after.is_empty() {
                self.set_selections(inverted.selections_after);
            }
            self.redo_stack.push(tx);
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    /// Performs a Redo operation.
    pub fn redo(&mut self) -> bool {
        if let Some(tx) = self.redo_stack.pop() {
            for op in &tx.ops {
                match op {
                    EditOp::Insert { char_offset, text } => {
                        let offset = min(*char_offset, self.rope.len_chars());
                        self.rope.insert(offset, text);
                    }
                    EditOp::Delete { char_offset, text } => {
                        let start = min(*char_offset, self.rope.len_chars());
                        let len = text.chars().count();
                        let end = min(start + len, self.rope.len_chars());
                        if start < end {
                            self.rope.remove(start..end);
                        }
                    }
                }
            }
            if !tx.selections_after.is_empty() {
                self.set_selections(tx.selections_after.clone());
            }
            self.undo_stack.push(tx);
            self.is_dirty = true;
            true
        } else {
            false
        }
    }

    /// Simple direct insert for non-cursor batch manipulation.
    pub fn insert(&mut self, char_offset: usize, text: &str) {
        let pos = self.char_offset_to_position(char_offset);
        self.set_selections(vec![Selection::point(pos)]);
        self.insert_at_cursors(text);
    }

    /// Simple direct delete for non-cursor batch manipulation.
    pub fn delete(&mut self, char_offset: usize, char_len: usize) {
        let start_pos = self.char_offset_to_position(char_offset);
        let end_pos = self.char_offset_to_position(char_offset + char_len);
        self.set_selections(vec![Selection::range(start_pos, end_pos)]);
        self.delete_at_cursors(false);
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

        assert!(buffer.undo());
        assert_eq!(buffer.to_string(), "");

        assert!(buffer.redo());
        assert_eq!(buffer.to_string(), "Hello, Oxide!");
    }

    #[test]
    fn test_multi_cursor_insertion() {
        let mut buffer = TextBuffer::from_str("foo\nbar\nbaz");
        buffer.set_selections(vec![
            Selection::point(Position::new(0, 3)),
            Selection::point(Position::new(1, 3)),
            Selection::point(Position::new(2, 3)),
        ]);

        buffer.insert_at_cursors("_123");
        assert_eq!(buffer.to_string(), "foo_123\nbar_123\nbaz_123");

        buffer.undo();
        assert_eq!(buffer.to_string(), "foo\nbar\nbaz");
    }

    #[test]
    fn test_multi_cursor_deletion() {
        let mut buffer = TextBuffer::from_str("apple\nbanana\ncherry");
        buffer.set_selections(vec![
            Selection::point(Position::new(0, 5)),
            Selection::point(Position::new(1, 6)),
            Selection::point(Position::new(2, 6)),
        ]);

        buffer.delete_at_cursors(true); // backspace
        assert_eq!(buffer.to_string(), "appl\nbanan\ncherr");
    }

    #[test]
    fn test_overlapping_selections_merge() {
        let mut buffer = TextBuffer::from_str("0123456789");
        buffer.set_selections(vec![
            Selection::range(Position::new(0, 1), Position::new(0, 4)),
            Selection::range(Position::new(0, 3), Position::new(0, 6)),
        ]);

        assert_eq!(buffer.selections().len(), 1);
        assert_eq!(
            buffer.selections()[0],
            Selection::range(Position::new(0, 1), Position::new(0, 6))
        );
    }

    #[test]
    fn test_streaming_reader_load() {
        use std::io::Cursor;
        let sample = "Alpha\nBeta\nGamma\nDelta";
        let reader = Cursor::new(sample.as_bytes());
        let buffer = TextBuffer::from_reader(reader).expect("failed to load from reader");
        assert_eq!(buffer.len_lines(), 4);
        assert_eq!(buffer.to_string(), sample);
    }

    #[test]
    fn test_batch_transaction() {
        let mut buffer = TextBuffer::from_str("The quick brown fox");
        let mut tx = Transaction::new();
        tx.ops.push(EditOp::Insert {
            char_offset: 19,
            text: " jumps".to_string(),
        });
        tx.ops.push(EditOp::Insert {
            char_offset: 25,
            text: " over".to_string(),
        });
        buffer.apply_transaction(tx);
        assert_eq!(buffer.to_string(), "The quick brown fox jumps over");

        buffer.undo();
        assert_eq!(buffer.to_string(), "The quick brown fox");

        buffer.redo();
        assert_eq!(buffer.to_string(), "The quick brown fox jumps over");
    }
}
