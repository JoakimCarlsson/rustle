//! Layout: turns a node tree into drawing commands in two steps.
//!
//! Measuring passes [`Constraints`] down and sizes up, applying each node's
//! modifiers and its [`Layout`] policy. Painting then walks the measured tree
//! and emits [`DrawCommand`]s.

use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, DrawCommand};
use rustle_text::{TextMeasurer, TextStyle};
use rustle_unit::Constraints;

use crate::modifier::Element;
use crate::{Content, Node};

/// Decides how a node sizes and places its children.
pub trait Layout {
    /// Measures the children through `children` and returns the node's size
    /// and where each child goes, relative to the node's top-left corner.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult;
}

/// The outcome of a [`Layout`]: the node's size and one position per child.
pub struct LayoutResult {
    pub size: Size,
    pub positions: Vec<Offset>,
}

/// The children of the node being measured.
pub struct Children<'a> {
    nodes: &'a [Node],
    measured: Vec<Option<Measured>>,
    text: &'a mut dyn TextMeasurer,
}

impl Children<'_> {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Measures child `index` within `constraints` and returns its size.
    /// Measuring a child again replaces the earlier result.
    pub fn measure(&mut self, index: usize, constraints: Constraints) -> Size {
        let measured = measure_node(&self.nodes[index], constraints, self.text);
        let size = measured.size;
        self.measured[index] = Some(measured);
        size
    }
}

/// Places every child at the top-left corner, on top of each other.
///
/// Also the layout of a root that emits several elements.
pub struct Overlay;

impl Layout for Overlay {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            ..constraints
        };
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, child_constraints);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        LayoutResult {
            size,
            positions: vec![Offset::default(); children.len()],
        }
    }
}

struct Measured {
    size: Size,
    paint: Paint,
}

enum Paint {
    Clickable {
        on_click: Rc<dyn Fn()>,
        inner: Box<Measured>,
    },
    Text {
        content: String,
        color: Color,
        style: TextStyle,
    },
    Children(Vec<(Offset, Measured)>),
    Background {
        color: Color,
        corner_radius: f32,
        inner: Box<Measured>,
    },
}

/// The result of laying out a tree: what to draw and what can be clicked.
#[derive(Default)]
pub struct DisplayList {
    /// Drawing commands in paint order.
    pub commands: Vec<DrawCommand>,
    /// Clickable areas in paint order, so later ones are on top.
    pub regions: Vec<HitRegion>,
}

/// An area that runs a handler when clicked.
pub struct HitRegion {
    pub rect: Rect,
    pub on_click: Rc<dyn Fn()>,
}

impl DisplayList {
    /// The topmost clickable region containing `point`.
    pub fn hit_test(&self, point: Offset) -> Option<&HitRegion> {
        self.regions.iter().rev().find(|region| {
            let Rect { origin, size } = region.rect;
            point.x >= origin.x
                && point.x < origin.x + size.width
                && point.y >= origin.y
                && point.y < origin.y + size.height
        })
    }
}

/// Lays out `root` within `constraints`.
pub fn display_list(
    root: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> DisplayList {
    let measured = measure_node(root, constraints, text);
    let mut list = DisplayList::default();
    paint(&measured, Offset::default(), &mut list);
    list
}

fn measure_node(node: &Node, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
    measure_modified(&node.modifier.elements, node, constraints, text)
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
    match *element {
        Element::Padding {
            left,
            top,
            right,
            bottom,
        } => {
            let inner = measure_modified(
                rest,
                node,
                constraints.deflate(left + right, top + bottom),
                text,
            );
            let size = constraints.constrain(Size::new(
                inner.size.width + left + right,
                inner.size.height + top + bottom,
            ));
            Measured {
                size,
                paint: Paint::Children(vec![(Offset::new(left, top), inner)]),
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
            measure_modified(rest, node, tightened, text)
        }
        Element::FillMax { width, height } => {
            let mut filled = constraints;
            if width && constraints.max_width.is_finite() {
                filled.min_width = constraints.max_width;
            }
            if height && constraints.max_height.is_finite() {
                filled.min_height = constraints.max_height;
            }
            measure_modified(rest, node, filled, text)
        }
        Element::Clickable(ref on_click) => {
            let inner = measure_modified(rest, node, constraints, text);
            Measured {
                size: inner.size,
                paint: Paint::Clickable {
                    on_click: Rc::clone(on_click),
                    inner: Box::new(inner),
                },
            }
        }
        Element::Background {
            color,
            corner_radius,
        } => {
            let inner = measure_modified(rest, node, constraints, text);
            Measured {
                size: inner.size,
                paint: Paint::Background {
                    color,
                    corner_radius,
                    inner: Box::new(inner),
                },
            }
        }
    }
}

fn measure_content(node: &Node, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
    match &node.content {
        Content::Text {
            content,
            color,
            style,
        } => Measured {
            size: constraints.constrain(text.measure(content, style)),
            paint: Paint::Text {
                content: content.clone(),
                color: *color,
                style: *style,
            },
        },
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

fn paint(measured: &Measured, origin: Offset, list: &mut DisplayList) {
    match &measured.paint {
        Paint::Text {
            content,
            color,
            style,
        } => list.commands.push(DrawCommand::Text {
            x: origin.x,
            y: origin.y,
            content: content.clone(),
            color: *color,
            font_size: style.font_size,
            line_height: style.line_height,
        }),
        Paint::Children(children) => {
            for (offset, child) in children {
                paint(
                    child,
                    Offset::new(origin.x + offset.x, origin.y + offset.y),
                    list,
                );
            }
        }
        Paint::Background {
            color,
            corner_radius,
            inner,
        } => {
            list.commands.push(DrawCommand::Rect {
                rect: Rect::new(origin, measured.size),
                color: *color,
                corner_radius: *corner_radius,
            });
            paint(inner, origin, list);
        }
        Paint::Clickable { on_click, inner } => {
            list.regions.push(HitRegion {
                rect: Rect::new(origin, measured.size),
                on_click: Rc::clone(on_click),
            });
            paint(inner, origin, list);
        }
    }
}
