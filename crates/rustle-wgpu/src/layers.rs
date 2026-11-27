//! Splitting draw commands into layers, groups and the clips they are cut to.

use glyphon::Color;
use rustle_geometry::Rect;
use rustle_graphics::DrawCommand;

use crate::composite::{Mask, Placement};
use crate::quad::Instance;
use crate::text::{
    Content, Ink, Setting, Shaped, legible_color, legible_weight, plain_shape_key, rich_shape_key,
    shape,
};
use crate::{Renderer, image, path, shaders};

impl Renderer {
    /// Splits `commands` into the layers a frame of the renderer's size draws, shaping the
    /// text they hold, without preparing or drawing anything, and returns how many items the
    /// frame holds: what a frame costs on the CPU before the GPU is involved, for benchmarks.
    pub fn build_layers(&mut self, commands: &[DrawCommand], scale_factor: f32) -> usize {
        let target = (self.surface_config.width, self.surface_config.height);
        self.layers(commands, scale_factor, target).len()
    }

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
        let mut clip = scissor(&clips, scale_factor, target);
        let mut current = Layer::new();
        for command in commands {
            // Nothing outside the clip or the target can show, so it is not
            // shaped, tessellated or uploaded. Long lists that are not lazy
            // would otherwise pay for every row on every frame.
            if let Some(bounds) = bounds(command) {
                let visible = clip.unwrap_or((0, 0, target.0, target.1));
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
                    current = Layer::new();
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
                    current = Layer::new();
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
                    current = Layer::new();
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
                    current = Layer::new();
                }
                DrawCommand::PopLayer | DrawCommand::PopEffect => {
                    push_layer(&mut frames, current.take());
                    if frames.len() > 1 {
                        close_group(&mut frames);
                    }
                    current = Layer::new();
                }
                DrawCommand::PushClip { rect } => {
                    clips.push(*rect);
                    rounded.push(false);
                    clip = scissor(&clips, scale_factor, target);
                }
                DrawCommand::PushRoundedClip { rect, radii } => {
                    clips.push(*rect);
                    let scissor = scissor(&clips, scale_factor, target);
                    clip = scissor;
                    if radii.iter().all(|radius| *radius <= 0.0) {
                        rounded.push(false);
                    } else {
                        push_layer(&mut frames, current.take());
                        current = Layer::new();
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
                }
                DrawCommand::PopClip => {
                    clips.pop();
                    clip = scissor(&clips, scale_factor, target);
                    if rounded.pop() == Some(true) {
                        push_layer(&mut frames, current.take());
                        current = Layer::new();
                        if frames.len() > 1 {
                            close_group(&mut frames);
                        }
                    }
                }
                DrawCommand::Rect { .. }
                | DrawCommand::Shadow { .. }
                | DrawCommand::Gradient { .. }
                | DrawCommand::RadialGradient { .. }
                | DrawCommand::SweepGradient { .. }
                | DrawCommand::Arc { .. }
                | DrawCommand::Line { .. } => {
                    let area = covered(command, clip, scale_factor, target);
                    if current.hides(area, Kind::Quad) {
                        push_layer(&mut frames, current.take());
                        current = Layer::new();
                    }
                    if let Some(instance) = self.quads.instance(command) {
                        let start = current.quads.len();
                        current.quads.push(instance);
                        extend(&mut current.quad_runs, clip, start, current.quads.len());
                    }
                }
                DrawCommand::Path { .. } => {
                    let area = covered(command, clip, scale_factor, target);
                    if current.hides(area, Kind::Path) {
                        push_layer(&mut frames, current.take());
                        current = Layer::new();
                    }
                    let start = current.paths.len();
                    self.paths.push(
                        &self.device,
                        &self.queue,
                        &mut current.paths,
                        command,
                        scale_factor,
                    );
                    let end = current.paths.len();
                    if end > start {
                        extend(&mut current.path_runs, clip, start, end);
                        current.cover(area, Kind::Path);
                    }
                }
                DrawCommand::Image { .. } | DrawCommand::Mask { .. } | DrawCommand::Svg { .. } => {
                    let area = covered(command, clip, scale_factor, target);
                    if current.hides(area, Kind::Image) {
                        push_layer(&mut frames, current.take());
                        current = Layer::new();
                    }
                    let start = current.images.len();
                    current.images.extend(self.images.instance(
                        &self.device,
                        &self.queue,
                        command,
                        scale_factor,
                    ));
                    let end = current.images.len();
                    if end > start {
                        extend(&mut current.image_runs, clip, start, end);
                        current.cover(area, Kind::Image);
                    }
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
                    let font_weight = legible_weight(*font_weight, *color);
                    let key = plain_shape_key(
                        content,
                        font_weight,
                        *font_family,
                        *font_size,
                        *line_height,
                        (*align, *max_lines, *width, *wrap),
                        (*italic, *letter_spacing, *ligatures),
                        self.font_generation.get(),
                    );
                    let (buffer, ink) = self.kept(key, scale_factor, |font_system| {
                        shape(
                            font_system,
                            Setting {
                                content: Content::Plain {
                                    text: content,
                                    font_weight,
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
                        )
                    });
                    let text = Shaped {
                        position: (*x, *y),
                        color: legible_color(*color),
                        buffer,
                        clip,
                    };
                    add_text(&mut frames, &mut current, text, ink, scale_factor, target);
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
                    let key = rich_shape_key(
                        spans,
                        *font_size,
                        *line_height,
                        (*align, *max_lines, *width, *wrap),
                        self.font_generation.get(),
                    );
                    let (buffer, ink) = self.kept(key, scale_factor, |font_system| {
                        shape(
                            font_system,
                            Setting {
                                content: Content::Rich(spans),
                                font_size: *font_size,
                                line_height: *line_height,
                                align: *align,
                                max_lines: *max_lines,
                                wrap: *wrap,
                            },
                            Some(*width + 1.0),
                        )
                    });
                    let text = Shaped {
                        position: (*x, *y),
                        color: Color::rgb(255, 255, 255),
                        buffer,
                        clip,
                    };
                    add_text(&mut frames, &mut current, text, ink, scale_factor, target);
                }
            }
        }
        self.frame += 1;
        self.paths.end_frame();
        if self.frame.is_multiple_of(SWEEP_FRAMES) {
            let frame = self.frame;
            self.shaped
                .retain(|_, kept| frame - kept.used < KEPT_FRAMES);
            self.measured
                .retain(|_, (_, used)| frame - *used < KEPT_FRAMES);
        }
        push_layer(&mut frames, current.take());
        while frames.len() > 1 {
            close_group(&mut frames);
        }
        frames.pop().map(|frame| frame.items).unwrap_or_default()
    }
}

/// How many frames pass between sweeps of the text caches.
const SWEEP_FRAMES: u64 = 60;

/// How many frames shaped and measured text is kept after it was last used.
const KEPT_FRAMES: u64 = 240;

/// A scissor rectangle: left, top, width and height in physical pixels of the target.
pub(super) type Scissor = (u32, u32, u32, u32);

/// A rectangle as left, top, right and bottom in physical pixels of the target.
type Area = (i32, i32, i32, i32);

/// The kinds of primitive a layer draws, in the order it draws them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Quad,
    Path,
    Image,
    Text,
}

