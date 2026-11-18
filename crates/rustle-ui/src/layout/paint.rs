use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, Color, DrawCommand, LayerEffect};
use rustle_text::{TextMeasurer, TextSpan};
use rustle_unit::Constraints;

use super::ContentScale;
use super::measure::measure_node;
use super::measured::{Measured, Paint};
use super::regions::{
    ButtonRegion, CursorRegion, DisplayList, DragRegion, DragSourceRegion, DropTargetRegion,
    FileDropRegion, FocusRegion, HitRegion, HoverRegion, KeyRegion, LongPressRegion, PinchRegion,
    PointerInputRegion, PointerMoveRegion, PressRegion, ScrollRegion, SemanticsNode, intersect,
};
use crate::Node;
use crate::modifier::Fill;
use crate::scroll::{ScrollState, ScrollbarVisibility, thumb};
use crate::semantics::{Role, Semantics};

/// How the edges of laid-out areas are placed on the pixel grid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PixelRounding {
    /// Areas are drawn where layout put them, at fractional positions if need be.
    #[default]
    None,
    /// Every area's edges are rounded to whole logical pixels: its origin is rounded, and its
    /// size becomes the distance between its rounded edges, so neighbours still meet. The
    /// rounding is of each area's unrounded place in the window, so it never adds up down
    /// the tree. Text keeps its measured width, so it wraps as it was measured.
    Logical,
    /// As `Logical`, to whole physical pixels at [`DisplayOptions::scale`].
    Physical,
}

/// How [`display_list_with`] paints what it laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayOptions {
    /// Whether and to what edges are rounded.
    pub rounding: PixelRounding,
    /// Physical pixels per logical pixel, for [`PixelRounding::Physical`].
    pub scale: f32,
}

impl Default for DisplayOptions {
    /// No rounding, at a scale of one.
    fn default() -> Self {
        Self {
            rounding: PixelRounding::None,
            scale: 1.0,
        }
    }
}

impl DisplayOptions {
    /// How many grid steps make a logical pixel, or `None` when nothing is rounded.
    fn grid(self) -> Option<f32> {
        match self.rounding {
            PixelRounding::None => None,
            PixelRounding::Logical => Some(1.0),
            PixelRounding::Physical if self.scale.is_finite() && self.scale > 0.0 => {
                Some(self.scale)
            }
            PixelRounding::Physical => Some(1.0),
        }
    }
}

/// Lays out `root` within `constraints`.
pub fn display_list(
    root: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> DisplayList {
    display_list_with(root, constraints, text, DisplayOptions::default())
}

/// Lays out `root` within `constraints` like [`display_list`], painting as `options` says,
/// such as with edges rounded to whole pixels.
pub fn display_list_with(
    root: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
    options: DisplayOptions,
) -> DisplayList {
    let measured = measure_node(root, constraints, text);
    let window = Size::new(
        if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            measured.size.width
        },
        if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            measured.size.height
        },
    );
    let mut painter = Painter {
        pending_end: None,
        deferred: Vec::new(),
        alpha: 1.0,
        merging: 0,
        clearing: 0,
        parents: Vec::new(),
        merged_text: Vec::new(),
        merged_labels: Vec::new(),
        list: DisplayList::default(),
        clips: Vec::new(),
        clip_radii: Vec::new(),
        window,
        grid: options.grid(),
    };
    painter.paint(&measured, Offset::default());
    painter.paint_deferred();
    painter.list
}

/// Something to paint after the rest of the tree, because it has a z-index.
struct Deferred<'a> {
    z_index: i32,
    origin: Offset,
    alpha: f32,
    /// The clips around it where it was declared, outermost first, which cut it
    /// as well unless it is unclipped.
    clips: Vec<(Rect, Option<[f32; 4]>)>,
    /// The semantics it was declared inside, innermost last, so what it describes is still
    /// their child.
    parents: Vec<usize>,
    /// How many enclosing semantics drop every description inside them.
    clearing: usize,
    measured: &'a Measured,
}

