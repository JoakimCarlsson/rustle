use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::DrawCommand;

use crate::input::{DropHandler, Focus, KeyHandler, PressEvent};
use crate::scroll::ScrollState;
use crate::semantics::Semantics;

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
    /// The drawing commands of the area, as indices into [`DisplayList::commands`],
    /// so what is being dragged can be drawn again under the pointer.
    pub commands: std::ops::Range<usize>,
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
    /// How many clickable regions were painted when this area and what is inside it had been.
    pub covered_after: usize,
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
    /// How many clickable regions were painted when this area and what is inside it had been.
    pub covered_after: usize,
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
    /// Whether the runner tints the region while hovered or pressed.
    pub feedback: bool,
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

pub(super) fn intersect(a: Rect, b: Rect) -> Rect {
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

    /// Every hover region containing `point` that nothing painted later covers.
    pub fn hovers_at(&self, point: Offset) -> impl Iterator<Item = &HoverRegion> {
        self.hovers.iter().filter(move |region| {
            contains(region.rect, point) && !self.covered_at(region.covered_after, point)
        })
    }

    /// Every pointer-follow region containing `point` that nothing painted later covers.
    pub fn pointer_moves_at(&self, point: Offset) -> impl Iterator<Item = &PointerMoveRegion> {
        self.pointer_moves.iter().filter(move |region| {
            contains(region.rect, point) && !self.covered_at(region.covered_after, point)
        })
    }

    /// Whether a clickable region painted after the first `painted` ones contains `point`,
    /// such as a dialog over the page, which keeps the pointer from reaching what is beneath.
    fn covered_at(&self, painted: usize, point: Offset) -> bool {
        self.regions
            .iter()
            .skip(painted)
            .any(|region| contains(region.rect, point))
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
