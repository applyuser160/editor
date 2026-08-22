use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Semantic and syntactic token types for coloring and styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TokenType {
    Keyword,
    ControlFlow,
    Function,
    Method,
    Type,
    Struct,
    Enum,
    Trait,
    Interface,
    Variable,
    Parameter,
    Constant,
    String,
    Number,
    Boolean,
    Comment,
    DocComment,
    Operator,
    Punctuation,
    Macro,
    Attribute,
    Unknown,
}

/// A contiguous span of highlighted text in a buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSpan {
    pub start_offset: usize,
    pub end_offset: usize,
    pub token_type: TokenType,
}

impl HighlightSpan {
    pub fn new(start_offset: usize, end_offset: usize, token_type: TokenType) -> Self {
        Self {
            start_offset,
            end_offset,
            token_type,
        }
    }
}

/// Supported language identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LanguageId {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Cpp,
    C,
    Markdown,
    Json,
    Yaml,
    Toml,
    PlainText,
}

impl LanguageId {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "rs" => Self::Rust,
            "ts" | "tsx" => Self::TypeScript,
            "js" | "jsx" => Self::JavaScript,
            "py" => Self::Python,
            "go" => Self::Go,
            "cpp" | "cc" | "cxx" | "hpp" | "h" => Self::Cpp,
            "c" => Self::C,
            "md" | "markdown" => Self::Markdown,
            "json" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            _ => Self::PlainText,
        }
    }
}

/// Syntax highlight engine that analyzes code tokens and manages syntax trees.
pub struct SyntaxEngine {
    rust_keywords: HashMap<&'static str, TokenType>,
}

impl SyntaxEngine {
    pub fn new() -> Self {
        let mut rust_keywords = HashMap::new();
        for kw in ["fn", "let", "mut", "pub", "struct", "enum", "trait", "impl", "use", "mod", "crate", "type", "const", "static", "where", "as", "ref"] {
            rust_keywords.insert(kw, TokenType::Keyword);
        }
        for kw in ["if", "else", "match", "for", "while", "loop", "return", "break", "continue", "in", "yield", "await"] {
            rust_keywords.insert(kw, TokenType::ControlFlow);
        }
        for kw in ["true", "false"] {
            rust_keywords.insert(kw, TokenType::Boolean);
        }

        Self { rust_keywords }
    }

    /// Highlights a complete text slice for a given language.
    pub fn highlight(&self, text: &str, language: LanguageId) -> Vec<HighlightSpan> {
        match language {
            LanguageId::Rust => self.highlight_rust(text),
            LanguageId::Json => self.highlight_json(text),
            _ => self.highlight_generic(text),
        }
    }

    /// Fast Rust tokenizer and highlighter.
    fn highlight_rust(&self, text: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            let (byte_idx, c) = chars[i];

            // Line comment or Doc comment
            if c == '/' && i + 1 < len && chars[i + 1].1 == '/' {
                let start = byte_idx;
                let is_doc = i + 2 < len && (chars[i + 2].1 == '/' || chars[i + 2].1 == '!');
                while i < len && chars[i].1 != '\n' {
                    i += 1;
                }
                let end = if i < len { chars[i].0 } else { text.len() };
                spans.push(HighlightSpan::new(
                    start,
                    end,
                    if is_doc { TokenType::DocComment } else { TokenType::Comment },
                ));
                continue;
            }

            // Block comment
            if c == '/' && i + 1 < len && chars[i + 1].1 == '*' {
                let start = byte_idx;
                i += 2;
                while i + 1 < len && !(chars[i].1 == '*' && chars[i + 1].1 == '/') {
                    i += 1;
                }
                i = (i + 2).min(len);
                let end = if i < len { chars[i].0 } else { text.len() };
                spans.push(HighlightSpan::new(start, end, TokenType::Comment));
                continue;
            }

            // String literal
            if c == '"' {
                let start = byte_idx;
                i += 1;
                let mut escaped = false;
                while i < len {
                    let (_, ch) = chars[i];
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let end = if i < len { chars[i].0 } else { text.len() };
                spans.push(HighlightSpan::new(start, end, TokenType::String));
                continue;
            }

            // Numbers
            if c.is_ascii_digit() {
                let start = byte_idx;
                while i < len && (chars[i].1.is_ascii_alphanumeric() || chars[i].1 == '.' || chars[i].1 == '_') {
                    i += 1;
                }
                let end = if i < len { chars[i].0 } else { text.len() };
                spans.push(HighlightSpan::new(start, end, TokenType::Number));
                continue;
            }

            // Identifiers / Keywords
            if c.is_alphabetic() || c == '_' {
                let start = byte_idx;
                while i < len && (chars[i].1.is_alphanumeric() || chars[i].1 == '_') {
                    i += 1;
                }
                let end = if i < len { chars[i].0 } else { text.len() };
                let word = &text[start..end];

                let token_type = if let Some(kw_type) = self.rust_keywords.get(word) {
                    *kw_type
                } else if word.chars().next().map_or(false, |first| first.is_uppercase()) {
                    TokenType::Type
                } else if i < len && chars[i].1 == '(' {
                    TokenType::Function
                } else if i < len && chars[i].1 == '!' {
                    TokenType::Macro
                } else {
                    TokenType::Variable
                };

                spans.push(HighlightSpan::new(start, end, token_type));
                continue;
            }

            // Operators / Punctuation
            if "+-*/%=<>!&|^~?".contains(c) {
                spans.push(HighlightSpan::new(byte_idx, byte_idx + c.len_utf8(), TokenType::Operator));
            } else if "{}[](),;.:".contains(c) {
                spans.push(HighlightSpan::new(byte_idx, byte_idx + c.len_utf8(), TokenType::Punctuation));
            }

            i += 1;
        }

        spans
    }

