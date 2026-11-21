//! Icons are [Phosphor](https://phosphoricons.com) outlines: `icon(phosphor::PLUS, 16.0, color)`,
//! or SVG documents loaded while the program runs: `svg_icon(modifier, &painter, 16.0, Some(color))`.

use rustle_foundation::{ContentScale, Modifier, canvas, svg_image};
use rustle_graphics::{Color, SvgPainter};
use rustle_icons::Icon;

/// Emits `icon`, `size` square, filled with `color`.
#[track_caller]
pub fn icon(icon: Icon, size: f32, color: Color) {
    icon_with(Modifier::new(), icon, size, color);
}

/// Emits `icon`, `size` square, filled with `color`, with `modifier` around it, as Compose's
/// `Icon(imageVector, contentDescription, modifier, tint)`: a size set by `modifier` wins over
/// `size`, and the icon is fitted inside the area it gets, centered.
#[track_caller]
pub fn icon_with(modifier: Modifier, icon: Icon, size: f32, color: Color) {
    canvas(modifier.size(size, size), move |canvas| {
        let side = canvas.width().min(canvas.height());
        let x = (canvas.width() - side) / 2.0;
        let y = (canvas.height() - side) / 2.0;
        canvas.path(&icon, x, y, side, side, color);
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