/// Instances of one kind drawn with one scissor: those numbered `start..end`.
pub(super) struct Run {
    /// `None` is the whole target.
    pub(super) scissor: Option<Scissor>,
    pub(super) start: u32,
    pub(super) end: u32,
}

/// Records that the instances numbered `start..end` are cut to `scissor`, lengthening the
/// last of `runs` when it has the same scissor and ends where they start.
fn extend(runs: &mut Vec<Run>, scissor: Option<Scissor>, start: usize, end: usize) {
    let (start, end) = (start as u32, end as u32);
    if start == end {
        return;
    }
    match runs.last_mut() {
        Some(last) if last.scissor == scissor && last.end == start => last.end = end,
        _ => runs.push(Run {
            scissor,
            start,
            end,
        }),
    }
}

/// The areas a layer's primitives of one kind cover, to tell whether something drawn later
/// would land under them if it joined the layer.
#[derive(Default)]
struct Cover {
    /// Everything in `areas` together.
    union: Option<Area>,
    areas: Vec<Area>,
}

impl Cover {
    /// Adds `area` to what is covered.
    fn add(&mut self, area: Area) {
        self.union = Some(self.union.map_or(area, |union| {
            (
                union.0.min(area.0),
                union.1.min(area.1),
                union.2.max(area.2),
                union.3.max(area.3),
            )
        }));
        self.areas.push(area);
    }

    /// Whether any covered area shares a pixel with `area`.
    fn meets(&self, area: Area) -> bool {
        self.union.is_some_and(|union| meet(union, area))
            && self.areas.iter().any(|covered| meet(*covered, area))
    }
}

