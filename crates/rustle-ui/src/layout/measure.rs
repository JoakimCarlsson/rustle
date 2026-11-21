use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_text::{StyledSpan, TextAlign, TextLayout, TextMeasurer, TextSpan, TextStyle};
use rustle_unit::Constraints;

use super::intrinsic::{Query, intrinsic_modified};
use super::measured::{Measured, Paint};
use super::{Children, IntrinsicSize, Measurable};
use crate::modifier::Element;
use crate::{CharPosition, Content, Node};
use rustle_runtime::{Reads, Scope, mark_unskippable, track_reads};

/// Measures `node` within `constraints`, retaining its painting data.
pub(super) fn measure_node(
    node: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> Measured {
    measure_modified(&node.modifier.elements, node, constraints, text)
}

/// Wraps `inner` in a paint modifier without changing its measured size.
fn wrap(inner: Measured, paint: impl FnOnce(Box<Measured>) -> Paint) -> Measured {
    Measured {
        size: inner.size,
        paint: paint(Box::new(inner)),
    }
}

/// Applies the modifier `elements` from the outermost in, then measures the node itself.
pub(super) fn measure_modified(
    elements: &[Element],
    node: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> Measured {
    let Some((element, rest)) = elements.split_first() else {
        return measure_content(node, constraints, text);
    };
    let mut next = |constraints: Constraints| measure_modified(rest, node, constraints, text);
    match element {
        Element::Padding {
            left,
            top,
            right,
            bottom,
        } => {
            let inner = next(constraints.deflate(left + right, top + bottom));
            let size = constraints.constrain(Size::new(
                inner.size.width + left + right,
                inner.size.height + top + bottom,
            ));
            Measured {
                size,
                paint: Paint::Children(vec![(Offset::new(*left, *top), inner)]),
            }
        }
        Element::Size { width, height } => {
            let mut tightened = constraints;
            if let Some(width) = width {
                let width = width.clamp(constraints.min_width, constraints.max_width);
                tightened.min_width = width;
                tightened.max_width = width;
            }
            if let Some(height) = height {
                let height = height.clamp(constraints.min_height, constraints.max_height);
                tightened.min_height = height;
                tightened.max_height = height;
            }
            next(tightened)
        }
        Element::SizeIn {
            min_width,
            max_width,
            min_height,
            max_height,
        } => {
            let mut bounded = constraints;
            if let Some(min) = min_width {
                bounded.min_width = min.clamp(constraints.min_width, constraints.max_width);
            }
            if let Some(max) = max_width {
                bounded.max_width = max.clamp(bounded.min_width, constraints.max_width);
            }
            if let Some(min) = min_height {
                bounded.min_height = min.clamp(constraints.min_height, constraints.max_height);
            }
            if let Some(max) = max_height {
                bounded.max_height = max.clamp(bounded.min_height, constraints.max_height);
            }
            next(bounded)
        }
        Element::FillMax { width, height } => {
            let mut filled = constraints;
            if let Some(fraction) = width
                && constraints.max_width.is_finite()
            {
                let filling = (constraints.max_width * fraction)
                    .clamp(constraints.min_width, constraints.max_width);
                filled.min_width = filling;
                filled.max_width = filling;
            }
            if let Some(fraction) = height
                && constraints.max_height.is_finite()
            {
                let filling = (constraints.max_height * fraction)
                    .clamp(constraints.min_height, constraints.max_height);
                filled.min_height = filling;
                filled.max_height = filling;
            }
            next(filled)
        }
        Element::AspectRatio(ratio) => {
            let size = if *ratio <= 0.0 {
                None
            } else if constraints.max_width.is_finite() {
                let (mut width, mut height) =
                    (constraints.max_width, constraints.max_width / ratio);
                if height > constraints.max_height {
                    height = constraints.max_height;
                    width = height * ratio;
                }
                Some((width, height))
            } else if constraints.max_height.is_finite() {
                Some((constraints.max_height * ratio, constraints.max_height))
            } else {
                None
            };
            match size {
                Some((width, height)) => {
                    let size = constraints.constrain(Size::new(width, height));
                    next(Constraints {
                        min_width: size.width,
                        max_width: size.width,
                        min_height: size.height,
                        max_height: size.height,
                    })
                }
                None => next(constraints),
            }
        }
        Element::Offset { x, y } => {
            let inner = next(constraints);
            Measured {
                size: inner.size,
                paint: Paint::Children(vec![(Offset::new(*x, *y), inner)]),
            }
        }
        Element::Background {
            color,
            corner_radius,
        } => wrap(next(constraints), |inner| Paint::Background {
            color: *color,
            corner_radius: *corner_radius,
            inner,
        }),
        Element::InteractiveDraw(draw) => wrap(next(constraints), |inner| Paint::Interactive {
            draw: Rc::clone(draw),
            inner,
        }),
        Element::KeepInWindow => wrap(next(constraints), |inner| Paint::KeepInWindow { inner }),
        Element::Popup(placement) => wrap(next(constraints), |inner| Paint::Popup {
            placement: placement.clone(),
            inner,
        }),
        Element::InWindow(x, y) => wrap(next(constraints), |inner| Paint::InWindow {
            at: Offset::new(*x, *y),
            inner,
        }),
        Element::InteractionGroup => {
            wrap(next(constraints), |inner| Paint::InteractionGroup { inner })
        }
        Element::InteractionFocused(focused) => {
            wrap(next(constraints), |inner| Paint::InteractionFocused {
                focused: Rc::clone(focused),
                inner,
            })
        }
        Element::Draw(draw) => wrap(next(constraints), |inner| Paint::Draw {
            draw: Rc::clone(draw),
            inner,
        }),
        Element::Fill(fill) => wrap(next(constraints), |inner| Paint::Fill {
            fill: fill.clone(),
            inner,
        }),
        Element::Effect(effect) => wrap(next(constraints), |inner| Paint::Effect {
            effect: *effect,
            inner,
        }),
        Element::Gradient {
            from,
            to,
            angle,
            corner_radius,
        } => wrap(next(constraints), |inner| Paint::Gradient {
            from: *from,
            to: *to,
            angle: *angle,
            corner_radius: *corner_radius,
            inner,
        }),
        Element::Ring {
            color,
            thickness,
            start,
            sweep,
        } => wrap(next(constraints), |inner| Paint::Ring {
            color: *color,
            thickness: *thickness,
            start: *start,
            sweep: *sweep,
            inner,
        }),
        Element::DragEnd(on_end) => wrap(next(constraints), |inner| Paint::DragEnd {
            on_end: Rc::clone(on_end),
            inner,
        }),
        Element::PointerInput(on_event) => wrap(next(constraints), |inner| Paint::PointerInput {
            on_event: Rc::clone(on_event),
            inner,
        }),
        Element::PointerEvent {
            event_type,
            pass,
            on_event,
        } => wrap(next(constraints), |inner| Paint::PointerEvent {
            event_type: *event_type,
            pass: *pass,
            on_event: Rc::clone(on_event),
            inner,
        }),
        Element::TextInput(session) => wrap(next(constraints), |inner| Paint::TextInput {
            session: session.clone(),
            inner,
        }),
        Element::DragSource {
            payload,
            button,
            touch_slop,
        } => wrap(next(constraints), |inner| Paint::DragSource {
            payload: Rc::clone(payload),
            button: *button,
            touch_slop: *touch_slop,
            inner,
        }),
        Element::DropTarget(on_drop) => wrap(next(constraints), |inner| Paint::DropTarget {
            on_drop: Rc::clone(on_drop),
            inner,
        }),
        Element::DragAndDropSource(transfer_data, touch_slop) => {
            wrap(next(constraints), |inner| Paint::DragAndDropSource {
                transfer_data: Rc::clone(transfer_data),
                touch_slop: *touch_slop,
                inner,
            })
        }
        Element::DragAndDropTarget(should_start, target) => {
            wrap(next(constraints), |inner| Paint::DragAndDropTarget {
                should_start: Rc::clone(should_start),
                target: Rc::clone(target),
                inner,
            })
        }
        Element::FileDrop(on_drop) => wrap(next(constraints), |inner| Paint::FileDrop {
            on_drop: Rc::clone(on_drop),
            inner,
        }),
        Element::Pinch(on_pinch) => wrap(next(constraints), |inner| Paint::Pinch {
            on_pinch: Rc::clone(on_pinch),
            inner,
        }),
        Element::BlocksPointer => wrap(next(constraints), Paint::BlocksPointer),
        Element::PointerMove(on_move) => wrap(next(constraints), |inner| Paint::PointerMove {
            on_move: Rc::clone(on_move),
            inner,
        }),
        Element::LongPress(on_long_press, view_configuration) => {
            wrap(next(constraints), |inner| Paint::LongPress {
                on_long_press: Rc::clone(on_long_press),
                view_configuration: *view_configuration,
                inner,
            })
        }
        Element::TextLayout(_) => next(constraints),
        Element::SizeChanged(on_size_changed) => {
            let inner = next(constraints);
            on_size_changed(inner.size);
            inner
        }
        Element::Cursor(icon) => wrap(next(constraints), |inner| Paint::Cursor {
            icon: *icon,
            inner,
        }),
        Element::ImeCaret(rect) => wrap(next(constraints), |inner| Paint::ImeCaret {
            rect: *rect,
            inner,
        }),
        Element::Hover(on_hover) => wrap(next(constraints), |inner| Paint::Hover {
            on_hover: Rc::clone(on_hover),
            inner,
        }),
        Element::Placed(on_placed) => wrap(next(constraints), |inner| Paint::Placed {
            on_placed: Rc::clone(on_placed),
            inner,
        }),
        Element::PlacedVisible(on_visible) => {
            wrap(next(constraints), |inner| Paint::PlacedVisible {
                on_visible: Rc::clone(on_visible),
                inner,
            })
        }
        Element::ZIndex(z_index) => wrap(next(constraints), |inner| Paint::ZIndex {
            z_index: *z_index,
            clipped: true,
            inner,
        }),
        Element::ZIndexUnclipped(z_index) => wrap(next(constraints), |inner| Paint::ZIndex {
            z_index: *z_index,
            clipped: false,
            inner,
        }),
        Element::GraphicsLayer {
            alpha,
            rotation,
            scale,
        } => wrap(next(constraints), |inner| Paint::GraphicsLayer {
            alpha: *alpha,
            rotation: *rotation,
            scale: *scale,
            inner,
        }),
        Element::RotateWith(degrees) => wrap(next(constraints), |inner| Paint::RotateWith {
            degrees: Rc::clone(degrees),
            inner,
        }),
        Element::Alpha(alpha) => wrap(next(constraints), |inner| Paint::Alpha {
            alpha: *alpha,
            inner,
        }),
        Element::BorderSide { width, color } => {
            wrap(next(constraints), |inner| Paint::BorderSide {
                width: *width,
                color: *color,
                inner,
            })
        }
        Element::BorderBottom { width, color } => {
            wrap(next(constraints), |inner| Paint::BorderBottom {
                width: *width,
                color: *color,
                inner,
            })
        }
        Element::Border {
            width,
            color,
            corner_radius,
        } => wrap(next(constraints), |inner| Paint::Border {
            width: *width,
            color: *color,
            corner_radius: *corner_radius,
            inner,
        }),
        Element::Shadow {
            elevation,
            corner_radius,
        } => wrap(next(constraints), |inner| Paint::Shadow {
            elevation: *elevation,
            corner_radius: *corner_radius,
            inner,
        }),
        Element::Clip => wrap(next(constraints), Paint::Clip),
        Element::ClipRounded(radii) => wrap(next(constraints), |inner| Paint::ClipRounded {
            radii: *radii,
            inner,
        }),
        Element::Scroll { state, vertical } => {
            let mut unbounded = constraints;
            if *vertical {
                unbounded.max_height = f32::INFINITY;
            } else {
                unbounded.max_width = f32::INFINITY;
            }
            let inner = next(unbounded);
            let size = constraints.constrain(inner.size);
            let (content, viewport) = if *vertical {
                (inner.size.height, size.height)
            } else {
                (inner.size.width, size.width)
            };
            state.set_extent(content, viewport);
            Measured {
                size,
                paint: Paint::Scroll {
                    state: state.clone(),
                    vertical: *vertical,
                    offset: state.drawn_offset(),
                    inner: Box::new(inner),
                },
            }
        }
        Element::Clickable(on_click, feedback, hand) => {
            wrap(next(constraints), |inner| Paint::Clickable {
                on_click: Rc::clone(on_click),
                feedback: *feedback,
                hand: *hand,
                inner,
            })
        }
        Element::FocusRequester(focus) => wrap(next(constraints), |inner| Paint::FocusRequester {
            focus: focus.clone(),
            inner,
        }),
        Element::Semantics(semantics) => wrap(next(constraints), |inner| Paint::Semantics {
            semantics: semantics.clone(),
            inner,
        }),
        Element::Press(on_press, view_configuration) => {
            wrap(next(constraints), |inner| Paint::Press {
                on_press: Rc::clone(on_press),
                view_configuration: *view_configuration,
                inner,
            })
        }
        Element::Button { button, on_event } => wrap(next(constraints), |inner| Paint::Button {
            button: *button,
            on_event: Rc::clone(on_event),
            inner,
        }),
        Element::KeyEvent { on_key, preview } => wrap(next(constraints), |inner| Paint::KeyEvent {
            on_key: Rc::clone(on_key),
            preview: *preview,
            inner,
        }),
        Element::Drag(on_drag) => wrap(next(constraints), |inner| Paint::Drag {
            on_drag: Rc::clone(on_drag),
            inner,
        }),
        Element::Focusable {
            focus,
            on_key,
            takes_press,
            pin,
        } => wrap(next(constraints), |inner| Paint::Focusable {
            focus: focus.clone(),
            on_key: Rc::clone(on_key),
            takes_press: *takes_press,
            pin: pin.clone(),
            inner,
        }),
        Element::FocusChanged(on_focus_changed) => {
            wrap(next(constraints), |inner| Paint::FocusChanged {
                on_focus_changed: Rc::clone(on_focus_changed),
                inner,
            })
        }
        Element::TestTag(tag) => wrap(next(constraints), |inner| Paint::TestTag {
            tag: Rc::clone(tag),
            inner,
        }),
        Element::IntrinsicWidth(size) => {
            let query = match size {
                IntrinsicSize::Min => Query::MinWidth,
                IntrinsicSize::Max => Query::MaxWidth,
            };
            let width = intrinsic_modified(rest, node, query, constraints.max_height, text);
            let width = width.clamp(constraints.min_width, constraints.max_width);
            measure_modified(
                rest,
                node,
                Constraints {
                    min_width: width,
                    max_width: width,
                    ..constraints
                },
                text,
            )
        }
        Element::IntrinsicHeight(size) => {
            let query = match size {
                IntrinsicSize::Min => Query::MinHeight,
                IntrinsicSize::Max => Query::MaxHeight,
            };
            let height = intrinsic_modified(rest, node, query, constraints.max_width, text);
            let height = height.clamp(constraints.min_height, constraints.max_height);
            measure_modified(
                rest,
                node,
                Constraints {
                    min_height: height,
                    max_height: height,
                    ..constraints
                },
                text,
            )
        }
        Element::Layout(place) => {
            let mut measurable = Measurable {
                rest,
                node,
                text,
                measured: None,
            };
            let placement = place(&mut measurable, constraints);
            let inner = measurable.measured.unwrap_or(Measured {
                size: Size::default(),
                paint: Paint::Children(Vec::new()),
            });
            Measured {
                size: constraints.constrain(placement.size),
                paint: Paint::Children(vec![(placement.offset, inner)]),
            }
        }
        Element::Weight { .. } | Element::Align(_) | Element::MatchParentSize => next(constraints),
    }
}

/// The text shaped and broken into lines at `max_width`, laid out from its start side:
/// alignment only matters for text wider than its content.
fn flowing_layout(
    content: &str,
    style: &TextStyle,
    max_width: f32,
    text: &mut dyn TextMeasurer,
) -> Rc<TextLayout> {
    let flow = TextStyle {
        align: TextAlign::Start,
        ..*style
    };
    let spans = [TextSpan::new(content).resolve(&flow)];
    Rc::new(text.layout(&spans, &flow, max_width))
}

/// The text shaped and broken into lines at `max_width`, with where the boundary before
/// each character is, and after the last. Lines end at line breaks and where the text wraps.
fn character_boundaries(
    content: &str,
    style: &TextStyle,
    max_width: f32,
    text: &mut dyn TextMeasurer,
) -> (Rc<TextLayout>, Vec<CharPosition>) {
    let layout = flowing_layout(content, style, max_width, text);
    let boundaries = layout
        .char_positions()
        .into_iter()
        .map(|(x, line)| CharPosition {
            x,
            line: line as u32,
        })
        .collect();
    (layout, boundaries)
}

/// Gives the text layout `layout` makes to every
/// [`on_text_layout`](crate::Modifier::on_text_layout) handler of `node`, making it only when
/// there is one, and keeps the block being measured from being cached so they are told each
/// time.
fn tell_text_layout(node: &Node, layout: impl FnOnce() -> Option<Rc<TextLayout>>) {
    let mut handlers = node
        .modifier
        .elements
        .iter()
        .filter_map(|element| match element {
            Element::TextLayout(handler) => Some(handler),
            _ => None,
        })
        .peekable();
    if handlers.peek().is_none() {
        return;
    }
    mark_unskippable();
    if let Some(layout) = layout() {
        for handler in handlers {
            handler(Rc::clone(&layout));
        }
    }
}

/// What a scope keeps of its last measurement, to reuse it when nothing it depends on changed.
struct LayoutCache {
    constraints: Constraints,
    measurer: usize,
    fonts: u64,
    reads: Reads,
    measured: Rc<Measured>,
}

/// Measures the node a restartable block last produced. The result is cached with the
/// state it read, the blocks inside it, the constraints and the text measurer, and reused
/// while all of them are as they were. Work with effects that cannot be replayed (composing
/// content while measuring, editable text) is never cached.
fn measure_scope(scope: &Scope, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
    let measurer = std::ptr::from_mut(text).cast::<()>() as usize;
    let fonts = rustle_text::font_epoch();
    let hit = scope
        .user_data()
        .borrow()
        .as_ref()
        .and_then(|data| data.downcast_ref::<LayoutCache>())
        .filter(|cache| {
            cache.constraints == constraints
                && cache.measurer == measurer
                && cache.fonts == fonts
                && cache.reads.is_current()
        })
        .map(|cache| (Rc::clone(&cache.measured), cache.reads.clone()));
    if let Some((measured, reads)) = hit {
        reads.merge();
        return Measured {
            size: measured.size,
            paint: Paint::Shared(measured),
        };
    }
    let (measured, reads) = track_reads(|| {
        scope.record_read();
        let node = scope
            .output::<Node>()
            .expect("a restartable block has not run");
        measure_node(&node, constraints, text)
    });
    let measured = Rc::new(measured);
    *scope.user_data().borrow_mut() = reads.is_trackable().then(|| {
        Box::new(LayoutCache {
            constraints,
            measurer,
            fonts,
            reads,
            measured: Rc::clone(&measured),
        }) as Box<dyn std::any::Any>
    });
    Measured {
        size: measured.size,
        paint: Paint::Shared(measured),
    }
}

/// Measures a node's content, retaining owned text and resolved rich spans.
fn measure_content(node: &Node, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
    match &node.content {
        Content::Text {
            content,
            style,
            editing,
        } => {
            let editing = editing.as_ref().map(|editing| {
                mark_unskippable();
                let (layout, boundaries) =
                    character_boundaries(content, style, constraints.max_width, text);
                *editing.boundaries.borrow_mut() = boundaries.clone();
                *editing.layout.borrow_mut() = Some(layout);
                if let Some(on_layout) = &editing.on_layout {
                    on_layout();
                }
                (editing.clone(), boundaries)
            });
            tell_text_layout(node, || match &editing {
                Some((editing, _)) => editing.layout.borrow().clone(),
                None => Some(flowing_layout(content, style, constraints.max_width, text)),
            });
            Measured {
                size: constraints.constrain(text.measure(content, style, constraints.max_width)),
                paint: Paint::Text {
                    content: content.clone(),
                    style: *style,
                    editing,
                },
            }
        }
        Content::RichText {
            spans,
            style,
            editing,
        } => {
            let spans: Vec<StyledSpan> = spans.iter().map(|span| span.resolve(style)).collect();
            let editing = editing.as_ref().map(|editing| {
                mark_unskippable();
                let flow = TextStyle {
                    align: TextAlign::Start,
                    ..*style
                };
                let layout = Rc::new(text.layout(&spans, &flow, constraints.max_width));
                let boundaries = layout
                    .char_positions()
                    .into_iter()
                    .map(|(x, line)| CharPosition {
                        x,
                        line: line as u32,
                    })
                    .collect();
                *editing.boundaries.borrow_mut() = boundaries;
                *editing.layout.borrow_mut() = Some(layout);
                if let Some(on_layout) = &editing.on_layout {
                    on_layout();
                }
                editing.clone()
            });
            tell_text_layout(node, || match &editing {
                Some(editing) => editing.layout.borrow().clone(),
                None => {
                    let flow = TextStyle {
                        align: TextAlign::Start,
                        ..*style
                    };
                    Some(Rc::new(text.layout(&spans, &flow, constraints.max_width)))
                }
            });
            Measured {
                size: constraints.constrain(text.measure_spans(
                    &spans,
                    style,
                    constraints.max_width,
                )),
                paint: Paint::RichText {
                    spans,
                    style: *style,
                    editing,
                },
            }
        }
        Content::Canvas { draw } => Measured {
            size: constraints.constrain(Size::default()),
            paint: Paint::Canvas {
                draw: Rc::clone(draw),
            },
        },
        Content::Scope(scope) => measure_scope(scope, constraints, text),
        Content::Image { image, scale } => {
            let (width, height) = (image.width() as f32, image.height() as f32);
            let shrink = (constraints.max_width / width)
                .min(constraints.max_height / height)
                .min(1.0);
            Measured {
                size: constraints.constrain(Size::new(width * shrink, height * shrink)),
                paint: Paint::Image {
                    image: image.clone(),
                    scale: *scale,
                },
            }
        }
        Content::Layout { policy, children } => {
            let static_count = children.len();
            let mut scope = Children {
                nodes: children,
                extra: Vec::new(),
                measured: children.iter().map(|_| None).collect(),
                text,
            };
            let result = policy.measure(&mut scope, constraints);
            let placed = scope
                .measured
                .into_iter()
                .enumerate()
                .filter_map(|(index, measured)| {
                    let position = result.positions.get(index).copied().unwrap_or_default();
                    match measured {
                        Some(measured) => Some((position, measured)),
                        None if index < static_count => {
                            panic!("layout did not measure every child")
                        }
                        None => None,
                    }
                })
                .collect();
            Measured {
                size: constraints.constrain(result.size),
                paint: Paint::Children(placed),
            }
        }
    }
}
