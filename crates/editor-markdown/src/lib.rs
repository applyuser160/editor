//! `editor-markdown`: Markdown parsing and live preview renderer.

use pulldown_cmark::{html, Parser, Options};

pub struct MarkdownPreview;

impl MarkdownPreview {
    /// Parses Markdown source and produces rendered HTML for preview.
    pub fn render_html(markdown_text: &str) -> String {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_FOOTNOTES);
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TASKLISTS);

        let parser = Parser::new_ext(markdown_text, options);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);
        html_output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_markdown() {
        let md = "# Title\n\n- [ ] Task 1\n- [x] Task 2";
        let html = MarkdownPreview::render_html(md);
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("type=\"checkbox\""));
    }
}
