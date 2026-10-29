//! Layout: turns a node tree into drawing commands in two steps.
//!
//! Measuring passes [`Constraints`] down and sizes up, applying each node's
//! modifiers and its [`Layout`] policy. Painting then walks the measured tree
//! and emits [`DrawCommand`]s, clickable regions and scrolling regions.

use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, Color, DrawCommand, ImageData};
use rustle_text::{StyledSpan, TextMeasurer, TextStyle};
use rustle_unit::Constraints;

use crate::input::{DropHandler, Focus, KeyHandler, PressEvent};
use crate::modifier::Element;
use crate::scroll::ScrollState;
use crate::semantics::{Role, Semantics};
use crate::{CharPosition, Content, Node, TextEditing};

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

    /// The `weight` modifier of child `index`, if it has one.
    pub fn weight(&self, index: usize) -> Option<f32> {
        self.nodes[index]
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Weight(weight) => Some(*weight),
                _ => None,
            })
    }

    /// The `align` modifier of child `index`, if it has one.
    pub fn alignment(&self, index: usize) -> Option<BoxAlignment> {
        self.nodes[index]
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Align(alignment) => Some(*alignment),
                _ => None,
            })
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

/// How a picture fills the area it is given when the shapes differ.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ContentScale {
    /// All of the picture, as large as fits, centered.
    #[default]
    Fit,
    /// The picture fills the area, centered, and what overflows is cut off.
    Crop,
    /// The picture is stretched to the area, distorting it.
    Stretch,
}

/// Where a child sits within a stack.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BoxAlignment {
    #[default]
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl BoxAlignment {
    /// How far along each axis the child sits, from 0 (start) to 1 (end).
    fn factors(self) -> (f32, f32) {
        match self {
            Self::TopStart => (0.0, 0.0),
            Self::TopCenter => (0.5, 0.0),
            Self::TopEnd => (1.0, 0.0),
            Self::CenterStart => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::CenterEnd => (1.0, 0.5),
            Self::BottomStart => (0.0, 1.0),
            Self::BottomCenter => (0.5, 1.0),
            Self::BottomEnd => (1.0, 1.0),
        }
    }
}

/// Places every child on top of each other, each at its own alignment or at `alignment`.
///
/// Also the layout of a root that emits several elements.
#[derive(Default)]
pub struct Overlay {
    pub alignment: BoxAlignment,
    /// Mirrors the horizontal alignments.
    pub rtl: bool,
}

impl Layout for Overlay {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            ..constraints
        };
        let mut sizes = Vec::with_capacity(children.len());
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, child_constraints);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
            sizes.push(child);
        }
        // Align against the final size, which may be larger than the content.
        let size = constraints.constrain(size);
        let positions = sizes
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let (mut x, y) = children
                    .alignment(index)
                    .unwrap_or(self.alignment)
                    .factors();
                if self.rtl {
                    x = 1.0 - x;
                }
                Offset::new(
                    (size.width - child.width) * x,
                    (size.height - child.height) * y,
                )
            })
            .collect();
        LayoutResult { size, positions }
    }
}

struct Measured {
    size: Size,
    paint: Paint,
}

