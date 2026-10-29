//! What a renderer draws: backend-independent drawing commands.

use rustle_geometry::Rect;

/// An sRGB color with straight alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// A backend-independent drawing instruction produced by layout, in paint order.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// A filled rectangle with rounded corners.
    Rect {
        rect: Rect,
        color: Color,
        corner_radius: f32,
    },
    /// One line of text whose line box starts at (`x`, `y`).
    Text {
        x: f32,
        y: f32,
        content: String,
        color: Color,
        font_size: f32,
        line_height: f32,
    },
}
