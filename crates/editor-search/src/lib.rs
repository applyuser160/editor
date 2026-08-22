//! `editor-search`: In-file search/replace, project-wide search, and fuzzy file finder.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SearchError {
    #[error("Invalid regular expression: {0}")]
    InvalidRegex(#[from] regex::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Search query configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub pattern: String,
    pub is_regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
}

impl SearchQuery {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: pattern.to_string(),
            is_regex: false,
            match_case: false,
            whole_word: false,
        }
    }
}

/// A match found during search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line_number: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub line_content: String,
    pub match_text: String,
}

/// Result of a project-wide search grouped by file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSearchResult {
    pub file_path: PathBuf,
    pub matches: Vec<SearchMatch>,
}

/// Search engine for in-buffer search and replace operations.
pub struct SearchEngine;

impl SearchEngine {
    /// Searches in text content using specified search options.
    pub fn search(text: &str, query: &SearchQuery) -> Result<Vec<SearchMatch>, SearchError> {
        if query.pattern.is_empty() {
            return Ok(Vec::new());
        }

        let regex_pattern = if query.is_regex {
            if query.match_case {
                query.pattern.clone()
            } else {
                format!("(?i){}", query.pattern)
            }
        } else {
            let escaped = regex::escape(&query.pattern);
            let word_bounded = if query.whole_word {
                format!(r"\b{}\b", escaped)
            } else {
                escaped
            };
            if query.match_case {
                word_bounded
            } else {
                format!("(?i){}", word_bounded)
            }
        };

        let re = Regex::new(&regex_pattern)?;
        let mut matches = Vec::new();

        for (line_idx, line) in text.lines().enumerate() {
            for mat in re.find_iter(line) {
                matches.push(SearchMatch {
                    line_number: line_idx,
                    start_col: mat.start(),
                    end_col: mat.end(),
                    line_content: line.to_string(),
                    match_text: mat.as_str().to_string(),
                });
            }
        }

        Ok(matches)
    }

    /// Replaces matches in text with replacement string.
    pub fn replace_all(text: &str, query: &SearchQuery, replacement: &str) -> Result<String, SearchError> {
        if query.pattern.is_empty() {
            return Ok(text.to_string());
        }

        let regex_pattern = if query.is_regex {
            if query.match_case {
                query.pattern.clone()
            } else {
                format!("(?i){}", query.pattern)
            }
        } else {
            let escaped = regex::escape(&query.pattern);
            if query.match_case {
                escaped
            } else {
                format!("(?i){}", escaped)
            }
        };

        let re = Regex::new(&regex_pattern)?;
        Ok(re.replace_all(text, replacement).to_string())
    }

    /// Performs a project-wide search across a folder.
    pub fn search_project(root: &Path, query: &SearchQuery) -> Result<Vec<FileSearchResult>, SearchError> {
        let mut results = Vec::new();
        Self::search_dir_recursive(root, query, &mut results)?;
        Ok(results)
    }

    fn search_dir_recursive(dir: &Path, query: &SearchQuery, out: &mut Vec<FileSearchResult>) -> Result<(), SearchError> {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();

                if name == ".git" || name == "target" || name == "node_modules" {
                    continue;
                }

                if path.is_dir() {
                    Self::search_dir_recursive(&path, query, out)?;
                } else if path.is_file() {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let matches = Self::search(&content, query)?;
                        if !matches.is_empty() {
                            out.push(FileSearchResult {
                                file_path: path,
                                matches,
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Fuzzy match score calculator for Quick Open (Ctrl+P).
pub struct FuzzyFinder;

impl FuzzyFinder {
    /// Returns matching score (None if pattern does not match as subsequence).
    pub fn score(candidate: &str, pattern: &str) -> Option<i64> {
        if pattern.is_empty() {
            return Some(0);
        }

        let cand_lower = candidate.to_lowercase();
        let pat_lower = pattern.to_lowercase();

        let mut pat_chars = pat_lower.chars().peekable();
        let mut score: i64 = 0;
        let mut last_match_idx: Option<usize> = None;

        for (idx, ch) in cand_lower.char_indices() {
            if let Some(&pat_ch) = pat_chars.peek() {
                if ch == pat_ch {
                    pat_chars.next();

                    // Bonus for consecutive matches
                    if let Some(prev) = last_match_idx {
                        if prev + 1 == idx {
                            score += 10;
                        }
                    }

                    // Bonus for start of word / boundary
                    if idx == 0 || candidate.as_bytes().get(idx.saturating_sub(1)).map_or(false, |b| *b == b'/' || *b == b'\\' || *b == b'_' || *b == b'-') {
                        score += 20;
                    }

                    score += 5;
                    last_match_idx = Some(idx);
                }
            }
        }

        if pat_chars.peek().is_none() {
            Some(score)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_and_replace() {
        let text = "Hello world!\nHello Oxide!\nHELLO Rust!";
        let mut query = SearchQuery::new("hello");
        query.match_case = false;

        let matches = SearchEngine::search(text, &query).unwrap();
        assert_eq!(matches.len(), 3);

        let replaced = SearchEngine::replace_all(text, &query, "Greetings").unwrap();
        assert_eq!(replaced, "Greetings world!\nGreetings Oxide!\nGreetings Rust!");
    }

    #[test]
    fn test_regex_search() {
        let text = "let val1 = 100;\nlet val2 = 200;";
        let mut query = SearchQuery::new(r"val\d");
        query.is_regex = true;

        let matches = SearchEngine::search(text, &query).unwrap();
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].match_text, "val1");
        assert_eq!(matches[1].match_text, "val2");
    }

    #[test]
    fn test_fuzzy_finder() {
        let file1 = "crates/editor-core/src/lib.rs";
        let file2 = "README.md";

        let score1 = FuzzyFinder::score(file1, "edcore");
        assert!(score1.is_some());

        let score2 = FuzzyFinder::score(file2, "edcore");
        assert!(score2.is_none());
    }
}
