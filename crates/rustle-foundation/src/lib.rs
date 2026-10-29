//! Design-system-agnostic building blocks: layouts and basic elements.
//!
//! Everything here is a composable built on the primitives of `rustle-ui`.
//! Each element has a plain form and a `_with` form taking a [`Modifier`].

use rustle_geometry::{Offset, Size};
use rustle_graphics::Color;
use rustle_text::TextStyle;
use rustle_ui::{Children, Content, Layout, LayoutResult, Node, Overlay, emit, layout};
use rustle_unit::Constraints;

pub use rustle_ui::Modifier;

/// The color text is drawn in.
const TEXT_COLOR: Color = Color::rgb(235, 235, 240);

/// Emits a line of text.
pub fn text(content: impl Into<String>) {
    text_with(Modifier::new(), content);
}

/// Emits a line of text with `modifier`.
pub fn text_with(modifier: Modifier, content: impl Into<String>) {
    emit(Node {
        modifier,
        content: Content::Text {
            content: content.into(),
            color: TEXT_COLOR,
            style: TextStyle::default(),
        },
    });
}

/// Emits a column that places the children of `content` top to bottom.
pub fn column(content: impl FnOnce()) {
    column_with(Modifier::new(), content);
}

/// Emits a column with `modifier`.
pub fn column_with(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier, Column, content);
}

/// Emits a row that places the children of `content` left to right.
pub fn row(content: impl FnOnce()) {
    row_with(Modifier::new(), content);
}

/// Emits a row with `modifier`.
pub fn row_with(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier, Row, content);
}

/// Emits a stack that places the children of `content` on top of each other.
///
/// This is Compose's `Box`, which Rust reserves as a keyword.
pub fn stack(content: impl FnOnce()) {
    stack_with(Modifier::new(), content);
}

/// Emits a stack with `modifier`.
pub fn stack_with(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier, Overlay, content);
}

/// Emits empty space sized by `modifier`, such as `Modifier::new().height(8.0)`.
pub fn spacer(modifier: Modifier) {
    stack_with(modifier, || {});
}

struct Column;

impl Layout for Column {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let mut positions = Vec::with_capacity(children.len());
        let mut width: f32 = 0.0;
        let mut y = 0.0;
        for index in 0..children.len() {
            let child_constraints = Constraints {
                min_width: 0.0,
                min_height: 0.0,
                max_height: (constraints.max_height - y).max(0.0),
                ..constraints
            };
            let size = children.measure(index, child_constraints);
            positions.push(Offset::new(0.0, y));
            width = width.max(size.width);
            y += size.height;
        }
        LayoutResult {
            size: Size::new(width, y),
            positions,
        }
    }
}

struct Row;

impl Layout for Row {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let mut positions = Vec::with_capacity(children.len());
        let mut height: f32 = 0.0;
        let mut x = 0.0;
        for index in 0..children.len() {
            let child_constraints = Constraints {
                min_width: 0.0,
                min_height: 0.0,
                max_width: (constraints.max_width - x).max(0.0),
                ..constraints
            };
            let size = children.measure(index, child_constraints);
            positions.push(Offset::new(x, 0.0));
            height = height.max(size.height);
            x += size.width;
        }
        LayoutResult {
            size: Size::new(x, height),
            positions,
        }
    }
}