    /// Fast JSON tokenizer.
    fn highlight_json(&self, text: &str) -> Vec<HighlightSpan> {
        let mut spans = Vec::new();
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            let (byte_idx, c) = chars[i];

            if c == '"' {
                let start = byte_idx;
                i += 1;
                let mut escaped = false;
                while i < len {
                    let (_, ch) = chars[i];
                    if escaped {
                        escaped = false;
                    } else if ch == '\\' {
                        escaped = true;
                    } else if ch == '"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                let end = if i < len { chars[i].0 } else { text.len() };

                // Look ahead for colon (indicating a JSON key)
                let mut j = i;
                while j < len && chars[j].1.is_whitespace() {
                    j += 1;
                }
                let token_type = if j < len && chars[j].1 == ':' {
                    TokenType::Keyword
                } else {
                    TokenType::String
                };

                spans.push(HighlightSpan::new(start, end, token_type));
                continue;
            }

            if c.is_ascii_digit() || c == '-' {
                let start = byte_idx;
                while i < len && (chars[i].1.is_ascii_digit() || chars[i].1 == '.' || chars[i].1 == 'e' || chars[i].1 == 'E' || chars[i].1 == '+' || chars[i].1 == '-') {
                    i += 1;
                }
                let end = if i < len { chars[i].0 } else { text.len() };
                spans.push(HighlightSpan::new(start, end, TokenType::Number));
                continue;
            }

            if "{}[],:".contains(c) {
                spans.push(HighlightSpan::new(byte_idx, byte_idx + c.len_utf8(), TokenType::Punctuation));
            }

            i += 1;
        }

        spans
    }

    /// Generic token highlighter for fallback.
    fn highlight_generic(&self, text: &str) -> Vec<HighlightSpan> {
        let spans = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let _ = (idx, line);
        }
        spans
    }
}

impl Default for SyntaxEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_highlighting() {
        let engine = SyntaxEngine::new();
        let code = "fn main() {\n    let count = 42;\n    // Comment\n}";
        let spans = engine.highlight(code, LanguageId::Rust);

        assert!(!spans.is_empty());
        let fn_kw = spans.iter().find(|s| s.token_type == TokenType::Keyword).unwrap();
        assert_eq!(&code[fn_kw.start_offset..fn_kw.end_offset], "fn");

        let number = spans.iter().find(|s| s.token_type == TokenType::Number).unwrap();
        assert_eq!(&code[number.start_offset..number.end_offset], "42");

        let comment = spans.iter().find(|s| s.token_type == TokenType::Comment).unwrap();
        assert_eq!(&code[comment.start_offset..comment.end_offset], "// Comment");
    }

    #[test]
    fn test_json_highlighting() {
        let engine = SyntaxEngine::new();
        let json = r#"{"name": "Oxide", "version": 1}"#;
        let spans = engine.highlight(json, LanguageId::Json);

        assert!(!spans.is_empty());
        let key = spans.iter().find(|s| s.token_type == TokenType::Keyword).unwrap();
        assert_eq!(&json[key.start_offset..key.end_offset], "\"name\"");

        let str_val = spans.iter().find(|s| s.token_type == TokenType::String).unwrap();
        assert_eq!(&json[str_val.start_offset..str_val.end_offset], "\"Oxide\"");
    }
}
