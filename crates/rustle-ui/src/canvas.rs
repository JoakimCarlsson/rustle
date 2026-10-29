#![allow(clippy::too_many_arguments)]

//! A canvas: an area drawn with shapes, lines and text in its own coordinates.

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, Color, DrawCommand, FontFamily, Path, TextAlign};

/// Collects what is drawn on a canvas, with (0, 0) at its top-left corner.
pub struct Canvas {
    size: Size,
    commands: Vec<DrawCommand>,
}

impl Canvas {
    pub(crate) fn new(size: Size) -> Self {
        Self {
            size,
            commands: Vec::new(),
        }
    }

    pub(crate) fn finish(self) -> Vec<DrawCommand> {
        self.commands
    }

    /// The size the canvas was laid out at.
    pub fn size(&self) -> Size {
        self.size
    }

    pub fn width(&self) -> f32 {
        self.size.width
    }

    pub fn height(&self) -> f32 {
        self.size.height
    }

    fn rect_at(x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::new(
            Offset::new(x, y),
            Size::new(width.max(0.0), height.max(0.0)),
        )
    }

    /// A filled rectangle.
    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.rounded_rect(x, y, width, height, 0.0, color);
    }

    /// A vector outline, such as an icon, stretched over a rectangle and filled.
    pub fn path(&mut self, path: &Path, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.commands.push(DrawCommand::Path {
            path: path.clone(),
            rect: Self::rect_at(x, y, width, height),
            color,
        });
    }

    /// A filled rectangle with rounded corners.
    pub fn rounded_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        color: Color,
    ) {
        self.commands.push(DrawCommand::Rect {
            rect: Self::rect_at(x, y, width, height),
            color,
            corner_radius: radius,
            border: None,
        });
    }

    /// An outline of `thickness` inside a rectangle.
    pub fn stroke_rect(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        thickness: f32,
        color: Color,
    ) {
        self.commands.push(DrawCommand::Rect {
            rect: Self::rect_at(x, y, width, height),
            color: Color::TRANSPARENT,
            corner_radius: radius,
            border: Some(Border {
                width: thickness,
                color,
            }),
        });
    }

    /// A rectangle blending from `from` to `to` along `degrees` (0 to the right, 90 downward).
    pub fn gradient(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        from: Color,
        to: Color,
        degrees: f32,
    ) {
        self.commands.push(DrawCommand::Gradient {
            rect: Self::rect_at(x, y, width, height),
            from,
            to,
            angle: degrees.to_radians(),
            corner_radius: radius,
        });
    }

    /// A soft blur of a rounded rectangle: a glow when `color` is bright, a drop
    /// shadow when it is dark. The colour fades out over `blur` pixels.
    pub fn shadow(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
        blur: f32,
        color: Color,
    ) {
        self.commands.push(DrawCommand::Shadow {
            rect: Self::rect_at(x, y, width, height),
            corner_radius: radius,
            blur,
            color,
        });
    }

    /// A straight line with round ends.
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: Color) {
        self.commands.push(DrawCommand::Line {
            from,
            to,
            width,
            color,
        });
    }

    /// Lines joining `points` in order.
    pub fn polyline(&mut self, points: &[(f32, f32)], width: f32, color: Color) {
        for pair in points.windows(2) {
            self.line(pair[0], pair[1], width, color);
        }
    }

    /// A filled circle.
    pub fn circle(&mut self, x: f32, y: f32, radius: f32, color: Color) {
        self.rounded_rect(
            x - radius,
            y - radius,
            radius * 2.0,
            radius * 2.0,
            radius,
            color,
        );
    }

    /// An outlined circle.
    pub fn stroke_circle(&mut self, x: f32, y: f32, radius: f32, thickness: f32, color: Color) {
        self.stroke_rect(
            x - radius,
            y - radius,
            radius * 2.0,
            radius * 2.0,
            radius,
            thickness,
            color,
        );
    }

    /// A ring segment around (`x`, `y`): `sweep` (0 to 1) of the circle clockwise
    /// from `start_degrees` (0 at the top), with round ends.
    pub fn arc(
        &mut self,
        x: f32,
        y: f32,
        radius: f32,
        thickness: f32,
        start_degrees: f32,
        sweep: f32,
        color: Color,
    ) {
        self.commands.push(DrawCommand::Arc {
            rect: Self::rect_at(x - radius, y - radius, radius * 2.0, radius * 2.0),
            color,
            thickness,
            start: start_degrees.to_radians(),
            sweep: sweep.clamp(0.0, 1.0),
        });
    }

    /// Fills the space between the line through `points` (left to right) and the
    /// level `baseline`, blending from `top` at the line to `bottom` at the baseline.
    pub fn fill_below(&mut self, points: &[(f32, f32)], baseline: f32, top: Color, bottom: Color) {
        let (Some(first), Some(last)) = (points.first(), points.last()) else {
            return;
        };
        // One-pixel columns side by side, each as tall as the line is there. They
        // sit on whole pixels and do not overlap, so they leave no seams or stripes.
        let mut x = first.0.ceil();
        while x < last.0 {
            let y = value_at(points, x);
            if baseline > y {
                self.gradient(x, y, 1.0, baseline - y, 0.0, top, bottom, 90.0);
            }
            x += 1.0;
        }
    }

    /// One line of text whose box is `width` wide and starts at (`x`, `y`).
    pub fn text(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        content: &str,
        size: f32,
        weight: u16,
        color: Color,
        align: TextAlign,
    ) {
        self.commands.push(DrawCommand::Text {
            x,
            y,
            width,
            content: content.to_string(),
            color,
            font_size: size,
            line_height: size * 1.35,
            font_weight: weight,
            font_family: FontFamily::SansSerif,
            align,
            max_lines: Some(1),
        });
    }

    /// Everything after this, until [`Canvas::pop_clip`], is cut to a rectangle.
    pub fn push_clip(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.commands.push(DrawCommand::PushClip {
            rect: Self::rect_at(x, y, width, height),
        });
    }

    pub fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }
}

/// The y of the line through `points` (sorted by x) at `x`.
fn value_at(points: &[(f32, f32)], x: f32) -> f32 {
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if x >= x0 && x <= x1 {
            return if x1 == x0 {
                y0
            } else {
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            };
        }
    }
    points.last().map_or(0.0, |last| last.1)
}
