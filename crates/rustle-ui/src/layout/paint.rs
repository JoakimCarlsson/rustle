use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, Color, DrawCommand};
use rustle_text::{StyledSpan, TextMeasurer};
use rustle_unit::Constraints;

use super::ContentScale;
use super::measure::measure_node;
use super::measured::{Measured, Paint};
use super::regions::{
    DisplayList, DragRegion, DragSourceRegion, DropTargetRegion, FileDropRegion, FocusRegion,
    HitRegion, HoverRegion, LongPressRegion, PinchRegion, PointerMoveRegion, PressRegion,
    ScrollRegion, SemanticsNode, intersect,
};
use crate::Node;
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
                self.paint(raised.measured, raised.origin);
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
        self.list.commands.push(DrawCommand::PushClip { rect });
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
        self.list.commands.push(DrawCommand::PopClip);
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
                if let Some((editing, boundaries)) = editing
                    && let Some((start, end)) = editing.selection
                    && start != end
                {
                    let (start, end) = (start.min(end), start.max(end));
                    for line in boundaries[start].line..=boundaries[end].line {
                        let on_line = |index: &usize| boundaries[*index].line == line;
                        let (Some(first), Some(last)) = (
                            (start..=end).find(on_line),
                            (start..=end).rev().find(on_line),
                        ) else {
                            continue;
                        };
                        // A selected line break shows as a little extra width.
                        let extra = if line < boundaries[end].line {
                            6.0
                        } else {
                            0.0
                        };
                        let left = boundaries[first].x;
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(
                                    origin.x + left,
                                    origin.y + line as f32 * style.line_height,
                                ),
                                Size::new(boundaries[last].x - left + extra, style.line_height),
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
                });
                if let Some((editing, boundaries)) = editing
                    && let Some(caret) = editing.caret
                {
                    self.list.commands.push(DrawCommand::Rect {
                        rect: Rect::new(
                            Offset::new(
                                origin.x + boundaries[caret].x - 0.75,
                                origin.y + boundaries[caret].line as f32 * style.line_height,
                            ),
                            Size::new(1.5, style.line_height),
                        ),
                        color: self.faded(editing.caret_color),
                        corner_radius: 0.0,
                        border: None,
                    });
                }
            }
            Paint::RichText { spans, style } => {
                if self.merging == 0 {
                    let label: String = spans.iter().map(|span| span.text.as_str()).collect();
                    self.list.semantics.push(SemanticsNode {
                        rect: self.visible(rect),
                        semantics: Semantics::new(Role::Label).label(label),
                        on_click: None,
                    });
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
                });
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
            Paint::PointerMove { on_move, inner } => {
                self.list.pointer_moves.push(PointerMoveRegion {
                    rect: self.visible(rect),
                    origin,
                    on_move: Rc::clone(on_move),
                });
                self.paint(inner, origin);
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
            Paint::Hover { on_hover, inner } => {
                self.list.hovers.push(HoverRegion {
                    rect: self.visible(rect),
                    on_hover: Rc::clone(on_hover),
                });
                self.paint(inner, origin);
            }
            Paint::Placed { on_placed, inner } => {
                on_placed(rect, self.window);
                self.paint(inner, origin);
            }
            Paint::ZIndex { z_index, inner } => self.deferred.push(Deferred {
                z_index: *z_index,
                origin,
                alpha: self.alpha,
                measured: inner,
            }),
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
