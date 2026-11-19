use std::rc::Rc;

use rustle_geometry::Size;
use rustle_graphics::{BlendMode, Color, ColorFilter, LayerEffect, ShaderBrush};
use rustle_unit::Dp;

use super::{Element, Fill, Modifier};
use crate::Canvas;

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

    /// Turns the content clockwise about the center of the area by the angle, in degrees,
    /// `degrees` returns when the area is drawn, as Compose's `graphicsLayer { rotationZ = ... }`
    /// block does: state it reads is read while drawing, so an angle that keeps changing, such
    /// as a spinner's, draws the area again without composing or measuring anything. See
    /// [`Modifier::graphics_layer`].
    pub fn rotate_with(self, degrees: impl Fn() -> f32 + 'static) -> Self {
        self.then(Element::RotateWith(Rc::new(degrees)))
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

    /// Draws on a [`Canvas`] the size of the area, behind its content and what is
    /// before it in the chain, as `background` does. `draw` runs each time the
    /// interface is painted.
    pub fn draw_behind(self, draw: impl Fn(&mut Canvas) + 'static) -> Self {
        self.then(Element::Draw(Rc::new(move |size: Size| {
            let mut canvas = Canvas::new(size);
            draw(&mut canvas);
            let (commands, _) = canvas.finish_with_content();
            let end = commands.len();
            (commands, Some(end))
        })))
    }

    /// Like [`draw_behind`](Self::draw_behind), with `draw` also given the area's
    /// [`Interaction`](crate::Interaction) state: whether the pointer is over it, presses it,
    /// and whether it has focus.
    ///
    /// The state is resolved when the interface is painted, not when it is composed, so
    /// hovering, pressing or focusing the area paints the laid-out tree again without
    /// composing or measuring anything: a hover style on every row of a long list costs a
    /// repaint, not a recomposition of whatever reads the list's state.
    pub fn draw_behind_interactive(
        self,
        draw: impl Fn(&mut Canvas, crate::Interaction) + 'static,
    ) -> Self {
        self.then(Element::InteractiveDraw(Rc::new(
            move |size, interaction| {
                let mut canvas = Canvas::new(size);
                draw(&mut canvas, interaction);
                let (commands, _) = canvas.finish_with_content();
                let end = commands.len();
                (commands, Some(end))
            },
        )))
    }

    /// Like [`draw_behind_interactive`](Self::draw_behind_interactive), drawing over the
    /// content instead of behind it, such as a focus ring.
    pub fn draw_over_interactive(
        self,
        draw: impl Fn(&mut Canvas, crate::Interaction) + 'static,
    ) -> Self {
        self.then(Element::InteractiveDraw(Rc::new(
            move |size, interaction| {
                let mut canvas = Canvas::new(size);
                draw(&mut canvas, interaction);
                let (commands, _) = canvas.finish_with_content();
                (commands, Some(0))
            },
        )))
    }

    /// Like [`draw_with_content`](Self::draw_with_content), with `draw` also given the
    /// area's [`Interaction`](crate::Interaction) state, resolved when painting as for
    /// [`draw_behind_interactive`](Self::draw_behind_interactive).
    pub fn draw_with_content_interactive(
        self,
        draw: impl Fn(&mut Canvas, crate::Interaction) + 'static,
    ) -> Self {
        self.then(Element::InteractiveDraw(Rc::new(
            move |size, interaction| {
                let mut canvas = Canvas::new(size);
                draw(&mut canvas, interaction);
                canvas.finish_with_content()
            },
        )))
    }

    /// Fills the area behind its content with the color of `colors` for its
    /// [`Interaction`](crate::Interaction) state: pressed, hovered, focused or idle. The state
    /// is resolved when painting, as for
    /// [`draw_behind_interactive`](Self::draw_behind_interactive), so a change repaints
    /// without composing.
    pub fn interaction_background(self, colors: crate::InteractionColors) -> Self {
        self.draw_behind_interactive(move |canvas, interaction| {
            let color = colors.color(interaction);
            if color.a == 0 {
                return;
            }
            let (width, height) = (canvas.width(), canvas.height());
            canvas.rounded_rect(0.0, 0.0, width, height, colors.corner_radius, color);
        })
    }

    /// Gives the interactive drawing inside this modifier (see
    /// [`draw_behind_interactive`](Self::draw_behind_interactive)) its focused state from
    /// outside, for an application that keeps its own notion of focus. It wins over the
    /// focus of any [`focusable`](Self::focusable). Changing it composes again, as any value
    /// given in composition does.
    pub fn interaction_focused(self, focused: bool) -> Self {
        self.then(Element::InteractionFocused(Rc::new(move || focused)))
    }

    /// Makes the area a group for the interactive drawing inside it (see
    /// [`draw_behind_interactive`](Self::draw_behind_interactive)): while the pointer is inside
    /// the area's visible rectangle, everything inside it is painted with
    /// [`Interaction::group_hovered`](crate::Interaction::group_hovered) set, such as a button
    /// in a row that only shows while the row is hovered. Resolved when painting, so hovering
    /// the group repaints without composing.
    pub fn interaction_group(self) -> Self {
        self.then(Element::InteractionGroup)
    }

    /// Like [`interaction_focused`](Self::interaction_focused), with the focused state read by
    /// calling `focused` each time the interface is painted or checks whether its interaction
    /// state changed, not when it is composed. An application that keeps its own focus in a
    /// `Cell` changes it and calls `Embedded::update`, which paints again without composing
    /// and returns true when the drawing depends on it.
    pub fn interaction_focused_with(self, focused: impl Fn() -> bool + 'static) -> Self {
        self.then(Element::InteractionFocused(Rc::new(focused)))
    }

    /// Draws on a [`Canvas`] the size of the area, with the content drawn where `draw`
    /// calls [`Canvas::draw_content`]: what it draws before is behind the content and
    /// what it draws after is over it. If it never calls it, the content is not drawn
    /// (and takes no input), like `drawWithContent` in Compose.
    pub fn draw_with_content(self, draw: impl Fn(&mut Canvas) + 'static) -> Self {
        self.then(Element::Draw(Rc::new(move |size: Size| {
            let mut canvas = Canvas::new(size);
            draw(&mut canvas);
            canvas.finish_with_content()
        })))
    }

    /// Fills the area, with corners rounded to `corner_radius`, with what the
    /// fragment shader of `brush` computes for each point. See [`ShaderBrush`] for what
    /// the WGSL source may use. A shader that does not compile is reported and not drawn.
    pub fn shader(self, brush: ShaderBrush, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Fill(Fill::Shader {
            brush,
            corner_radius: corner_radius.into().0,
        }))
    }

    /// Fills the area with a blend from `from` at `center` (fractions of the area:
    /// (0.5, 0.5) is its middle) to `to` at `radius` from it, with corners rounded to `corner_radius`.
    pub fn background_radial_gradient(
        self,
        from: Color,
        to: Color,
        center: (f32, f32),
        radius: impl Into<Dp>,
        corner_radius: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Fill(Fill::Radial {
            from,
            to,
            center,
            radius: radius.into().0,
            corner_radius: corner_radius.into().0,
        }))
    }

    /// Fills the area with a blend from `from` to `to` once around `center` (fractions
    /// of the area), clockwise starting `start_degrees` from pointing right, with corners
    /// rounded to `corner_radius`. The two ends meet at the start, so use the same color
    /// for both for a seamless wheel.
    pub fn background_sweep_gradient(
        self,
        from: Color,
        to: Color,
        center: (f32, f32),
        start_degrees: f32,
        corner_radius: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Fill(Fill::Sweep {
            from,
            to,
            center,
            start: start_degrees.to_radians(),
            corner_radius: corner_radius.into().0,
        }))
    }

    /// Casts a soft shadow of the area, moved by (`dx`, `dy`) and blurred with a
    /// Gaussian of `blur / 2` standard deviation, behind the content.
    pub fn drop_shadow(
        self,
        dx: impl Into<Dp>,
        dy: impl Into<Dp>,
        blur: impl Into<Dp>,
        color: Color,
        corner_radius: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Fill(Fill::DropShadow {
            dx: dx.into().0,
            dy: dy.into().0,
            blur: blur.into().0,
            color,
            corner_radius: corner_radius.into().0,
        }))
    }

    /// Casts `shadow` from the area's rectangle with corners rounded to `corner_radius`,
    /// behind the content: Compose's `Modifier.dropShadow(shape, shadow)` for a rounded
    /// rectangle. The shadow is grown by its spread, moved by its offset, blurred over its
    /// radius and blended as it says; chain one per layer for a shadow of several layers,
    /// the first drawn lowest.
    pub fn drop_shadow_with(
        self,
        corner_radius: impl Into<Dp>,
        shadow: rustle_graphics::Shadow,
    ) -> Self {
        self.then(Element::Fill(Fill::ShadowOf {
            shadow,
            corner_radius: corner_radius.into().0,
        }))
    }

    /// Blurs what is already drawn behind the area, in its rounded rectangle, with a
    /// Gaussian of `blur` standard deviation: frosted glass. Follow it with a
    /// translucent `background` for a tint. See [`rustle_graphics::DrawCommand::BackdropBlur`].
    pub fn background_blur(self, blur: impl Into<Dp>, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Fill(Fill::BackdropBlur {
            blur: blur.into().0,
            corner_radius: corner_radius.into().0,
        }))
    }

    /// Draws the content as one picture and blurs it with a Gaussian of `radius`
    /// standard deviation, cut to the area. Costs a texture the size of the window
    /// and two extra passes.
    pub fn blur(self, radius: impl Into<Dp>) -> Self {
        self.layer_effect(LayerEffect {
            blur: radius.into().0,
            ..LayerEffect::default()
        })
    }

    /// Draws the content as one picture and changes its colors with `filter`.
    pub fn color_filter(self, filter: ColorFilter) -> Self {
        self.layer_effect(LayerEffect {
            color_filter: Some(filter),
            ..LayerEffect::default()
        })
    }

    /// Draws the content as one picture and combines it with what is below as `mode` says.
    pub fn blend_mode(self, mode: BlendMode) -> Self {
        self.layer_effect(LayerEffect {
            blend_mode: mode,
            ..LayerEffect::default()
        })
    }

    /// Draws the content as one picture and puts it on what is below as `effect`
    /// says: blurred, recolored, blended and faded, all at once.
    pub fn layer_effect(self, effect: LayerEffect) -> Self {
        self.then(Element::Effect(LayerEffect {
            alpha: effect.alpha.clamp(0.0, 1.0),
            ..effect
        }))
    }
}
