use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, Color, DrawCommand};
use rustle_text::{StyledSpan, TextMeasurer, TextSpan};
use rustle_unit::Constraints;

use super::ContentScale;
use super::measure::measure_node;
use super::measured::{Measured, Paint};
use super::regions::{
    ButtonRegion, CursorRegion, DisplayList, DragRegion, DragSourceRegion, DropTargetRegion,
    FileDropRegion, FocusRegion, HitRegion, HoverRegion, KeyRegion, LongPressRegion, PinchRegion,
    PointerMoveRegion, PressRegion, ScrollRegion, SemanticsNode, intersect,
};
use crate::Node;
use crate::scroll::{ScrollState, ScrollbarVisibility, thumb};
use crate::semantics::{Role, Semantics};

/// Lays out `root` within `constraints`.
pub fn display_list(
    root: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> DisplayList {
    let measured = measure_node(root, constraints, text);
    let mut painter = Painter {
        pending_end: None,
        deferred: Vec::new(),
        alpha: 1.0,
        merging: 0,
        list: DisplayList::default(),
        clips: Vec::new(),
        clip_radii: Vec::new(),
        window: measured.size,
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
    measured: &'a Measured,
}

struct Painter<'a> {
    /// The drag-end handler of the area being painted, for the drag handler inside it.
    pending_end: Option<Rc<dyn Fn()>>,
    /// Raised areas, painted over the rest in order of z-index.
    deferred: Vec<Deferred<'a>>,
    /// How opaque everything painted now is, from the alpha modifiers around it.
    alpha: f32,
    /// How many enclosing semantics merge the text inside into themselves.
    merging: usize,
    list: DisplayList,
    /// The clips in effect, innermost last, cutting hit and scroll regions.
    clips: Vec<Rect>,
    /// The corner radii of each of `clips`, if it is rounded.
    clip_radii: Vec<Option<[f32; 4]>>,
    /// How large the window is.
    window: Size,
}

impl<'a> Painter<'a> {
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
                self.paint(raised.measured, raised.origin);
                for _ in &raised.clips {
                    self.pop_clip();
                }
            }
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

    fn paint(&mut self, measured: &'a Measured, origin: Offset) {
        let rect = Rect::new(origin, measured.size);
        match &measured.paint {
            Paint::Text {
                content,
                style,
                editing,
            } => {
                if self.merging == 0 && editing.is_none() {
                    self.list.semantics.push(SemanticsNode {
                        rect: self.visible(rect),
                        semantics: Semantics::new(Role::Label).label(content.clone()),
                        on_click: None,
                    });
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
                if self.merging == 0 {
                    let label: String = spans.iter().map(|span| span.text.as_str()).collect();
                    self.list.semantics.push(SemanticsNode {
                        rect: self.visible(rect),
                        semantics: Semantics::new(Role::Label).label(label),
                        on_click: None,
                    });
                }
                let joined: String = spans.iter().map(|span| span.text.as_str()).collect();
                if let Some(editing) = editing
                    && let Some(layout) = editing.layout.borrow().clone()
                    && let Some((start, end)) = editing.selection
                    && start != end
                {
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
                    rustle_text::layout_styled(spans, style, Some(measured.size.width + 1.0))
                });
                if let Some(layout) = &layout {
                    let faded = |color| self.faded(color);
                    let backgrounds = layout.background_commands(origin, &faded);
                    self.list.commands.extend(backgrounds);
                }
                let faded = spans
                    .iter()
                    .map(|span| StyledSpan {
                        color: self.faded(span.color),
                        ..span.clone()
                    })
                    .collect();
                self.list.commands.push(DrawCommand::RichText {
                    x: origin.x,
                    y: origin.y,
                    width: measured.size.width,
                    spans: faded,
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
                for command in draw(measured.size) {
                    self.list
                        .commands
                        .push(command.translated(origin.x, origin.y));
                }
            }
            Paint::Image { image, scale } => {
                if self.merging == 0 {
                    self.list.semantics.push(SemanticsNode {
                        rect: self.visible(rect),
                        semantics: Semantics::new(Role::Image),
                        on_click: None,
                    });
                }
                let (width, height) = (image.width() as f32, image.height() as f32);
                let fit = (measured.size.width / width).min(measured.size.height / height);
                let cover = (measured.size.width / width).max(measured.size.height / height);
                let drawn = match scale {
                    ContentScale::Stretch => measured.size,
                    ContentScale::Fit => Size::new(width * fit, height * fit),
                    ContentScale::Crop => Size::new(width * cover, height * cover),
                };
                let at = Offset::new(
                    origin.x + (measured.size.width - drawn.width) / 2.0,
                    origin.y + (measured.size.height - drawn.height) / 2.0,
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
            Paint::Children(children) => {
                for (offset, child) in children {
                    self.paint(child, Offset::new(origin.x + offset.x, origin.y + offset.y));
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
                self.paint(inner, origin);
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
                self.paint(inner, origin);
            }
            Paint::Ring {
                color,
                thickness,
                start,
                sweep,
                inner,
            } => {
                self.paint(inner, origin);
                let side = measured.size.width.min(measured.size.height);
                self.list.commands.push(DrawCommand::Arc {
                    rect: Rect::new(
                        Offset::new(
                            origin.x + (measured.size.width - side) / 2.0,
                            origin.y + (measured.size.height - side) / 2.0,
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
                self.paint(inner, origin);
                self.pending_end = None;
            }
            Paint::DragSource { payload, inner } => {
                let start = self.list.commands.len();
                let index = self.list.drag_sources.len();
                self.list.drag_sources.push(DragSourceRegion {
                    rect: self.visible(rect),
                    payload: Rc::clone(payload),
                    commands: start..start,
                });
                self.paint(inner, origin);
                self.list.drag_sources[index].commands.end = self.list.commands.len();
            }
            Paint::DropTarget { on_drop, inner } => {
                self.list.drop_targets.push(DropTargetRegion {
                    rect: self.visible(rect),
                    on_drop: Rc::clone(on_drop),
                });
                self.paint(inner, origin);
            }
            Paint::FileDrop { on_drop, inner } => {
                self.list.file_drops.push(FileDropRegion {
                    rect: self.visible(rect),
                    on_drop: Rc::clone(on_drop),
                });
                self.paint(inner, origin);
            }
            Paint::Pinch { on_pinch, inner } => {
                self.list.pinches.push(PinchRegion {
                    rect: self.visible(rect),
                    on_pinch: Rc::clone(on_pinch),
                });
                self.paint(inner, origin);
            }
            Paint::BlocksPointer(inner) => {
                self.list.blockers.push(self.visible(rect));
                self.paint(inner, origin);
            }
            Paint::PointerMove { on_move, inner } => {
                let index = self.list.pointer_moves.len();
                self.list.pointer_moves.push(PointerMoveRegion {
                    rect: self.visible(rect),
                    origin,
                    on_move: Rc::clone(on_move),
                    covered_after: 0,
                });
                self.paint(inner, origin);
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
                self.paint(inner, origin);
            }
            Paint::Cursor { icon, inner } => {
                self.list.cursors.push(CursorRegion {
                    rect: self.visible(rect),
                    icon: *icon,
                });
                self.paint(inner, origin);
            }
            Paint::Hover { on_hover, inner } => {
                let index = self.list.hovers.len();
                self.list.hovers.push(HoverRegion {
                    rect: self.visible(rect),
                    on_hover: Rc::clone(on_hover),
                    covered_after: 0,
                });
                self.paint(inner, origin);
                self.list.hovers[index].covered_after = self.list.regions.len();
            }
            Paint::Placed { on_placed, inner } => {
                on_placed(rect, self.window);
                self.paint(inner, origin);
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
                    origin,
                    alpha: self.alpha,
                    clips,
                    measured: inner,
                });
            }
            Paint::BorderSide {
                width,
                color,
                inner,
            } => {
                self.paint(inner, origin);
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(origin.x + measured.size.width - width, origin.y),
                        Size::new(*width, measured.size.height),
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
                self.paint(inner, origin);
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(origin.x, origin.y + measured.size.height - width),
                        Size::new(measured.size.width, *width),
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
                self.paint(inner, origin);
                self.list.commands.push(DrawCommand::PopLayer);
            }
            Paint::Alpha { alpha, inner } => {
                let before = self.alpha;
                self.alpha *= alpha;
                self.paint(inner, origin);
                self.alpha = before;
            }
            Paint::Border {
                width,
                color,
                corner_radius,
                inner,
            } => {
                self.paint(inner, origin);
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
                    rect: Rect::new(
                        Offset::new(origin.x, origin.y + elevation / 2.0),
                        measured.size,
                    ),
                    corner_radius: *corner_radius,
                    blur: *elevation,
                    color: self.faded(Color::rgba(0, 0, 0, 110)),
                });
                self.paint(inner, origin);
            }
            Paint::Clip(inner) => {
                self.push_clip(rect);
                self.paint(inner, origin);
                self.pop_clip();
            }
            Paint::ClipRounded { radii, inner } => {
                self.push_rounded_clip(rect, *radii);
                self.paint(inner, origin);
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
                let origin = if *vertical {
                    Offset::new(origin.x, origin.y - offset)
                } else {
                    Offset::new(origin.x - offset, origin.y)
                };
                self.paint(inner, origin);
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
                self.paint(inner, origin);
            }
            Paint::Semantics { semantics, inner } => {
                let index = self.list.semantics.len();
                let regions = self.list.regions.len();
                self.list.semantics.push(SemanticsNode {
                    rect: self.visible(rect),
                    semantics: semantics.clone(),
                    on_click: None,
                });
                if semantics.merge_descendants {
                    self.merging += 1;
                }
                self.paint(inner, origin);
                if semantics.merge_descendants {
                    self.merging -= 1;
                }
                // Activating the description does what clicking the area does.
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
                self.paint(inner, origin);
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
                self.paint(inner, origin);
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
                self.paint(inner, origin);
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
                self.paint(inner, origin);
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
                self.paint(inner, origin);
            }
        }
    }
}
