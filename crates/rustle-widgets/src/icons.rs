//! Icons are [Phosphor](https://phosphoricons.com) outlines: `icon(phosphor::PLUS, 16.0, color)`,
//! or SVG documents loaded while the program runs: `svg_icon(modifier, &painter, 16.0, Some(color))`.

use rustle_foundation::{ContentScale, Modifier, canvas, svg_image};
use rustle_graphics::{Color, SvgPainter};
use rustle_icons::Icon;

/// Emits `icon`, `size` square, filled with `color`.
#[track_caller]
pub fn icon(icon: Icon, size: f32, color: Color) {
    canvas(Modifier::new().size(size, size), move |canvas| {
        canvas.path(&icon, 0.0, 0.0, size, size, color);
    });
}

/// Emits the SVG document `painter`, `size` square, with `modifier` around it, as Compose's
/// `Icon(painter, contentDescription, modifier, tint)`: filled with `tint` where it covers, or in
/// its own colors with none, as Compose's `Color.Unspecified` tint draws it. A document that is
/// not square is fitted inside, centered.
#[track_caller]
pub fn svg_icon(modifier: Modifier, painter: &SvgPainter, size: f32, tint: Option<Color>) {
    svg_image(modifier.size(size, size), painter, ContentScale::Fit, tint);
}