/// Paints the transient measured tree into an independently owned display list.
struct Painter<'a> {
    /// The drag-end handler of the area being painted, for the drag handler inside it.
    pending_end: Option<Rc<dyn Fn()>>,
    /// Raised areas, painted over the rest in order of z-index.
    deferred: Vec<Deferred<'a>>,
    /// How opaque everything painted now is, from the alpha modifiers around it.
    alpha: f32,
    /// How many enclosing semantics merge the text inside into themselves.
    merging: usize,
    /// How many enclosing semantics drop every description inside them.
    clearing: usize,
    /// The semantics being painted inside, as indices into the list, innermost last.
    parents: Vec<usize>,
    /// The text found inside each merging semantics, innermost last.
    merged_text: Vec<Vec<String>>,
    /// The described areas whose label was made by merging the text inside them.
    merged_labels: Vec<usize>,
    list: DisplayList,
    /// The clips in effect, innermost last, cutting hit and scroll regions.
    clips: Vec<Rect>,
    /// The corner radii of each of `clips`, if it is rounded.
    clip_radii: Vec<Option<[f32; 4]>>,
    /// How large the window is.
    window: Size,
    /// How many grid steps make a logical pixel when edges are rounded to them.
    grid: Option<f32>,
}

impl<'a> Painter<'a> {
    /// Where an area whose unrounded place is `exact` and size is `size` is drawn: as it is,
    /// or with its edges rounded to the grid, the size being the distance between them.
    fn snap(&self, exact: Offset, size: Size) -> (Offset, Size) {
        let Some(grid) = self.grid else {
            return (exact, size);
        };
        let round = |value: f32| (value * grid).round() / grid;
        let origin = Offset::new(round(exact.x), round(exact.y));
        let far = Offset::new(round(exact.x + size.width), round(exact.y + size.height));
        (origin, Size::new(far.x - origin.x, far.y - origin.y))
    }

    /// Paints what was raised with a z-index, lowest first, over what is already painted.
    fn paint_deferred(&mut self) {
        while !self.deferred.is_empty() {
            let mut raised = std::mem::take(&mut self.deferred);
            raised.sort_by_key(|raised| raised.z_index);
            for raised in raised {
                self.alpha = raised.alpha;
                self.clips.clear();
                self.clip_radii.clear();
                for (rect, radii) in &raised.clips {
                    match radii {
                        Some(radii) => self.push_rounded_clip(*rect, *radii),
                        None => self.push_clip(*rect),
                    }
                }
                self.parents = raised.parents;
                self.clearing = raised.clearing;
                let merging = self.merging_node();
                if merging.is_some() {
                    self.merging += 1;
                    self.merged_text.push(Vec::new());
                }
                self.paint(raised.measured, raised.origin);
                if let Some(index) = merging {
                    self.merging -= 1;
                    let text = self.merged_text.pop().unwrap_or_default();
                    self.add_merged_text(index, text);
                }
                for _ in &raised.clips {
                    self.pop_clip();
                }
            }
        }
    }

    /// The innermost enclosing described area that merges the text inside it, if any, among
    /// the semantics being painted inside.
    fn merging_node(&self) -> Option<usize> {
        (self.clearing == 0)
            .then(|| {
                self.parents
                    .iter()
                    .rev()
                    .copied()
                    .find(|index| self.list.semantics[*index].semantics.merge_descendants)
            })
            .flatten()
    }

    /// Gives the area at `index`, which merges its content, the `text` found in content that
    /// was painted after it was finished: it becomes the label, or is added to a label made
    /// by merging.
    fn add_merged_text(&mut self, index: usize, text: Vec<String>) {
        if text.is_empty() {
            return;
        }
        let merged_labels = &self.merged_labels;
        let node = &mut self.list.semantics[index].semantics;
        match &mut node.label {
            None => node.label = Some(text.join(" ")),
            Some(label) if merged_labels.contains(&index) => {
                label.push(' ');
                label.push_str(&text.join(" "));
            }
            Some(_) => {}
        }
    }

    /// `rect` cut to what the clips leave visible.
    fn visible(&self, rect: Rect) -> Rect {
        self.clips
            .iter()
            .fold(rect, |rect, clip| intersect(rect, *clip))
    }

    /// `color` as faded by the alpha modifiers in effect.
    fn faded(&self, color: Color) -> Color {
        Color {
            a: (f32::from(color.a) * self.alpha).round() as u8,
            ..color
        }
    }

