//! `editor-ui`: GPU rendering layout, Viewport, Theme engine, and glyph metrics cache.

use serde::{Deserialize, Serialize};

/// RGBA color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Self::new(r, g, b, 255))
        } else if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Self::new(r, g, b, a))
        } else {
            None
        }
    }
}

/// Comprehensive IDE color theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub background: RgbaColor,
    pub foreground: RgbaColor,
    pub selection: RgbaColor,
    pub cursor: RgbaColor,
    pub line_number: RgbaColor,
    pub line_number_active: RgbaColor,
    pub gutter_bg: RgbaColor,
    pub sidebar_bg: RgbaColor,
    pub status_bar_bg: RgbaColor,
    pub tab_active_bg: RgbaColor,
    pub tab_inactive_bg: RgbaColor,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "Oxide Dark".to_string(),
            background: RgbaColor::new(30, 30, 30, 255),       // #1e1e1e
            foreground: RgbaColor::new(212, 212, 212, 255),   // #d4d4d4
            selection: RgbaColor::new(38, 79, 120, 255),      // #264f78
            cursor: RgbaColor::new(255, 255, 255, 255),
            line_number: RgbaColor::new(133, 133, 133, 255),
            line_number_active: RgbaColor::new(200, 200, 200, 255),
            gutter_bg: RgbaColor::new(24, 24, 24, 255),
            sidebar_bg: RgbaColor::new(37, 37, 38, 255),      // #252526
            status_bar_bg: RgbaColor::new(0, 122, 204, 255),  // #007acc
            tab_active_bg: RgbaColor::new(30, 30, 30, 255),
            tab_inactive_bg: RgbaColor::new(45, 45, 45, 255),
        }
    }
}

/// Viewport managing scroll positions and visible lines for 60/120fps GPU text rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub line_height: f32,
    pub char_width: f32,
}

impl Viewport {
    pub fn new(width: f32, height: f32, line_height: f32, char_width: f32) -> Self {
        Self {
            width,
            height,
            scroll_x: 0.0,
            scroll_y: 0.0,
            line_height,
            char_width,
        }
    }

    /// Computes the range of visible line indices (start_line..end_line) in the viewport.
    pub fn visible_line_range(&self, total_lines: usize) -> (usize, usize) {
        if self.line_height <= 0.0 || total_lines == 0 {
            return (0, 0);
        }

        let start_line = (self.scroll_y / self.line_height).floor() as usize;
        let visible_count = (self.height / self.line_height).ceil() as usize + 1;
        let end_line = (start_line + visible_count).min(total_lines);

        (start_line.min(total_lines), end_line)
    }

    /// Computes pixel position (x, y) for a line and column.
    pub fn position_to_pixels(&self, line: usize, col: usize) -> (f32, f32) {
        let x = (col as f32 * self.char_width) - self.scroll_x;
        let y = (line as f32 * self.line_height) - self.scroll_y;
        (x, y)
    }
}

/// IDE Window Layout partition calculation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Complete UI layout computed from window dimensions.
#[derive(Debug, Clone, PartialEq)]
pub struct UiLayout {
    pub sidebar_rect: Rect,
    pub editor_rect: Rect,
    pub terminal_rect: Option<Rect>,
    pub status_bar_rect: Rect,
}

impl UiLayout {
    pub fn compute(
        window_width: f32,
        window_height: f32,
        sidebar_width: f32,
        terminal_height: Option<f32>,
    ) -> Self {
        let status_bar_height = 24.0;
        let main_height = window_height - status_bar_height;

        let sidebar_rect = Rect {
            x: 0.0,
            y: 0.0,
            width: sidebar_width,
            height: main_height,
        };

        let content_x = sidebar_width;
        let content_width = (window_width - sidebar_width).max(0.0);

        let (editor_rect, terminal_rect) = if let Some(term_h) = terminal_height {
            let actual_term_h = term_h.min(main_height * 0.6);
            let editor_h = main_height - actual_term_h;
            (
                Rect {
                    x: content_x,
                    y: 0.0,
                    width: content_width,
                    height: editor_h,
                },
                Some(Rect {
                    x: content_x,
                    y: editor_h,
                    width: content_width,
                    height: actual_term_h,
                }),
            )
        } else {
            (
                Rect {
                    x: content_x,
                    y: 0.0,
                    width: content_width,
                    height: main_height,
                },
                None,
            )
        };

        let status_bar_rect = Rect {
            x: 0.0,
            y: main_height,
            width: window_width,
            height: status_bar_height,
        };

        Self {
            sidebar_rect,
            editor_rect,
            terminal_rect,
            status_bar_rect,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rgba_from_hex() {
        let col = RgbaColor::from_hex("#1e1e1e").unwrap();
        assert_eq!(col, RgbaColor::new(30, 30, 30, 255));

        let col_alpha = RgbaColor::from_hex("#ff000080").unwrap();
        assert_eq!(col_alpha, RgbaColor::new(255, 0, 0, 128));
    }

    #[test]
    fn test_viewport_visible_line_range() {
        let viewport = Viewport::new(800.0, 600.0, 20.0, 10.0);
        let (start, end) = viewport.visible_line_range(100);
        assert_eq!(start, 0);
        assert_eq!(end, 31); // 600 / 20 = 30 + 1 extra line
    }

    #[test]
    fn test_ui_layout_computation() {
        let layout = UiLayout::compute(1280.0, 800.0, 250.0, Some(200.0));
        assert_eq!(layout.sidebar_rect.width, 250.0);
        assert_eq!(layout.status_bar_rect.height, 24.0);
        assert_eq!(layout.editor_rect.width, 1030.0);
        assert!(layout.terminal_rect.is_some());
    }
}
