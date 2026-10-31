use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_text::{StyledSpan, TextAlign, TextLayout, TextMeasurer, TextSpan, TextStyle};
use rustle_unit::Constraints;

use super::Children;
use super::measured::{Measured, Paint};
use crate::modifier::Element;
use crate::{CharPosition, Content, Node};

pub(super) fn measure_node(
    node: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> Measured {
    measure_modified(&node.modifier.elements, node, constraints, text)
}

fn wrap(inner: Measured, paint: impl FnOnce(Box<Measured>) -> Paint) -> Measured {
    Measured {
        size: inner.size,
        paint: paint(Box::new(inner)),
    }
}

/// Applies the modifier `elements` from the outermost in, then measures the node itself.
fn measure_modified(
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
            if *width && constraints.max_width.is_finite() {
                filled.min_width = constraints.max_width;
            }
            if *height && constraints.max_height.is_finite() {
                filled.min_height = constraints.max_height;
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
        Element::DragSource(payload) => wrap(next(constraints), |inner| Paint::DragSource {
            payload: Rc::clone(payload),
            inner,
        }),
        Element::DropTarget(on_drop) => wrap(next(constraints), |inner| Paint::DropTarget {
            on_drop: Rc::clone(on_drop),
            inner,
        }),
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
        Element::LongPress(on_long_press) => wrap(next(constraints), |inner| Paint::LongPress {
            on_long_press: Rc::clone(on_long_press),
            inner,
        }),
        Element::SizeChanged(on_size_changed) => {
            let inner = next(constraints);
            on_size_changed(inner.size);
            inner
        }
        Element::Cursor(icon) => wrap(next(constraints), |inner| Paint::Cursor {
            icon: *icon,
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
                unbounded.min_height = 0.0;
                unbounded.max_height = f32::INFINITY;
            } else {
                unbounded.min_width = 0.0;
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
                    offset: state.offset().clamp(0.0, state.max_offset()),
                    inner: Box::new(inner),
                },
            }
        }
        Element::Clickable(on_click, feedback) => {
            wrap(next(constraints), |inner| Paint::Clickable {
                on_click: Rc::clone(on_click),
                feedback: *feedback,
                inner,
            })
        }
        Element::Semantics(semantics) => wrap(next(constraints), |inner| Paint::Semantics {
            semantics: semantics.clone(),
            inner,
        }),
        Element::Press(on_press) => wrap(next(constraints), |inner| Paint::Press {
            on_press: Rc::clone(on_press),
            inner,
        }),
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
        Element::Focusable { focus, on_key } => wrap(next(constraints), |inner| Paint::Focusable {
            focus: focus.clone(),
            on_key: Rc::clone(on_key),
            inner,
        }),
        Element::Weight(_) | Element::Align(_) => next(constraints),
    }
}

/// The text shaped and broken into lines at `max_width`, with where the boundary before
/// each character is, and after the last. Lines end at line breaks and where the text wraps.
fn character_boundaries(
    content: &str,
    style: &TextStyle,
    max_width: f32,
    text: &mut dyn TextMeasurer,
) -> (Rc<TextLayout>, Vec<CharPosition>) {
    // The text is laid out from its start side; alignment only matters for text wider than its content.
    let flow = TextStyle {
        align: TextAlign::Start,
        ..*style
    };
    let spans = [TextSpan::new(content).resolve(&flow)];
    let layout = Rc::new(text.layout(&spans, &flow, max_width));
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

fn measure_content(node: &Node, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
    match &node.content {
        Content::Text {
            content,
            style,
            editing,
        } => {
            let editing = editing.as_ref().map(|editing| {
                let (layout, boundaries) =
                    character_boundaries(content, style, constraints.max_width, text);
                *editing.boundaries.borrow_mut() = boundaries.clone();
                *editing.layout.borrow_mut() = Some(layout);
                if let Some(on_layout) = &editing.on_layout {
                    on_layout();
                }
                (editing.clone(), boundaries)
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
            let mut scope = Children {
                nodes: children,
                measured: children.iter().map(|_| None).collect(),
                text,
            };
            let result = policy.measure(&mut scope, constraints);
            let placed = scope
                .measured
                .into_iter()
                .zip(result.positions)
                .map(|(measured, position)| {
                    (
                        position,
                        measured.expect("layout did not measure every child"),
                    )
                })
                .collect();
            Measured {
                size: constraints.constrain(result.size),
                paint: Paint::Children(placed),
            }
        }
    }
}