    /// Draws `fill` over `rect`, faded by the alpha modifiers in effect.
    fn paint_fill(&mut self, fill: &Fill, rect: Rect) {
        let command = match fill {
            Fill::Radial {
                from,
                to,
                center,
                radius,
                corner_radius,
            } => DrawCommand::RadialGradient {
                rect,
                center: *center,
                radius: *radius,
                from: self.faded(*from),
                to: self.faded(*to),
                corner_radius: *corner_radius,
            },
            Fill::Sweep {
                from,
                to,
                center,
                start,
                corner_radius,
            } => DrawCommand::SweepGradient {
                rect,
                center: *center,
                start: *start,
                from: self.faded(*from),
                to: self.faded(*to),
                corner_radius: *corner_radius,
            },
            Fill::Shader {
                brush,
                corner_radius,
            } => {
                let shader = DrawCommand::Shader {
                    rect,
                    corner_radius: *corner_radius,
                    brush: brush.clone(),
                };
                if self.alpha < 1.0 {
                    let effect = LayerEffect {
                        alpha: self.alpha,
                        ..LayerEffect::default()
                    };
                    self.list
                        .commands
                        .push(DrawCommand::PushEffect { rect, effect });
                    self.list.commands.push(shader);
                    DrawCommand::PopEffect
                } else {
                    shader
                }
            }
            Fill::BackdropBlur {
                blur,
                corner_radius,
            } => DrawCommand::BackdropBlur {
                rect,
                corner_radius: *corner_radius,
                blur: *blur,
            },
            Fill::DropShadow {
                dx,
                dy,
                blur,
                color,
                corner_radius,
            } => DrawCommand::Shadow {
                rect: Rect::new(
                    Offset::new(rect.origin.x + dx, rect.origin.y + dy),
                    rect.size,
                ),
                corner_radius: *corner_radius,
                blur: *blur,
                color: self.faded(*color),
            },
        };
        self.list.commands.push(command);
    }

    fn push_clip(&mut self, rect: Rect) {
        self.clips.push(rect);
        self.clip_radii.push(None);
        self.list.commands.push(DrawCommand::PushClip { rect });
    }

    fn push_rounded_clip(&mut self, rect: Rect, radii: [f32; 4]) {
        self.clips.push(rect);
        self.clip_radii.push(Some(radii));
        self.list
            .commands
            .push(DrawCommand::PushRoundedClip { rect, radii });
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
        self.clip_radii.pop();
        self.list.commands.push(DrawCommand::PopClip);
    }

    /// Draws the overlay scrollbar of a scrolling area over its content, and
    /// registers the strip along its edge that reveals and drags it.
    fn paint_scrollbar(&mut self, rect: Rect, state: &ScrollState, vertical: bool, offset: f32) {
        const MARGIN: f32 = 2.0;
        let style = state.scrollbar();
        let viewport = if vertical {
            rect.size.height
        } else {
            rect.size.width
        };
        let track = viewport - 2.0 * MARGIN;
        if style.visibility == ScrollbarVisibility::Hidden
            || state.content_length() <= viewport + 0.5
            || track < style.min_length
        {
            return;
        }
        let hit = style.hit_thickness.max(style.thickness + 2.0 * MARGIN);
        // The strip along the end of the scrolling axis, and its place on that axis.
        let strip = if vertical {
            Rect::new(
                Offset::new(rect.origin.x + rect.size.width - hit, rect.origin.y),
                Size::new(hit, rect.size.height),
            )
        } else {
            Rect::new(
                Offset::new(rect.origin.x, rect.origin.y + rect.size.height - hit),
                Size::new(rect.size.width, hit),
            )
        };
        let alpha = state.scrollbar_alpha();
        if alpha > 0.01 {
            let (start, length) = thumb(
                offset,
                state.content_length(),
                viewport,
                track,
                style.min_length,
            );
            let thickness = if state.scrollbar_active() {
                style.thickness + 2.0
            } else {
                style.thickness
            };
            let color = if state.scrollbar_active() {
                style.active_color
            } else {
                style.color
            };
            let color = Color {
                a: (f32::from(color.a) * alpha).round() as u8,
                ..color
            };
            let thumb_rect = if vertical {
                Rect::new(
                    Offset::new(
                        rect.origin.x + rect.size.width - MARGIN - thickness,
                        rect.origin.y + MARGIN + start,
                    ),
                    Size::new(thickness, length),
                )
            } else {
                Rect::new(
                    Offset::new(
                        rect.origin.x + MARGIN + start,
                        rect.origin.y + rect.size.height - MARGIN - thickness,
                    ),
                    Size::new(length, thickness),
                )
            };
            self.list.commands.push(DrawCommand::Rect {
                rect: thumb_rect,
                color: self.faded(color),
                corner_radius: thickness / 2.0,
                border: None,
            });
        }
        let visible = self.visible(strip);
        let hover_state = state.clone();
        self.list.hovers.push(HoverRegion {
            rect: visible,
            on_hover: Rc::new(move |hovering| hover_state.set_scrollbar_hovered(hovering)),
            covered_after: self.list.regions.len(),
        });
        // Only a bar that can be seen can be grabbed, so hidden ones leave the content its pointer.
        if alpha > 0.05 {
            let (drag_state, end_state) = (state.clone(), state.clone());
            self.list.drags.push(DragRegion {
                on_end: Some(Rc::new(move || end_state.end_thumb_drag())),
                rect: visible,
                origin: strip.origin,
                on_drag: Rc::new(move |position| {
                    let along = if vertical { position.y } else { position.x };
                    drag_state.drag_thumb(along - MARGIN, track);
                }),
            });
        }
    }

