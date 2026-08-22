use editor_core::TextBuffer;
use editor_markdown::MarkdownPreview;
use editor_search::SearchEngine;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Oxide Editor (🦀 Lightweight VS Code Alternative in Rust)...");

    // Demonstration of core engine buffer
    let mut buffer = TextBuffer::new();
    buffer.insert(0, "# Welcome to Oxide Editor\n\nA memory-efficient IDE in Rust.");
    tracing::info!("Buffer initialized with {} lines.", buffer.len_lines());

    // Demonstration of search
    let matches = SearchEngine::search_in_text(&buffer.to_string(), "Oxide", true);
    tracing::info!("Found {} match(es) in buffer.", matches.len());

    // Demonstration of Markdown preview rendering
    let html = MarkdownPreview::render_html(&buffer.to_string());
    tracing::info!("Markdown rendered HTML len: {} bytes.", html.len());

    println!("=================================================");
    println!("  Oxide Editor v{} initialized successfully.", env!("CARGO_PKG_VERSION"));
    println!("  Lightweight, Fast, Memory-efficient VS Code Alternative.");
    println!("=================================================");

    Ok(())
}
