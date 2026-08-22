//! `editor-syntax`: Syntax parsing and token highlighting engine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenType {
    Keyword,
    Function,
    Type,
    Variable,
    String,
    Number,
    Comment,
    Punctuation,
    Operator,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightToken {
    pub start_offset: usize,
    pub end_offset: usize,
    pub token_type: TokenType,
}

pub trait SyntaxParser: Send + Sync {
    fn parse(&mut self, text: &str) -> Vec<HighlightToken>;
}

/// Fallback basic syntax highlighter for early stages.
#[derive(Default)]
pub struct BasicHighlighter;

impl SyntaxParser for BasicHighlighter {
    fn parse(&mut self, text: &str) -> Vec<HighlightToken> {
        let tokens = Vec::new();
        // Basic demonstration parser
        for (idx, line) in text.lines().enumerate() {
            let _ = (idx, line);
        }
        tokens
    }
}