enum Paint {
    Canvas {
        draw: Rc<dyn Fn(Size) -> Vec<DrawCommand>>,
    },
    RichText {
        spans: Vec<StyledSpan>,
        style: TextStyle,
    },
    Image {
        image: ImageData,
        scale: ContentScale,
    },
    Text {
        content: String,
        style: TextStyle,
        /// What to draw for editing, with the character boundaries measured.
        editing: Option<(TextEditing, Vec<CharPosition>)>,
    },
    Children(Vec<(Offset, Measured)>),
    Background {
        color: Color,
        corner_radius: f32,
        inner: Box<Measured>,
    },
    Gradient {
        from: Color,
        to: Color,
        angle: f32,
        corner_radius: f32,
        inner: Box<Measured>,
    },
    ZIndex {
        z_index: i32,
        inner: Box<Measured>,
    },
    Ring {
        color: Color,
        thickness: f32,
        start: f32,
        sweep: f32,
        inner: Box<Measured>,
    },
    Hover {
        on_hover: Rc<dyn Fn(bool)>,
        inner: Box<Measured>,
    },
    Placed {
        on_placed: Rc<dyn Fn(Rect, Size)>,
        inner: Box<Measured>,
    },
    LongPress {
        on_long_press: Rc<dyn Fn()>,
        inner: Box<Measured>,
    },
    PointerMove {
        on_move: Rc<dyn Fn(Option<Offset>)>,
        inner: Box<Measured>,
    },
    FileDrop {
        on_drop: Rc<dyn Fn(std::path::PathBuf)>,
        inner: Box<Measured>,
    },
    DragEnd {
        on_end: Rc<dyn Fn()>,
        inner: Box<Measured>,
    },
    DragSource {
        payload: Rc<dyn std::any::Any>,
        inner: Box<Measured>,
    },
    DropTarget {
        on_drop: DropHandler,
        inner: Box<Measured>,
    },
    Pinch {
        on_pinch: Rc<dyn Fn(f32)>,
        inner: Box<Measured>,
    },
    Alpha {
        alpha: f32,
        inner: Box<Measured>,
    },
    GraphicsLayer {
        alpha: f32,
        rotation: f32,
        scale: f32,
        inner: Box<Measured>,
    },
    BorderBottom {
        width: f32,
        color: Color,
        inner: Box<Measured>,
    },
    BorderSide {
        width: f32,
        color: Color,
        inner: Box<Measured>,
    },
    Border {
        width: f32,
        color: Color,
        corner_radius: f32,
        inner: Box<Measured>,
    },
    Shadow {
        elevation: f32,
        corner_radius: f32,
        inner: Box<Measured>,
    },
    Clip(Box<Measured>),
    Scroll {
        state: ScrollState,
        vertical: bool,
        offset: f32,
        inner: Box<Measured>,
    },
    Clickable {
        on_click: Rc<dyn Fn()>,
        inner: Box<Measured>,
    },
    Semantics {
        semantics: Semantics,
        inner: Box<Measured>,
    },
    Press {
        on_press: Rc<dyn Fn(PressEvent)>,
        inner: Box<Measured>,
    },
    Drag {
        on_drag: Rc<dyn Fn(Offset)>,
        inner: Box<Measured>,
    },
    Focusable {
        focus: Focus,
        on_key: KeyHandler,
        inner: Box<Measured>,
    },
}

/// The result of laying out a tree: what to draw, what can be clicked and what scrolls.
#[derive(Default)]
pub struct DisplayList {
    /// Drawing commands in paint order.
    pub commands: Vec<DrawCommand>,
    /// Clickable areas in paint order, so later ones are on top.
    pub regions: Vec<HitRegion>,
    /// Scrolling areas in paint order, so later ones are inside earlier ones.
    pub scrolls: Vec<ScrollRegion>,
    /// Areas that take keyboard focus, in paint order, which is also tab order.
    pub focusables: Vec<FocusRegion>,
    /// Areas that react to a press, in paint order.
    pub presses: Vec<PressRegion>,
    /// Areas that follow a drag, in paint order.
    pub drags: Vec<DragRegion>,
    /// Areas that want to know when the pointer is over them, in paint order.
    pub hovers: Vec<HoverRegion>,
    /// Areas that react to a long press, in paint order.
    pub long_presses: Vec<LongPressRegion>,
    /// Areas that follow the pointer over them, in paint order.
    pub pointer_moves: Vec<PointerMoveRegion>,
    /// Areas that can be picked up and dragged elsewhere, in paint order.
    pub drag_sources: Vec<DragSourceRegion>,
    /// Areas something dragged can be dropped on, in paint order.
    pub drop_targets: Vec<DropTargetRegion>,
    /// Areas files can be dropped on, in paint order.
    pub file_drops: Vec<FileDropRegion>,
    /// Areas that react to a pinch gesture, in paint order.
    pub pinches: Vec<PinchRegion>,
    /// What the areas mean, in paint order, for assistive technology.
    pub semantics: Vec<SemanticsNode>,
}

