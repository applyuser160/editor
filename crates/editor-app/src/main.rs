use editor_core::TextBuffer;
use editor_markdown::MarkdownPreview;
use editor_search::{SearchEngine, SearchQuery};
use editor_syntax::{LanguageId, SyntaxEngine};
use editor_ui::{Theme, UiLayout, Viewport};
use editor_workspace::Workspace;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt::init();
    tracing::info!("Starting Oxide Editor (🦀 Lightweight VS Code Alternative in Rust)...");

    // 1. Core Engine TextBuffer initialization
    let mut buffer = TextBuffer::new();
    buffer.insert(0, "# Welcome to Oxide Editor\n\nA memory-efficient IDE written in Rust.\n\nfn main() {\n    println!(\"Hello, Oxide!\");\n}");
    tracing::info!("Buffer initialized: {} chars, {} lines.", buffer.len_chars(), buffer.len_lines());

    // 2. Syntax Tokenization
    let syntax = SyntaxEngine::new();
    let tokens = syntax.highlight(&buffer.to_string(), LanguageId::Rust);
    tracing::info!("Syntax highlighted: {} tokens identified.", tokens.len());

    // 3. Search Engine
    let query = SearchQuery::new("Oxide");
    let matches = SearchEngine::search(&buffer.to_string(), &query)?;
    tracing::info!("Search matches: {} found for '{}'.", matches.len(), query.pattern);

    // 4. Markdown GFM Preview
    let preview_html = MarkdownPreview::render_html(&buffer.to_string());
    tracing::info!("Markdown live preview rendered: {} HTML bytes.", preview_html.len());

    // 5. Workspace & File Tree
    let workspace = Workspace::new(Some(std::env::current_dir().unwrap_or_default()));
    tracing::info!("Workspace initialized with {} tab groups.", workspace.tab_groups.len());

    // 6. UI Layout & Viewport calculation
    let layout = UiLayout::compute(1920.0, 1080.0, 260.0, Some(280.0));
    let viewport = Viewport::new(layout.editor_rect.width, layout.editor_rect.height, 20.0, 9.0);
    let (start_line, end_line) = viewport.visible_line_range(buffer.len_lines());
    let theme = Theme::default();

    println!("===============================================================");
    println!("  🦀 Oxide Editor v{} Initialized Successfully", env!("CARGO_PKG_VERSION"));
    println!("  Theme: {} | Visible Lines: {}..{}", theme.name, start_line, end_line);
    println!("  Editor Size: {}x{} px | Terminal Size: {}x{} px", 
        layout.editor_rect.width, layout.editor_rect.height,
        layout.terminal_rect.map_or(0.0, |r| r.width),
        layout.terminal_rect.map_or(0.0, |r| r.height)
    );
    println!("===============================================================");

    Ok(())
}