    /// Adds a description of `rect` inside the semantics being painted and returns its index.
    fn push_semantics(&mut self, rect: Rect, semantics: Semantics) -> usize {
        let index = self.list.semantics.len();
        self.list.semantics.push(SemanticsNode {
            rect: self.visible(rect),
            semantics,
            parent: self.parents.last().copied(),
            on_click: None,
        });
        index
    }

    /// Describes text as a label of its own, or gives it to the merging semantics around
    /// it, or drops it when they clear what is inside.
    fn describe_text(&mut self, rect: Rect, text: String) {
        if self.clearing > 0 {
            return;
        }
        match self.merged_text.last_mut() {
            Some(merged) => merged.push(text),
            None => {
                self.push_semantics(rect, Semantics::new(Role::Label).label(text));
            }
        }
    }

    /// Paints `measured` once at `exact`, its unrounded place, deferring raised descendants
    /// and consuming rich spans. With pixel rounding on, the area is drawn at its rounded
    /// place while its children are placed from the unrounded one, so rounding never adds up.
    fn paint(&mut self, measured: &'a Measured, exact: Offset) {
        let (origin, size) = self.snap(exact, measured.size);
        let rect = Rect::new(origin, size);
        match &measured.paint {
            Paint::Text {
                content,
                style,
                editing,
            } => {
                if editing.is_none() {
                    self.describe_text(rect, content.clone());
                }
                // The shaped text, when it is being edited: wrapped lines, selection and caret come from it.
                let edit_layout = editing
                    .as_ref()
                    .and_then(|(editing, _)| editing.layout.borrow().clone());
                let byte_of = |index: usize| {
                    content
                        .char_indices()
                        .nth(index)
                        .map_or(content.len(), |(byte, _)| byte)
                };
                if let Some((editing, _)) = editing
                    && let Some(layout) = &edit_layout
                    && let Some((start, end)) = editing.selection
                    && start != end
                {
                    for rect in layout.selection_rects(byte_of(start), byte_of(end)) {
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
                                rect.size,
                            ),
                            color: self.faded(editing.selection_color),
                            corner_radius: 0.0,
                            border: None,
                        });
                    }
                }
                self.list.commands.push(DrawCommand::Text {
                    x: origin.x,
                    y: origin.y,
                    width: measured.size.width,
                    content: content.clone(),
                    color: self.faded(style.color),
                    font_size: style.font_size,
                    line_height: style.line_height,
                    font_weight: style.font_weight.0,
                    font_family: style.font_family,
                    align: style.align,
                    max_lines: style.max_lines,
                    italic: style.italic,
                    letter_spacing: style.letter_spacing,
                    ligatures: style.ligatures,
                    wrap: style.wrap,
                });
                if style.underline || style.strikethrough {
                    let layout = match &edit_layout {
                        Some(layout) => Rc::clone(layout),
                        None => Rc::new(rustle_text::layout_spans(
                            &[TextSpan::new(content.as_str())],
                            style,
                            Some(measured.size.width + 1.0),
                        )),
                    };
                    let faded = |color| self.faded(color);
                    let lines = layout.decoration_commands(
                        origin,
                        style.underline,
                        style.strikethrough,
                        &faded,
                    );
                    self.list.commands.extend(lines);
                }
                // An input method's composition is underlined.
                if let Some((editing, boundaries)) = editing
                    && let Some((start, end)) = editing.underline
                    && start < end
                    && end < boundaries.len()
                {
                    let rects = match &edit_layout {
                        Some(layout) => layout.selection_rects(byte_of(start), byte_of(end)),
                        None => (boundaries[start].line..=boundaries[end].line)
                            .filter_map(|line| {
                                let on_line = |index: &usize| boundaries[*index].line == line;
                                let first = (start..=end).find(on_line)?;
                                let last = (start..=end).rev().find(on_line)?;
                                Some(Rect::new(
                                    Offset::new(
                                        boundaries[first].x,
                                        line as f32 * style.line_height,
                                    ),
                                    Size::new(
                                        boundaries[last].x - boundaries[first].x,
                                        style.line_height,
                                    ),
                                ))
                            })
                            .collect(),
                    };
                    for rect in rects {
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(
                                    origin.x + rect.origin.x,
                                    origin.y + rect.origin.y + rect.size.height - 2.0,
                                ),
                                Size::new(rect.size.width, 1.0),
                            ),
                            color: self.faded(style.color),
                            corner_radius: 0.0,
                            border: None,
                        });
                    }
                }
                // Where an input method puts its candidate popup: the composition up to the caret.
                if let Some((editing, boundaries)) = editing
                    && let Some(caret) = editing.ime_caret
                    && caret < boundaries.len()
                {
                    let from = editing
                        .underline
                        .map_or(caret, |(start, _)| start)
                        .min(caret);
                    let place = |index: usize| match &edit_layout {
                        Some(layout) => {
                            let at = layout.caret(byte_of(index), editing.caret_affinity);
                            (at.x, at.y, at.height)
                        }
                        None => (
                            boundaries[index].x,
                            boundaries[index].line as f32 * style.line_height,
                            style.line_height,
                        ),
                    };
                    let ((from_x, from_y, _), (x, y, height)) = (place(from), place(caret));
                    let left = if from_y == y { from_x } else { 0.0 };
                    self.list.ime_area = Some(self.visible(Rect::new(
                        Offset::new(origin.x + left, origin.y + y),
                        Size::new((x - left).max(1.0), height),
                    )));
                }
                if let Some((editing, boundaries)) = editing
                    && let Some(caret) = editing.caret
                {
                    let (x, y, height) = match &edit_layout {
                        Some(layout) => {
                            let at = layout.caret(byte_of(caret), editing.caret_affinity);
                            (at.x, at.y, at.height)
                        }
                        None => (
                            boundaries[caret].x,
                            boundaries[caret].line as f32 * style.line_height,
                            style.line_height,
                        ),
                    };
                    self.list.commands.push(DrawCommand::Rect {
                        rect: Rect::new(
                            Offset::new(origin.x + x - 0.75, origin.y + y),
                            Size::new(1.5, height),
                        ),
                        color: self.faded(editing.caret_color),
                        corner_radius: 0.0,
                        border: None,
                    });
                }
            }
            Paint::RichText {
                spans,
                style,
                editing,
            } => {
                let mut spans = spans.clone();
                let label: String = spans.iter().map(|span| span.text.as_str()).collect();
                self.describe_text(rect, label);
                if let Some(editing) = editing
                    && let Some(layout) = editing.layout.borrow().clone()
                    && let Some((start, end)) = editing.selection
                    && start != end
                {
                    let joined: String = spans.iter().map(|span| span.text.as_str()).collect();
                    let byte_of = |index: usize| {
                        joined
                            .char_indices()
                            .nth(index)
                            .map_or(joined.len(), |(byte, _)| byte)
                    };
                    for rect in layout.selection_rects(byte_of(start), byte_of(end)) {
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
                                rect.size,
                            ),
                            color: self.faded(editing.selection_color),
                            corner_radius: 0.0,
                            border: None,
                        });
                    }
                }
                // Backgrounds, underlines and strikethroughs are rectangles from the shaped layout.
                let decorated = style.underline
                    || style.strikethrough
                    || spans.iter().any(|span| {
                        span.underline || span.strikethrough || span.background.is_some()
                    });
                let layout = decorated.then(|| {
                    rustle_text::layout_styled(&spans, style, Some(measured.size.width + 1.0))
                });
                if let Some(layout) = &layout {
                    let faded = |color| self.faded(color);
                    let backgrounds = layout.background_commands(origin, &faded);
                    self.list.commands.extend(backgrounds);
                }
                for span in &mut spans {
                    span.color = self.faded(span.color);
                }
                self.list.commands.push(DrawCommand::RichText {
                    x: origin.x,
                    y: origin.y,
                    width: measured.size.width,
                    spans,
                    font_size: style.font_size,
                    line_height: style.line_height,
                    align: style.align,
                    max_lines: style.max_lines,
                    wrap: style.wrap,
                });
                if let Some(layout) = &layout {
                    let faded = |color| self.faded(color);
                    let lines = layout.decoration_commands(origin, false, false, &faded);
                    self.list.commands.extend(lines);
                }
            }
            Paint::Canvas { draw } => {
                for command in draw(size) {
                    self.list
                        .commands
                        .push(command.translated(origin.x, origin.y));
                }
            }
            Paint::Image { image, scale } => {
                if self.merging == 0 && self.clearing == 0 {
                    self.push_semantics(rect, Semantics::new(Role::Image));
                }
                let (width, height) = (image.width() as f32, image.height() as f32);
                let fit = (size.width / width).min(size.height / height);
                let cover = (size.width / width).max(size.height / height);
                let drawn = match scale {
                    ContentScale::Stretch => size,
                    ContentScale::Fit => Size::new(width * fit, height * fit),
                    ContentScale::Crop => Size::new(width * cover, height * cover),
                };
                let at = Offset::new(
                    origin.x + (size.width - drawn.width) / 2.0,
                    origin.y + (size.height - drawn.height) / 2.0,
                );
                let crop = *scale == ContentScale::Crop;
                if crop {
                    self.push_clip(rect);
                }
                self.list.commands.push(DrawCommand::Image {
                    rect: Rect::new(at, drawn),
                    image: image.clone(),
                    corner_radius: 0.0,
                    opacity: self.alpha,
                });
                if crop {
                    self.pop_clip();
                }
            }
            Paint::Shared(shared) => self.paint(shared, exact),
            Paint::Children(children) => {
                for (offset, child) in children {
                    self.paint(child, Offset::new(exact.x + offset.x, exact.y + offset.y));
                }
            }
            Paint::Background {
                color,
                corner_radius,
                inner,
            } => {
                self.list.commands.push(DrawCommand::Rect {
                    rect,
                    color: self.faded(*color),
                    corner_radius: *corner_radius,
                    border: None,
                });
                self.paint(inner, exact);
            }
            Paint::Draw { draw, inner } => {
                let (commands, content) = draw(size);
                let count = commands.len();
                for (index, command) in commands.into_iter().enumerate() {
                    if content == Some(index) {
                        self.paint(inner, exact);
                    }
                    self.list
                        .commands
                        .push(command.translated(origin.x, origin.y));
                }
                if content == Some(count) {
                    self.paint(inner, exact);
                }
            }
            Paint::Fill { fill, inner } => {
                self.paint_fill(fill, rect);
                self.paint(inner, exact);
            }
            Paint::Effect { effect, inner } => {
                let faded = LayerEffect {
                    alpha: effect.alpha * self.alpha,
                    ..*effect
                };
                self.list.commands.push(DrawCommand::PushEffect {
                    rect,
                    effect: faded,
                });
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::PopEffect);
            }
            Paint::Gradient {
                from,
                to,
                angle,
                corner_radius,
                inner,
            } => {
                self.list.commands.push(DrawCommand::Gradient {
                    rect,
                    from: self.faded(*from),
                    to: self.faded(*to),
                    angle: *angle,
                    corner_radius: *corner_radius,
                });
                self.paint(inner, exact);
            }
            Paint::Ring {
                color,
                thickness,
                start,
                sweep,
                inner,
            } => {
                self.paint(inner, exact);
                let side = size.width.min(size.height);
                self.list.commands.push(DrawCommand::Arc {
                    rect: Rect::new(
                        Offset::new(
                            origin.x + (size.width - side) / 2.0,
                            origin.y + (size.height - side) / 2.0,
                        ),
                        Size::new(side, side),
                    ),
                    color: self.faded(*color),
                    thickness: *thickness,
                    start: *start,
                    sweep: *sweep,
                });
            }
            Paint::DragEnd { on_end, inner } => {
                // Applies to the drag handler declared next, inside this.
                self.pending_end = Some(Rc::clone(on_end));
                self.paint(inner, exact);
                self.pending_end = None;
            }
            Paint::DragSource {
                payload,
                button,
                inner,
            } => {
                let start = self.list.commands.len();
                let index = self.list.drag_sources.len();
                self.list.drag_sources.push(DragSourceRegion {
                    rect: self.visible(rect),
                    button: *button,
                    payload: Rc::clone(payload),
                    commands: start..start,
                });
                self.paint(inner, exact);
                self.list.drag_sources[index].commands.end = self.list.commands.len();
            }
            Paint::DropTarget { on_drop, inner } => {
                self.list.drop_targets.push(DropTargetRegion {
                    rect: self.visible(rect),
                    on_drop: Rc::clone(on_drop),
                });
                self.paint(inner, exact);
            }
            Paint::FileDrop { on_drop, inner } => {
                self.list.file_drops.push(FileDropRegion {
                    rect: self.visible(rect),
                    on_drop: Rc::clone(on_drop),
                });
                self.paint(inner, exact);
            }
            Paint::PointerInput { on_event, inner } => {
                self.list.pointer_inputs.push(PointerInputRegion {
                    rect: self.visible(rect),
                    origin,
                    on_event: Rc::clone(on_event),
                });
                self.paint(inner, exact);
            }
            Paint::Pinch { on_pinch, inner } => {
                self.list.pinches.push(PinchRegion {
                    rect: self.visible(rect),
                    on_pinch: Rc::clone(on_pinch),
                });
                self.paint(inner, exact);
            }
            Paint::BlocksPointer(inner) => {
                self.list.blockers.push(self.visible(rect));
                self.paint(inner, exact);
            }
            Paint::PointerMove { on_move, inner } => {
                let index = self.list.pointer_moves.len();
                self.list.pointer_moves.push(PointerMoveRegion {
                    rect: self.visible(rect),
                    origin,
                    on_move: Rc::clone(on_move),
                    covered_after: 0,
                });
                self.paint(inner, exact);
                self.list.pointer_moves[index].covered_after = self.list.regions.len();
            }
            Paint::LongPress {
                on_long_press,
                inner,
            } => {
                self.list.long_presses.push(LongPressRegion {
                    rect: self.visible(rect),
                    on_long_press: Rc::clone(on_long_press),
                });
                self.paint(inner, exact);
            }
            Paint::Cursor { icon, inner } => {
                self.list.cursors.push(CursorRegion {
                    rect: self.visible(rect),
                    icon: *icon,
                });
                self.paint(inner, exact);
            }
            Paint::Hover { on_hover, inner } => {
                let index = self.list.hovers.len();
                self.list.hovers.push(HoverRegion {
                    rect: self.visible(rect),
                    on_hover: Rc::clone(on_hover),
                    covered_after: 0,
                });
                self.paint(inner, exact);
                self.list.hovers[index].covered_after = self.list.regions.len();
            }
            Paint::Placed { on_placed, inner } => {
                on_placed(rect, self.window);
                self.paint(inner, exact);
            }
            Paint::ZIndex {
                z_index,
                clipped,
                inner,
            } => {
                let clips = if *clipped {
                    self.clips
                        .iter()
                        .copied()
                        .zip(self.clip_radii.iter().copied())
                        .collect()
                } else {
                    Vec::new()
                };
                self.deferred.push(Deferred {
                    z_index: *z_index,
                    origin: exact,
                    alpha: self.alpha,
                    clips,
                    parents: self.parents.clone(),
                    clearing: self.clearing,
                    measured: inner,
                });
            }
            Paint::BorderSide {
                width,
                color,
                inner,
            } => {
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(origin.x + size.width - width, origin.y),
                        Size::new(*width, size.height),
                    ),
                    color: self.faded(*color),
                    corner_radius: 0.0,
                    border: None,
                });
            }
            Paint::BorderBottom {
                width,
                color,
                inner,
            } => {
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(origin.x, origin.y + size.height - width),
                        Size::new(size.width, *width),
                    ),
                    color: self.faded(*color),
                    corner_radius: 0.0,
                    border: None,
                });
            }
            Paint::GraphicsLayer {
                alpha,
                rotation,
                scale,
                inner,
            } => {
                self.list.commands.push(DrawCommand::PushLayer {
                    rect,
                    alpha: *alpha,
                    rotation: *rotation,
                    scale: *scale,
                });
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::PopLayer);
            }
            Paint::Alpha { alpha, inner } => {
                let before = self.alpha;
                self.alpha *= alpha;
                self.paint(inner, exact);
                self.alpha = before;
            }
            Paint::Border {
                width,
                color,
                corner_radius,
                inner,
            } => {
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::Rect {
                    rect,
                    color: Color::TRANSPARENT,
                    corner_radius: *corner_radius,
                    border: Some(Border {
                        width: *width,
                        color: self.faded(*color),
                    }),
                });
            }
            Paint::Shadow {
                elevation,
                corner_radius,
                inner,
            } => {
                self.list.commands.push(DrawCommand::Shadow {
                    rect: Rect::new(Offset::new(origin.x, origin.y + elevation / 2.0), size),
                    corner_radius: *corner_radius,
                    blur: *elevation,
                    color: self.faded(Color::rgba(0, 0, 0, 110)),
                });
                self.paint(inner, exact);
            }
            Paint::Clip(inner) => {
                self.push_clip(rect);
                self.paint(inner, exact);
                self.pop_clip();
            }
            Paint::ClipRounded { radii, inner } => {
                self.push_rounded_clip(rect, *radii);
                self.paint(inner, exact);
                self.pop_clip();
            }
            Paint::Scroll {
                state,
                vertical,
                offset,
                inner,
            } => {
                self.push_clip(rect);
                self.list.scrolls.push(ScrollRegion {
                    rect: self.visible(rect),
                    state: state.clone(),
                    vertical: *vertical,
                });
                let inside = if *vertical {
                    Offset::new(exact.x, exact.y - offset)
                } else {
                    Offset::new(exact.x - offset, exact.y)
                };
                self.paint(inner, inside);
                self.paint_scrollbar(rect, state, *vertical, *offset);
                self.pop_clip();
            }
            Paint::Drag { on_drag, inner } => {
                self.list.drags.push(DragRegion {
                    on_end: self.pending_end.clone(),
                    rect: self.visible(rect),
                    origin,
                    on_drag: Rc::clone(on_drag),
                });
                self.paint(inner, exact);
            }
            Paint::Semantics { semantics, inner } => {
                let regions = self.list.regions.len();
                let described = (self.clearing == 0).then(|| {
                    let index = self.push_semantics(rect, semantics.clone());
                    self.parents.push(index);
                    index
                });
                let merges = described.is_some() && semantics.merge_descendants;
                if merges {
                    self.merging += 1;
                    self.merged_text.push(Vec::new());
                }
                if semantics.clear_descendants {
                    self.clearing += 1;
                }
                self.paint(inner, exact);
                if semantics.clear_descendants {
                    self.clearing -= 1;
                }
                let Some(index) = described else {
                    return;
                };
                self.parents.pop();
                if merges {
                    self.merging -= 1;
                    let text = self.merged_text.pop().unwrap_or_default();
                    if self.list.semantics[index].semantics.label.is_none() && !text.is_empty() {
                        self.merged_labels.push(index);
                    }
                    self.add_merged_text(index, text);
                }
                self.list.semantics[index].on_click = self.list.regions[regions..]
                    .iter()
                    .find(|region| region.rect == self.list.semantics[index].rect)
                    .map(|region| Rc::clone(&region.on_click));
            }
            Paint::Press { on_press, inner } => {
                self.list.presses.push(PressRegion {
                    rect: self.visible(rect),
                    origin,
                    on_press: Rc::clone(on_press),
                });
                self.paint(inner, exact);
            }
            Paint::Button {
                button,
                on_event,
                inner,
            } => {
                self.list.buttons.push(ButtonRegion {
                    rect: self.visible(rect),
                    origin,
                    button: *button,
                    on_event: Rc::clone(on_event),
                });
                self.paint(inner, exact);
            }
            Paint::KeyEvent {
                on_key,
                preview,
                inner,
            } => {
                let index = self.list.key_handlers.len();
                let first = self.list.focusables.len();
                self.list.key_handlers.push(KeyRegion {
                    rect: self.visible(rect),
                    focusables: first..first,
                    preview: *preview,
                    on_key: Rc::clone(on_key),
                });
                self.paint(inner, exact);
                self.list.key_handlers[index].focusables.end = self.list.focusables.len();
            }
            Paint::Focusable {
                focus,
                on_key,
                inner,
            } => {
                self.list.focusables.push(FocusRegion {
                    rect: self.visible(rect),
                    focus: focus.clone(),
                    on_key: Rc::clone(on_key),
                });
                self.paint(inner, exact);
            }
            Paint::Clickable {
                on_click,
                feedback,
                inner,
            } => {
                let corner_radius = match &inner.paint {
                    Paint::Background { corner_radius, .. } => *corner_radius,
                    _ => 0.0,
                };
                self.list.regions.push(HitRegion {
                    rect: self.visible(rect),
                    corner_radius,
                    feedback: *feedback,
                    on_click: Rc::clone(on_click),
                });
                self.paint(inner, exact);
            }
        }
    }
}

#[cfg(test)]
#[path = "paint_tests.rs"]
mod tests;
