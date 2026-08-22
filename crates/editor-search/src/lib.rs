//! `editor-search`: In-file search/replace and multi-threaded project search engine.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line_number: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub line_content: String,
}

pub struct SearchEngine;

impl SearchEngine {
    pub fn search_in_text(text: &str, query: &str, case_sensitive: bool) -> Vec<SearchMatch> {
        let mut results = Vec::new();
        let query_normalized = if case_sensitive {
            query.to_string()
        } else {
            query.to_lowercase()
        };

        for (line_idx, line) in text.lines().enumerate() {
            let line_to_search = if case_sensitive {
                line.to_string()
            } else {
                line.to_lowercase()
            };

            let mut start = 0;
            while let Some(pos) = line_to_search[start..].find(&query_normalized) {
                let actual_start = start + pos;
                let actual_end = actual_start + query.len();
                results.push(SearchMatch {
                    line_number: line_idx,
                    start_col: actual_start,
                    end_col: actual_end,
                    line_content: line.to_string(),
                });
                start = actual_end;
            }
        }
        results
    }
}
