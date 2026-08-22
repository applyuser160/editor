//! `editor-ui`: GPU rendering abstractions, layout tree, and window events.

pub struct Theme {
    pub name: String,
    pub background: String,
    pub foreground: String,
    pub selection: String,
    pub line_number: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Oxide Dark".to_string(),
            background: "#1e1e1e".to_string(),
            foreground: "#d4d4d4".to_string(),
            selection: "#264f78".to_string(),
            line_number: "#858585".to_string(),
        }
    }
}