/// What an area means, where it is, and what activating it does.
pub struct SemanticsNode {
    pub rect: Rect,
    pub semantics: Semantics,
    /// Runs when assistive technology activates the area.
    pub on_click: Option<Rc<dyn Fn()>>,
}

/// An area that can hold keyboard focus, cut to what is visible.
pub struct FocusRegion {
    pub rect: Rect,
    pub focus: Focus,
    pub on_key: KeyHandler,
}

/// An area that reacts to a press, cut to what is visible.
pub struct PressRegion {
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_press: Rc<dyn Fn(PressEvent)>,
}

/// An area that can be picked up, cut to what is visible.
pub struct DragSourceRegion {
    pub rect: Rect,
    pub payload: Rc<dyn std::any::Any>,
}

/// An area something dragged can be dropped on, cut to what is visible.
pub struct DropTargetRegion {
    pub rect: Rect,
    pub on_drop: DropHandler,
}

/// An area files can be dropped on, cut to what is visible.
pub struct FileDropRegion {
    pub rect: Rect,
    pub on_drop: Rc<dyn Fn(std::path::PathBuf)>,
}

/// An area that reacts to a pinch gesture, cut to what is visible.
pub struct PinchRegion {
    pub rect: Rect,
    pub on_pinch: Rc<dyn Fn(f32)>,
}

/// An area that follows the pointer over it, cut to what is visible.
pub struct PointerMoveRegion {
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_move: Rc<dyn Fn(Option<Offset>)>,
}

/// An area that reacts to a long press, cut to what is visible.
pub struct LongPressRegion {
    pub rect: Rect,
    pub on_long_press: Rc<dyn Fn()>,
}

/// An area that wants to know when the pointer is over it, cut to what is visible.
pub struct HoverRegion {
    pub rect: Rect,
    pub on_hover: Rc<dyn Fn(bool)>,
}

/// An area that follows a drag, cut to what is visible.
pub struct DragRegion {
    /// What to run when this drag ends, if the area said.
    pub on_end: Option<Rc<dyn Fn()>>,
    pub rect: Rect,
    /// Where the area's top-left corner is, before it was cut to what is visible.
    pub origin: Offset,
    pub on_drag: Rc<dyn Fn(Offset)>,
}

/// An area that runs a handler when clicked, cut to what is visible.
pub struct HitRegion {
    pub rect: Rect,
    /// The corner radius of the background the region sits on, for drawing feedback.
    pub corner_radius: f32,
    pub on_click: Rc<dyn Fn()>,
}

/// An area that scrolls, cut to what is visible.
pub struct ScrollRegion {
    pub rect: Rect,
    pub state: ScrollState,
    pub vertical: bool,
}

fn contains(rect: Rect, point: Offset) -> bool {
    let Rect { origin, size } = rect;
    point.x >= origin.x
        && point.x < origin.x + size.width
        && point.y >= origin.y
        && point.y < origin.y + size.height
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let left = a.origin.x.max(b.origin.x);
    let top = a.origin.y.max(b.origin.y);
    let right = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let bottom = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    Rect::new(
        Offset::new(left, top),
        Size::new((right - left).max(0.0), (bottom - top).max(0.0)),
    )
}

