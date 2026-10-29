//! Icons are [Phosphor](https://phosphoricons.com) outlines: `icon(phosphor::PLUS, 16.0, color)`.

use rustle_foundation::{Modifier, canvas};
use rustle_graphics::Color;
use rustle_icons::Icon;

/// Emits `icon`, `size` square, filled with `color`.
#[track_caller]
pub fn icon(icon: Icon, size: f32, color: Color) {
    canvas(Modifier::new().size(size, size), move |canvas| {
        canvas.path(&icon, 0.0, 0.0, size, size, color);
    });
}
