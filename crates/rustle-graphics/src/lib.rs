//! What a renderer draws: backend-independent drawing commands.

/// Font size of all text, in logical pixels.
pub const FONT_SIZE: f32 = 18.0;
/// Height reserved for one line of text, in logical pixels.
pub const LINE_HEIGHT: f32 = 24.0;

/// A backend-independent drawing instruction produced by layout.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Text { x: f32, y: f32, content: String },
}