impl DisplayList {
    /// The topmost clickable region containing `point`.
    pub fn hit_test(&self, point: Offset) -> Option<&HitRegion> {
        self.regions
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost focusable region containing `point`.
    pub fn focus_at(&self, point: Offset) -> Option<&FocusRegion> {
        self.focusables
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost draggable region containing `point`.
    pub fn drag_source_at(&self, point: Offset) -> Option<&DragSourceRegion> {
        self.drag_sources
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost drop target containing `point`.
    pub fn drop_target_at(&self, point: Offset) -> Option<&DropTargetRegion> {
        self.drop_targets
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost file-drop region containing `point`.
    pub fn file_drop_at(&self, point: Offset) -> Option<&FileDropRegion> {
        self.file_drops
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost pinch region containing `point`.
    pub fn pinch_at(&self, point: Offset) -> Option<&PinchRegion> {
        self.pinches
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost long-press region containing `point`.
    pub fn long_press_at(&self, point: Offset) -> Option<&LongPressRegion> {
        self.long_presses
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// Every hover region containing `point`.
    pub fn hovers_at(&self, point: Offset) -> impl Iterator<Item = &HoverRegion> {
        self.hovers
            .iter()
            .filter(move |region| contains(region.rect, point))
    }

    /// The topmost drag region containing `point`.
    pub fn drag_at(&self, point: Offset) -> Option<&DragRegion> {
        self.drags
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The topmost press region containing `point`.
    pub fn press_at(&self, point: Offset) -> Option<&PressRegion> {
        self.presses
            .iter()
            .rev()
            .find(|region| contains(region.rect, point))
    }

    /// The innermost region containing `point` that scrolls along the given axis.
    pub fn scroll_at(&self, point: Offset, vertical: bool) -> Option<&ScrollRegion> {
        self.scrolls
            .iter()
            .rev()
            .find(|region| region.vertical == vertical && contains(region.rect, point))
    }
}

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

fn measure_node(node: &Node, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
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
        Element::Clickable(on_click) => wrap(next(constraints), |inner| Paint::Clickable {
            on_click: Rc::clone(on_click),
            inner,
        }),
        Element::Semantics(semantics) => wrap(next(constraints), |inner| Paint::Semantics {
            semantics: semantics.clone(),
            inner,
        }),
        Element::Press(on_press) => wrap(next(constraints), |inner| Paint::Press {
            on_press: Rc::clone(on_press),
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

/// Where the boundary before each character is, and after the last. Lines
/// end at line breaks.
fn character_boundaries(
    content: &str,
    style: &TextStyle,
    text: &mut dyn TextMeasurer,
) -> Vec<CharPosition> {
    let mut boundaries = vec![CharPosition { x: 0.0, line: 0 }];
    let (mut line, mut prefix) = (0_u32, String::new());
    for character in content.chars() {
        if character == '\n' {
            line += 1;
            prefix.clear();
            boundaries.push(CharPosition { x: 0.0, line });
        } else {
            prefix.push(character);
            boundaries.push(CharPosition {
                x: text.measure(&prefix, style, f32::INFINITY).width,
                line,
            });
        }
    }
    boundaries
}

fn measure_content(node: &Node, constraints: Constraints, text: &mut dyn TextMeasurer) -> Measured {
    match &node.content {
        Content::Text {
            content,
            style,
            editing,
        } => {
            let editing = editing.as_ref().map(|editing| {
                let boundaries = character_boundaries(content, style, text);
                *editing.boundaries.borrow_mut() = boundaries.clone();
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
        Content::RichText { spans, style } => {
            let spans: Vec<StyledSpan> = spans.iter().map(|span| span.resolve(style)).collect();
            Measured {
                size: constraints.constrain(text.measure_spans(
                    &spans,
                    style,
                    constraints.max_width,
                )),
                paint: Paint::RichText {
                    spans,
                    style: *style,
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
                self.list.drag_sources.push(DragSourceRegion {
                    rect: self.visible(rect),
                    payload: Rc::clone(payload),
                });
                self.paint(inner, origin);
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
            Paint::Clickable { on_click, inner } => {
                let corner_radius = match &inner.paint {
                    Paint::Background { corner_radius, .. } => *corner_radius,
                    _ => 0.0,
                };
                self.list.regions.push(HitRegion {
                    rect: self.visible(rect),
                    corner_radius,
                    on_click: Rc::clone(on_click),
                });
                self.paint(inner, origin);
            }
        }
    }
}
