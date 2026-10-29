//! Styled, ready-made components such as buttons, with their look.
//!
//! Built only from `rustle-foundation`.

use rustle_foundation::{Modifier, row_with, text};
use rustle_graphics::Color;

const BUTTON_COLOR: Color = Color::rgb(74, 96, 214);
const BUTTON_CORNER_RADIUS: f32 = 8.0;

/// Emits a button showing `label` that runs `on_click` when clicked.
pub fn button(label: impl Into<String>, on_click: impl Fn() + 'static) {
    button_with(Modifier::new(), label, on_click);
}

/// Emits a button with `modifier`, which applies outside the button's own look.
pub fn button_with(modifier: Modifier, label: impl Into<String>, on_click: impl Fn() + 'static) {
    let label = label.into();
    row_with(
        modifier
            .clickable(on_click)
            .background_rounded(BUTTON_COLOR, BUTTON_CORNER_RADIUS)
            .padding_xy(20.0, 10.0),
        || text(label),
    );
}
