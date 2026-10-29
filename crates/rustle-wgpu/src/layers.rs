//! Splitting draw commands into layers, groups and the clips they are cut to.

use std::rc::Rc;

use glyphon::Color;
use rustle_geometry::Rect;
use rustle_graphics::DrawCommand;

use crate::composite::Placement;
use crate::quad::Instance;
use crate::text::{Content, Setting, Shaped, legible_color, legible_weight, shape};
use crate::{Renderer, image, path};

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
        let mut current = Layer::new(scissor(&clips, scale_factor, target));
        for command in commands {
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
                            opacity: *alpha,
                        }),
                        scissor,
                    });
                    current = Layer::new(scissor);
                }
                DrawCommand::PopLayer => {
                    push_layer(&mut frames, current.take());
                    if frames.len() > 1 {
                        close_group(&mut frames);
                    }
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PushClip { rect } => {
                    push_layer(&mut frames, current.take());
                    clips.push(*rect);
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PopClip => {
                    push_layer(&mut frames, current.take());
                    clips.pop();
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::Rect { .. }
                | DrawCommand::Shadow { .. }
                | DrawCommand::Gradient { .. }
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
                    self.paths.push(&mut current.paths, command, scale_factor);
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
                } => {
                    let font_weight = &legible_weight(*font_weight, *color);
                    let cache_key = format!(
                        "{content}\u{0}{font_weight}{font_family:?}{font_size}{line_height}{align:?}{max_lines:?}{width}"
                    );
                    let buffer = match self.shaped.get_mut(&cache_key) {
                        Some((buffer, used)) => {
                            *used = self.frame;
                            Rc::clone(buffer)
                        }
                        None => {
                            let buffer = Rc::new(shape(
                                &mut self.font_system,
                                Setting {
                                    content: Content::Plain {
                                        text: content,
                                        font_weight: *font_weight,
                                        font_family: *font_family,
                                    },
                                    font_size: *font_size,
                                    line_height: *line_height,
                                    align: *align,
                                    max_lines: *max_lines,
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
                } => {
                    let buffer = Rc::new(shape(
                        &mut self.font_system,
                        Setting {
                            content: Content::Rich(spans),
                            font_size: *font_size,
                            line_height: *line_height,
                            align: *align,
                            max_lines: *max_lines,
                        },
                        Some(*width + 1.0),
                    ));
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
    /// Layers drawn as one picture and put on what is below as `placement` says.
    Group {
        placement: Placement,
        scissor: Option<(u32, u32, u32, u32)>,
        items: Vec<Item>,
    },
}

/// The items of a group being collected, and how it is placed once closed.
#[derive(Default)]
struct Frame {
    items: Vec<Item>,
    placement: Option<Placement>,
    scissor: Option<(u32, u32, u32, u32)>,
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
    if let (Some(placement), Some(parent)) = (frame.placement, frames.last_mut()) {
        parent.items.push(Item::Group {
            placement,
            scissor: frame.scissor,
            items: frame.items,
        });
    }
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