/// Whether two areas share a pixel.
fn meet(a: Area, b: Area) -> bool {
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

/// Where `command`, cut to `clip`, can draw in physical pixels of `target`: from
/// [`bounds`] grown by a pixel, so it errs on the large side. `None` when it draws nothing.
fn covered(
    command: &DrawCommand,
    clip: Option<Scissor>,
    scale_factor: f32,
    target: (u32, u32),
) -> Option<Area> {
    let (left, top, right, bottom) = bounds(command)?;
    let area = (
        (left * scale_factor).floor() as i32 - 1,
        (top * scale_factor).floor() as i32 - 1,
        (right * scale_factor).ceil() as i32 + 1,
        (bottom * scale_factor).ceil() as i32 + 1,
    );
    cut(area, clip, target)
}

/// `area` cut to `clip`, the whole target when `None`; `None` if nothing is left.
fn cut(area: Area, clip: Option<Scissor>, target: (u32, u32)) -> Option<Area> {
    let (x, y, width, height) = whole(clip, target);
    let (x, y) = (x as i32, y as i32);
    let cut = (
        area.0.max(x),
        area.1.max(y),
        area.2.min(x + width as i32),
        area.3.min(y + height as i32),
    );
    (cut.0 < cut.2 && cut.1 < cut.3).then_some(cut)
}

/// Adds `text`, whose glyphs land as `ink` says, to `current`, first ending the layer when
/// some of its text reaches outside its line boxes and does not share one clip with the rest.
fn add_text(
    frames: &mut [Frame],
    current: &mut Layer,
    text: Shaped,
    ink: Ink,
    scale_factor: f32,
    target: (u32, u32),
) {
    let shared = current.texts.iter().all(|other| other.clip == text.clip);
    if (ink.overhang || current.text_overhang) && !shared {
        push_layer(frames, std::mem::replace(current, Layer::new()).take());
    }
    current.text_overhang |= ink.overhang;
    let origin = (
        (text.position.0 * scale_factor).round() as i32,
        (text.position.1 * scale_factor).round() as i32,
    );
    let area = ink.rect.and_then(|(left, top, right, bottom)| {
        cut(
            (
                origin.0 + left,
                origin.1 + top,
                origin.0 + right,
                origin.1 + bottom,
            ),
            text.clip,
            target,
        )
    });
    current.cover(area, Kind::Text);
    current.texts.push(text);
}

/// Rectangles, outlines, pictures and text drawn together, each kind after the one before
/// and every primitive cut to its own scissor. A primitive only joins a layer when nothing
/// of a kind drawn after its own already lies where it draws.
pub(super) struct Layer {
    pub(super) quads: Vec<Instance>,
    pub(super) quad_runs: Vec<Run>,
    pub(super) paths: path::Geometry,
    pub(super) path_runs: Vec<Run>,
    pub(super) images: Vec<(u64, image::Instance)>,
    pub(super) image_runs: Vec<Run>,
    pub(super) texts: Vec<Shaped>,
    /// Whether a text reaches outside its line boxes, so that all the layer's text shares
    /// one clip and is cut by the scissor rather than by glyphon's bounds.
    text_overhang: bool,
    /// Where the outlines, pictures and text lie, by kind.
    covers: [Cover; 3],
}

impl Layer {
    /// An empty layer.
    fn new() -> Self {
        Self {
            quads: Vec::new(),
            quad_runs: Vec::new(),
            paths: path::Geometry::default(),
            path_runs: Vec::new(),
            images: Vec::new(),
            image_runs: Vec::new(),
            texts: Vec::new(),
            text_overhang: false,
            covers: Default::default(),
        }
    }

    /// Whether a primitive of `kind` drawn in `area` would land under something of a kind
    /// the layer draws after it, so it must go in a layer of its own.
    fn hides(&self, area: Option<Area>, kind: Kind) -> bool {
        let Some(area) = area else {
            return false;
        };
        [Kind::Path, Kind::Image, Kind::Text]
            .into_iter()
            .zip(&self.covers)
            .any(|(later, cover)| later > kind && cover.meets(area))
    }

    /// Records that a primitive of `kind` lies in `area`.
    fn cover(&mut self, area: Option<Area>, kind: Kind) {
        let index = match kind {
            Kind::Quad => return,
            Kind::Path => 0,
            Kind::Image => 1,
            Kind::Text => 2,
        };
        if let Some(area) = area {
            self.covers[index].add(area);
        }
    }

    /// The clip all the layer's text is cut to by the scissor, when it shares one because
    /// some of it reaches outside its line boxes; `None` when each text is cut to its own.
    pub(super) fn text_clip(&self) -> Option<Option<Scissor>> {
        if self.text_overhang {
            self.texts.first().map(|text| text.clip)
        } else {
            None
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
        | DrawCommand::Mask { rect, .. }
        | DrawCommand::Svg { rect, .. }
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
