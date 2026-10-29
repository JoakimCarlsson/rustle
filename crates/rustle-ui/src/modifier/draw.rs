use rustle_graphics::Color;
use rustle_unit::Dp;

use super::{Element, Modifier};

impl Modifier {
    /// Fills the area with `color`.
    pub fn background(self, color: Color) -> Self {
        self.background_rounded(color, 0.0)
    }

    /// Fills the area with `color`, with corners rounded to `corner_radius`.
    pub fn background_rounded(self, color: Color, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Background {
            color,
            corner_radius: corner_radius.into().0,
        })
    }

    /// Fills the area with a blend from `from` to `to` along `degrees`, clockwise
    /// from pointing right (90 blends downward), with corners rounded to `corner_radius`.
    pub fn background_gradient(
        self,
        from: Color,
        to: Color,
        degrees: f32,
        corner_radius: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Gradient {
            from,
            to,
            angle: degrees.to_radians(),
            corner_radius: corner_radius.into().0,
        })
    }

    /// Draws the content translucent, from 0 (invisible) to 1 (as is). Each
    /// thing is faded on its own, so overlapping things show through each other.
    pub fn alpha(self, alpha: f32) -> Self {
        self.then(Element::Alpha(alpha.clamp(0.0, 1.0)))
    }

    /// Draws the content as one picture, then puts it on what is below turned
    /// `rotation_degrees` clockwise and scaled by `scale` about the center of the
    /// area, and faded to `alpha`. Unlike [`Modifier::alpha`], overlapping parts
    /// of the content do not show through each other, and text and pictures
    /// turn and scale too. Pointer input still goes where the area was laid out.
    pub fn graphics_layer(self, alpha: f32, rotation_degrees: f32, scale: f32) -> Self {
        self.then(Element::GraphicsLayer {
            alpha: alpha.clamp(0.0, 1.0),
            rotation: rotation_degrees.to_radians(),
            scale,
        })
    }

    /// Turns the content `degrees` clockwise about the center of the area. See [`Modifier::graphics_layer`].
    pub fn rotate(self, degrees: f32) -> Self {
        self.graphics_layer(1.0, degrees, 1.0)
    }

    /// Scales the content by `factor` about the center of the area. See [`Modifier::graphics_layer`].
    pub fn scale(self, factor: f32) -> Self {
        self.graphics_layer(1.0, 0.0, factor)
    }

    /// Draws an outline of `width` inside the area, drawn over the content.
    pub fn border(self, width: impl Into<Dp>, color: Color, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Border {
            width: width.into().0,
            color,
            corner_radius: corner_radius.into().0,
        })
    }

    /// Draws a one-pixel line along the right edge of the area, over the content.
    pub fn border_right_hairline(self, color: Color) -> Self {
        self.then(Element::BorderSide { width: 1.0, color })
    }

    /// Draws a one-pixel line along the bottom edge of the area, over the content.
    pub fn border_bottom_hairline(self, color: Color) -> Self {
        self.border_bottom(1.0, color)
    }

    /// Draws a line of `width` along the bottom edge of the area, over the content.
    pub fn border_bottom(self, width: impl Into<Dp>, color: Color) -> Self {
        self.then(Element::BorderBottom {
            width: width.into().0,
            color,
        })
    }

    /// Casts a soft shadow behind the area; a larger `elevation` is softer and lower.
    pub fn shadow(self, elevation: impl Into<Dp>, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Shadow {
            elevation: elevation.into().0,
            corner_radius: corner_radius.into().0,
        })
    }

    /// Draws a ring segment in the largest circle that fits the area: `sweep`
    /// of the circle (0 to 1) clockwise from `start_degrees` (0 at the top), with round ends.
    pub fn ring(
        self,
        color: Color,
        thickness: impl Into<Dp>,
        start_degrees: f32,
        sweep: f32,
    ) -> Self {
        self.then(Element::Ring {
            color,
            thickness: thickness.into().0,
            start: start_degrees.to_radians(),
            sweep: sweep.clamp(0.0, 1.0),
        })
    }
}
