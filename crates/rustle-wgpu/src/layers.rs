//! Splitting draw commands into layers, groups and the clips they are cut to.

use std::rc::Rc;

use glyphon::Color;
use rustle_geometry::Rect;
use rustle_graphics::DrawCommand;

use crate::composite::{Mask, Placement};
use crate::quad::Instance;
use crate::text::{Content, Setting, Shaped, legible_color, legible_weight, shape};
use crate::{Renderer, image, path, shaders};

impl Renderer {
    /// Splits `commands` into layers: each is the rectangles drawn together,
    /// the text drawn over them, and the clip they are cut to.
    pub(super) fn layers(
        &mut self,
        commands: &[DrawCommand],
        scale_factor: f32,
        target: (u32, u32),
    ) -> Vec<Item> {
        let mut frames = vec![Frame::default()];
        let mut clips: Vec<Rect> = Vec::new();
        // Whether each of `clips` is rounded, and so has a group of its own.
        let mut rounded: Vec<bool> = Vec::new();
        let mut current = Layer::new(scissor(&clips, scale_factor, target));
        for command in commands {
            // Nothing outside the clip or the target can show, so it is not
            // shaped, tessellated or uploaded. Long lists that are not lazy
            // would otherwise pay for every row on every frame.
            if let Some(bounds) = bounds(command) {
                let visible = current.scissor.unwrap_or((0, 0, target.0, target.1));
                if outside(bounds, visible, scale_factor) {
                    continue;
                }
            }
            match command {
                DrawCommand::PushLayer {
                    rect,
                    alpha,
                    rotation,
                    scale,
                } => {
                    push_layer(&mut frames, current.take());
                    let center = (
                        (rect.origin.x + rect.size.width / 2.0) * scale_factor,
                        (rect.origin.y + rect.size.height / 2.0) * scale_factor,
                    );
                    let scissor = scissor(&clips, scale_factor, target);
                    frames.push(Frame {
                        items: Vec::new(),
                        placement: Some(Placement {
                            pivot: center,
                            rotation: *rotation,
                            scale: *scale,
                            ..Placement::plain(*alpha, None)
                        }),
                        scissor,
                        region: whole(scissor, target),
                    });
                    current = Layer::new(scissor);
                }
                DrawCommand::PushEffect { rect, effect } => {
                    push_layer(&mut frames, current.take());
                    let mut filter = effect.color_filter.map(|filter| filter.matrix());
                    if let (Some(matrix), true) = (filter.as_mut(), self.format.is_srgb()) {
                        for row in 0..3 {
                            matrix[row * 5 + 4] =
                                crate::srgb_to_linear(matrix[row * 5 + 4].clamp(0.0, 1.0));
                        }
                    }
                    let blur = effect.blur.max(0.0) * scale_factor;
                    let placement = Placement {
                        filter,
                        blend: effect.blend_mode,
                        blur,
                        ..Placement::plain(effect.alpha, None)
                    };
                    let inner = scissor(&clips, scale_factor, target);
                    let outer = if blur > 0.0 {
                        scissor_within(&clips, rect, scale_factor, target)
                    } else {
                        inner
                    };
                    let region = if blur > 0.0 {
                        let reach = (blur * 3.0).ceil() as u32 + 2;
                        intersection(
                            whole(inner, target),
                            grow(whole(outer, target), reach, target),
                        )
                    } else {
                        whole(inner, target)
                    };
                    frames.push(Frame {
                        items: Vec::new(),
                        placement: Some(placement),
                        scissor: outer,
                        region,
                    });
                    current = Layer::new(inner);
                }
                DrawCommand::BackdropBlur {
                    rect,
                    corner_radius,
                    blur,
                } => {
                    push_layer(&mut frames, current.take());
                    let pixels = |value: f32| value * scale_factor;
                    let item = Item::Backdrop {
                        rect: [
                            pixels(rect.origin.x),
                            pixels(rect.origin.y),
                            pixels(rect.size.width),
                            pixels(rect.size.height),
                        ],
                        corner_radius: pixels(*corner_radius),
                        sigma: pixels(blur.max(0.0)),
                        scissor: scissor_within(&clips, rect, scale_factor, target),
                    };
                    if let Some(frame) = frames.last_mut() {
                        frame.items.push(item);
                    }
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::Shader {
                    rect,
                    corner_radius,
                    brush,
                } => {
                    push_layer(&mut frames, current.take());
                    let item = Item::Shader {
                        scissor: scissor(&clips, scale_factor, target),
                        call: shaders::Call {
                            rect: [
                                rect.origin.x,
                                rect.origin.y,
                                rect.size.width,
                                rect.size.height,
                            ],
                            corner_radius: *corner_radius,
                            brush: brush.clone(),
                        },
                    };
                    if let Some(frame) = frames.last_mut() {
                        frame.items.push(item);
                    }
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PopLayer | DrawCommand::PopEffect => {
                    push_layer(&mut frames, current.take());
                    if frames.len() > 1 {
                        close_group(&mut frames);
                    }
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PushClip { rect } => {
                    push_layer(&mut frames, current.take());
                    clips.push(*rect);
                    rounded.push(false);
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PushRoundedClip { rect, radii } => {
                    push_layer(&mut frames, current.take());
                    clips.push(*rect);
                    let scissor = scissor(&clips, scale_factor, target);
                    if radii.iter().all(|radius| *radius <= 0.0) {
                        rounded.push(false);
                    } else {
                        rounded.push(true);
                        let pixels = |value: f32| value * scale_factor;
                        frames.push(Frame {
                            items: Vec::new(),
                            placement: Some(Placement::plain(
                                1.0,
                                Some(Mask {
                                    rect: [
                                        pixels(rect.origin.x),
                                        pixels(rect.origin.y),
                                        pixels(rect.size.width),
                                        pixels(rect.size.height),
                                    ],
                                    radii: radii.map(pixels),
                                }),
                            )),
                            scissor,
                            region: whole(scissor, target),
                        });
                    }
                    current = Layer::new(scissor);
                }
                DrawCommand::PopClip => {
                    push_layer(&mut frames, current.take());
                    clips.pop();
                    if rounded.pop() == Some(true) && frames.len() > 1 {
                        close_group(&mut frames);
                    }
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::Rect { .. }
                | DrawCommand::Shadow { .. }
                | DrawCommand::Gradient { .. }
                | DrawCommand::RadialGradient { .. }
                | DrawCommand::SweepGradient { .. }
                | DrawCommand::Arc { .. }
                | DrawCommand::Line { .. } => {
                    // A rectangle after an outline, text or a picture must paint over it, so it starts a new layer.
                    if !current.texts.is_empty()
                        || !current.images.is_empty()
                        || !current.paths.is_empty()
                    {
                        let scissor = current.scissor;
                        push_layer(&mut frames, current.take());
                        current = Layer::new(scissor);
                    }
                    current.quads.extend(self.quads.instance(command));
                }
                DrawCommand::Path { .. } => {
                    // An outline after text or a picture must paint over it.
                    if !current.texts.is_empty() || !current.images.is_empty() {
                        let scissor = current.scissor;
                        push_layer(&mut frames, current.take());
                        current = Layer::new(scissor);
                    }
                    self.paths.push(
                        &self.device,
                        &self.queue,
                        &mut current.paths,
                        command,
                        scale_factor,
                    );
                }
                DrawCommand::Image { .. } => {
                    // A picture after text, an outline or a rectangle must paint over it.
                    if !current.texts.is_empty()
                        || !current.quads.is_empty()
                        || !current.paths.is_empty()
                    {
                        let scissor = current.scissor;
                        push_layer(&mut frames, current.take());
                        current = Layer::new(scissor);
                    }
                    current.images.extend(self.images.instance(
                        &self.device,
                        &self.queue,
                        command,
                        scale_factor,
                    ));
                }
                DrawCommand::Text {
                    x,
                    y,
                    width,
                    content,
                    color,
                    font_size,
                    line_height,
                    font_weight,
                    font_family,
                    align,
                    max_lines,
                    italic,
                    letter_spacing,
                    ligatures,
                    wrap,
                } => {
                    let font_weight = &legible_weight(*font_weight, *color);
                    let generation = self.font_generation.get();
                    let cache_key = format!(
                        "{content}\u{0}{font_weight}{font_family:?}{font_size}{line_height}{align:?}{max_lines:?}{width}{italic}{letter_spacing}{ligatures}{wrap:?}{generation}"
                    );
                    let buffer = match self.shaped.get_mut(&cache_key) {
                        Some((buffer, used)) => {
                            *used = self.frame;
                            Rc::clone(buffer)
                        }
                        None => {
                            let buffer = Rc::new(shape(
                                &mut self.font_system.borrow_mut(),
                                Setting {
                                    content: Content::Plain {
                                        text: content,
                                        font_weight: *font_weight,
                                        font_family: *font_family,
                                        italic: *italic,
                                        letter_spacing: *letter_spacing,
                                        ligatures: *ligatures,
                                    },
                                    font_size: *font_size,
                                    line_height: *line_height,
                                    align: *align,
                                    max_lines: *max_lines,
                                    wrap: *wrap,
                                },
                                Some(*width + 1.0),
                            ));
                            self.shaped
                                .insert(cache_key, (Rc::clone(&buffer), self.frame));
                            buffer
                        }
                    };
                    current.texts.push(Shaped {
                        position: (*x, *y),
                        color: legible_color(*color),
                        buffer,
                    });
                }
                DrawCommand::RichText {
                    x,
                    y,
                    width,
                    spans,
                    font_size,
                    line_height,
                    align,
                    max_lines,
                    wrap,
                } => {
                    // Shaping rich text is the costly part, so it is kept like plain text.
                    let generation = self.font_generation.get();
                    let cache_key = format!(
                        "{spans:?}\u{1}{font_size}{line_height}{align:?}{max_lines:?}{width}{wrap:?}{generation}"
                    );
                    let buffer = match self.shaped.get_mut(&cache_key) {
                        Some((buffer, used)) => {
                            *used = self.frame;
                            Rc::clone(buffer)
                        }
                        None => {
                            let buffer = Rc::new(shape(
                                &mut self.font_system.borrow_mut(),
                                Setting {
                                    content: Content::Rich(spans),
                                    font_size: *font_size,
                                    line_height: *line_height,
                                    align: *align,
                                    max_lines: *max_lines,
                                    wrap: *wrap,
                                },
                                Some(*width + 1.0),
                            ));
                            self.shaped
                                .insert(cache_key, (Rc::clone(&buffer), self.frame));
                            buffer
                        }
                    };
                    current.texts.push(Shaped {
                        position: (*x, *y),
                        color: Color::rgb(255, 255, 255),
                        buffer,
                    });
                }
            }
        }
        self.frame += 1;
        self.paths.end_frame();
        let frame = self.frame;
        self.shaped.retain(|_, (_, used)| frame - *used < 240);
        self.measured.retain(|_, (_, used)| frame - *used < 240);
        push_layer(&mut frames, current.take());
        while frames.len() > 1 {
            close_group(&mut frames);
        }
        frames.pop().map(|frame| frame.items).unwrap_or_default()
    }
}

/// A run of rectangles with the text over them, cut to a clip.
pub(super) struct Layer {
    /// In physical pixels as x, y, width and height; `None` is the whole target.
    pub(super) scissor: Option<(u32, u32, u32, u32)>,
    pub(super) quads: Vec<Instance>,
    pub(super) paths: path::Geometry,
    pub(super) images: Vec<(u64, image::Instance)>,
    pub(super) texts: Vec<Shaped>,
}

impl Layer {
    fn new(scissor: Option<(u32, u32, u32, u32)>) -> Self {
        Self {
            scissor,
            quads: Vec::new(),
            paths: path::Geometry::default(),
            images: Vec::new(),
            texts: Vec::new(),
        }
    }

    /// The layer, or nothing if it has nothing to draw.
    fn take(self) -> Option<Self> {
        (!self.quads.is_empty()
            || !self.paths.is_empty()
            || !self.images.is_empty()
            || !self.texts.is_empty())
        .then_some(self)
    }
}

/// Layers and groups of layers, in drawing order.
pub(super) enum Item {
    Layer(Layer),
    /// A rounded rectangle filled by a program's shader.
    Shader {
        scissor: Option<(u32, u32, u32, u32)>,
        call: shaders::Call,
    },
    /// What is drawn so far in the rectangle, replaced by a blur of it. All in physical pixels.
    Backdrop {
        rect: [f32; 4],
        corner_radius: f32,
        sigma: f32,
        scissor: Option<(u32, u32, u32, u32)>,
    },
    /// Layers drawn as one picture and put on what is below as `placement` says.
    Group {
        placement: Placement,
        scissor: Option<(u32, u32, u32, u32)>,
        /// Where the group's content can lie, in physical pixels: what its texture holds.
        region: (u32, u32, u32, u32),
        items: Vec<Item>,
    },
}

/// The items of a group being collected, and how it is placed once closed.
#[derive(Default)]
struct Frame {
    items: Vec<Item>,
    placement: Option<Placement>,
    scissor: Option<(u32, u32, u32, u32)>,
    region: (u32, u32, u32, u32),
}

fn push_layer(frames: &mut [Frame], layer: Option<Layer>) {
    if let (Some(layer), Some(frame)) = (layer, frames.last_mut()) {
        frame.items.push(Item::Layer(layer));
    }
}

/// Ends the innermost group, adding it to the one around it.
fn close_group(frames: &mut Vec<Frame>) {
    let Some(frame) = frames.pop() else {
        return;
    };
    if frame.items.is_empty() {
        return;
    }
    if let (Some(placement), Some(parent)) = (frame.placement, frames.last_mut()) {
        parent.items.push(Item::Group {
            placement,
            scissor: frame.scissor,
            region: frame.region,
            items: frame.items,
        });
    }
}

/// `scissor` as a rectangle, which is the whole target without one.
pub(super) fn whole(
    scissor: Option<(u32, u32, u32, u32)>,
    target: (u32, u32),
) -> (u32, u32, u32, u32) {
    scissor.unwrap_or((0, 0, target.0, target.1))
}

/// What two rectangles (left, top, width and height) have in common; empty if nothing.
pub(super) fn intersection(
    a: (u32, u32, u32, u32),
    b: (u32, u32, u32, u32),
) -> (u32, u32, u32, u32) {
    let left = a.0.max(b.0);
    let top = a.1.max(b.1);
    let right = (a.0 + a.2).min(b.0 + b.2);
    let bottom = (a.1 + a.3).min(b.1 + b.3);
    (
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

/// `rect` grown by `by` pixels on every side, kept inside the target.
pub(super) fn grow(
    rect: (u32, u32, u32, u32),
    by: u32,
    target: (u32, u32),
) -> (u32, u32, u32, u32) {
    let left = rect.0.saturating_sub(by);
    let top = rect.1.saturating_sub(by);
    let right = (rect.0 + rect.2 + by).min(target.0);
    let bottom = (rect.1 + rect.3 + by).min(target.1);
    (
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    )
}

/// Like [`scissor`], with `rect` as one more clip.
fn scissor_within(
    clips: &[Rect],
    rect: &Rect,
    scale_factor: f32,
    target: (u32, u32),
) -> Option<(u32, u32, u32, u32)> {
    let mut within = clips.to_vec();
    within.push(*rect);
    scissor(&within, scale_factor, target)
}

/// The pixels the innermost of `clips` leaves visible, or `None` without clips.
fn scissor(clips: &[Rect], scale_factor: f32, target: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let (first, rest) = clips.split_first()?;
    let edges = |rect: &Rect| {
        (
            rect.origin.x,
            rect.origin.y,
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        )
    };
    let (left, top, right, bottom) = rest.iter().fold(edges(first), |(l, t, r, b), clip| {
        let (cl, ct, cr, cb) = edges(clip);
        (l.max(cl), t.max(ct), r.min(cr), b.min(cb))
    });
    let to_pixels =
        |value: f32, limit: u32| ((value * scale_factor).round().max(0.0) as u32).min(limit);
    let (left, top) = (to_pixels(left, target.0), to_pixels(top, target.1));
    let (right, bottom) = (to_pixels(right, target.0), to_pixels(bottom, target.1));
    Some((
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    ))
}

/// Where a command can draw, as left, top, right and bottom in logical pixels,
/// erring on the large side; `None` for commands that draw nothing themselves.
fn bounds(command: &DrawCommand) -> Option<(f32, f32, f32, f32)> {
    let around = |rect: &Rect, grow: f32| {
        (
            rect.origin.x - grow,
            rect.origin.y - grow,
            rect.origin.x + rect.size.width + grow,
            rect.origin.y + rect.size.height + grow,
        )
    };
    // Glyphs can hang a little past their box, and a line can wrap onto more rows than it has breaks.
    let text = |x: f32,
                y: f32,
                width: f32,
                chars: usize,
                breaks: usize,
                size: f32,
                line: f32,
                max_lines: Option<usize>| {
        let wrapped = (chars as f32 * size * 1.25 / width.max(1.0)).ceil() as usize + breaks + 1;
        let lines = max_lines.map_or(wrapped, |max| max.min(wrapped));
        (
            x - size,
            y - size,
            x + width + size,
            y + lines as f32 * line.max(size) + size,
        )
    };
    Some(match command {
        DrawCommand::Rect { rect, border, .. } => {
            around(rect, border.map_or(0.0, |border| border.width) + 1.0)
        }
        DrawCommand::Gradient { rect, .. }
        | DrawCommand::RadialGradient { rect, .. }
        | DrawCommand::SweepGradient { rect, .. }
        | DrawCommand::Shader { rect, .. }
        | DrawCommand::Arc { rect, .. }
        | DrawCommand::Image { rect, .. }
        | DrawCommand::Path { rect, .. } => around(rect, 2.0),
        DrawCommand::Shadow { rect, blur, .. } => around(rect, blur * 2.0 + 2.0),
        DrawCommand::BackdropBlur { rect, .. } => around(rect, 1.0),
        DrawCommand::Line {
            from, to, width, ..
        } => (
            from.0.min(to.0) - width - 1.0,
            from.1.min(to.1) - width - 1.0,
            from.0.max(to.0) + width + 1.0,
            from.1.max(to.1) + width + 1.0,
        ),
        DrawCommand::Text {
            x,
            y,
            width,
            content,
            font_size,
            line_height,
            max_lines,
            ..
        } => text(
            *x,
            *y,
            *width,
            content.chars().count(),
            content.matches('\n').count(),
            *font_size,
            *line_height,
            *max_lines,
        ),
        DrawCommand::RichText {
            x,
            y,
            width,
            spans,
            font_size,
            line_height,
            max_lines,
            ..
        } => {
            let chars = spans.iter().map(|span| span.text.chars().count()).sum();
            let breaks = spans
                .iter()
                .map(|span| span.text.matches('\n').count())
                .sum();
            text(
                *x,
                *y,
                *width,
                chars,
                breaks,
                *font_size,
                *line_height,
                *max_lines,
            )
        }
        DrawCommand::PushLayer { .. }
        | DrawCommand::PopLayer
        | DrawCommand::PushEffect { .. }
        | DrawCommand::PopEffect
        | DrawCommand::PushClip { .. }
        | DrawCommand::PushRoundedClip { .. }
        | DrawCommand::PopClip => return None,
    })
}

/// Whether `bounds` (logical pixels) lie entirely outside `visible` (physical pixels).
fn outside(bounds: (f32, f32, f32, f32), visible: (u32, u32, u32, u32), scale_factor: f32) -> bool {
    let (x, y, width, height) = visible;
    let (left, top, right, bottom) = bounds;
    right * scale_factor <= x as f32
        || bottom * scale_factor <= y as f32
        || left * scale_factor >= (x + width) as f32
        || top * scale_factor >= (y + height) as f32
}
