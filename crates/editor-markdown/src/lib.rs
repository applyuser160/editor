use pulldown_cmark::{html, Options, Parser};
use serde::{Deserialize, Serialize};

/// Heading item for Table of Contents (TOC) navigation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocHeading {
    pub level: usize,
    pub title: String,
    pub line_number: usize,
}

/// Markdown preview engine with GFM support and scroll mapping.
pub struct MarkdownPreview;

impl MarkdownPreview {
    /// Renders Markdown source into clean, GFM-compliant HTML.
    pub fn render_html(markdown_text: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = Parser::new_ext(markdown_text, options);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);
        html_output
    }

    /// Extracts table of contents (TOC) with line numbers for instant outline jump.
    pub fn extract_toc(markdown_text: &str) -> Vec<TocHeading> {
        let mut headings = Vec::new();

        for (line_idx, line) in markdown_text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                let hash_count = trimmed.chars().take_while(|&c| c == '#').count();
                if hash_count <= 6 && trimmed.as_bytes().get(hash_count) == Some(&b' ') {
                    let title = trimmed[hash_count + 1..].trim().to_string();
                    headings.push(TocHeading {
                        level: hash_count,
                        title,
                        line_number: line_idx,
                    });
                }
            }
        }

        headings
    }

    /// Generates mapping of source line numbers to rendered blocks for synchronized scrolling.
    pub fn compute_line_scroll_ratio(current_line: usize, total_lines: usize) -> f64 {
        if total_lines <= 1 {
            0.0
        } else {
            current_line as f64 / (total_lines - 1) as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_gfm_features() {
        let md = r#"# Hello Markdown

| Name | Type |
| :--- | :--- |
| Rust | Lang |

- [ ] Task 1
- [x] Task 2
"#;
        let html = MarkdownPreview::render_html(md);
        assert!(html.contains("<h1>Hello Markdown</h1>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("type=\"checkbox\""));
    }

    #[test]
    fn test_toc_extraction() {
        let md = r#"# Main Title
Intro text

## Section 1
Details

### Subsection 1.1
Deep details
"#;
        let toc = MarkdownPreview::extract_toc(md);
        assert_eq!(toc.len(), 3);
        assert_eq!(toc[0].title, "Main Title");
        assert_eq!(toc[0].level, 1);
        assert_eq!(toc[1].title, "Section 1");
        assert_eq!(toc[1].level, 2);
        assert_eq!(toc[2].title, "Subsection 1.1");
        assert_eq!(toc[2].level, 3);
    }
}
