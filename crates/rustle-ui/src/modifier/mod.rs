//! Modifiers: an ordered chain of changes to how an element is sized, drawn and laid out.

mod draw;
mod gesture;
mod input;
mod sizing;

use std::path::PathBuf;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, DrawCommand, LayerEffect, ShaderBrush};
use rustle_unit::Constraints;

use crate::cursor::CursorIcon;
use crate::gesture::PointerInputHandler;
use crate::input::{DropHandler, Focus, KeyHandler, PointerButton, PointerEvent, PressEvent};
use crate::layout::{BoxAlignment, IntrinsicSize, Measurable, Placement};
use crate::scroll::ScrollState;
use crate::semantics::Semantics;

/// Draws into a canvas of the given size: the commands, and where in them the content goes.
pub(crate) type DrawFn = Rc<dyn Fn(Size) -> (Vec<DrawCommand>, Option<usize>)>;

/// Says, when painting, whether interactive drawing counts as focused.
pub(crate) type FocusedFn = Rc<dyn Fn() -> bool>;

/// Draws into a canvas of the given size in an interaction state: the commands, and where in
/// them the content goes.
pub(crate) type InteractiveDrawFn =
    Rc<dyn Fn(Size, crate::Interaction) -> (Vec<DrawCommand>, Option<usize>)>;

/// A custom layout: measures the content through a [`Measurable`] and says where it goes.
pub(crate) type LayoutFn = dyn Fn(&mut Measurable<'_>, Constraints) -> Placement;

/// A fill drawn behind the content that needs more than a color.
#[derive(Clone)]
pub(crate) enum Fill {
    Radial {
        from: Color,
        to: Color,
        center: (f32, f32),
        radius: f32,
        corner_radius: f32,
    },
    Sweep {
        from: Color,
        to: Color,
        center: (f32, f32),
        start: f32,
        corner_radius: f32,
    },
    Shader {
        brush: ShaderBrush,
        corner_radius: f32,
    },
    BackdropBlur {
        blur: f32,
        corner_radius: f32,
    },
    DropShadow {
        dx: f32,
        dy: f32,
        blur: f32,
        color: Color,
        corner_radius: f32,
    },
}

#[derive(Clone)]
pub(crate) enum Element {
    Draw(DrawFn),
    /// Drawn according to the interaction state, resolved when painting.
    InteractiveDraw(InteractiveDrawFn),
    /// A focused flag for the interactive drawing inside.
    InteractionFocused(FocusedFn),
    /// An area whose hover the interactive drawing inside reports as `group_hovered`.
    InteractionGroup,
    /// An area moved, when it is painted, as far as it takes to lie inside the window.
    KeepInWindow,
    /// A child of a stack sized to the stack after its other children set the stack's size.
    MatchParentSize,
    /// An area painted with its top left at this point of the window, wherever layout put it.
    InWindow(f32, f32),
    /// An area painted where a popup position provider puts it against the layout it is
    /// declared in, told when it should be dismissed.
    Popup(crate::popup::PopupPlacement),
    Fill(Fill),
    Effect(LayerEffect),
    Padding {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    Size {
        width: Option<f32>,
        height: Option<f32>,
    },
    SizeIn {
        min_width: Option<f32>,
        max_width: Option<f32>,
        min_height: Option<f32>,
        max_height: Option<f32>,
    },
    FillMax {
        width: bool,
        height: bool,
    },
    AspectRatio(f32),
    Offset {
        x: f32,
        y: f32,
    },
    Background {
        color: Color,
        corner_radius: f32,
    },
    Border {
        width: f32,
        color: Color,
        corner_radius: f32,
    },
    Shadow {
        elevation: f32,
        corner_radius: f32,
    },
    Gradient {
        from: Color,
        to: Color,
        angle: f32,
        corner_radius: f32,
    },
    Alpha(f32),
    GraphicsLayer {
        alpha: f32,
        rotation: f32,
        scale: f32,
    },
    /// A layer turned by the angle, in degrees, the closure gives when the area is drawn.
    RotateWith(Rc<dyn Fn() -> f32>),
    BorderBottom {
        width: f32,
        color: Color,
    },
    BorderSide {
        width: f32,
        color: Color,
    },
    ZIndex(i32),
    /// Raised like `ZIndex`, but not cut to the clips around it.
    ZIndexUnclipped(i32),
    Ring {
        color: Color,
        thickness: f32,
        start: f32,
        sweep: f32,
    },
    Hover(Rc<dyn Fn(bool)>),
    Cursor(CursorIcon),
    /// Where text input goes, in the area's coordinates, while its focus is held.
    ImeCaret(Rect),
    Placed(Rc<dyn Fn(Rect, Size)>),
    /// Told the part of the area left visible by the clips around it, each time the display is built.
    PlacedVisible(Rc<dyn Fn(Rect)>),
    SizeChanged(Rc<dyn Fn(Size)>),
    /// Told the shaped text of the node's own text content after layout.
    TextLayout(Rc<dyn Fn(Rc<rustle_text::TextLayout>)>),
    /// What a long press runs, and the view configuration it is recognized by.
    LongPress(Rc<dyn Fn()>, crate::ViewConfiguration),
    PointerMove(Rc<dyn Fn(Option<Offset>)>),
    FileDrop(Rc<dyn Fn(PathBuf)>),
    DragEnd(Rc<dyn Fn()>),
    DragSource {
        payload: Rc<dyn std::any::Any>,
        button: PointerButton,
        /// How far the pointer moves before the area is picked up.
        touch_slop: f32,
    },
    PointerInput(PointerInputHandler),
    DropTarget(DropHandler),
    /// Where a drag and drop starts, what it carries and how far the pointer moves first.
    DragAndDropSource(crate::drag_and_drop::TransferDataFn, f32),
    /// An area that takes part in drags and drops.
    DragAndDropTarget(
        crate::drag_and_drop::ShouldStartFn,
        Rc<dyn crate::drag_and_drop::DragAndDropTarget>,
    ),
    Pinch(Rc<dyn Fn(f32)>),
    BlocksPointer,
    Clip,
    ClipRounded([f32; 4]),
    Scroll {
        state: ScrollState,
        vertical: bool,
    },
    Clickable(Rc<dyn Fn()>, bool),
    Semantics(Semantics),
    /// What a press runs, and the view configuration its presses are counted by.
    Press(Rc<dyn Fn(PressEvent)>, crate::ViewConfiguration),
    Button {
        button: PointerButton,
        on_event: Rc<dyn Fn(&PointerEvent)>,
    },
    KeyEvent {
        on_key: KeyHandler,
        preview: bool,
    },
    Drag(Rc<dyn Fn(Offset)>),
    Focusable {
        focus: Focus,
        on_key: KeyHandler,
        /// Whether a press of the primary button on the area gives it focus.
        takes_press: bool,
    },
    /// Told whether the nearest focus target inside holds focus, each time that changes.
    FocusChanged(Rc<dyn Fn(bool)>),
    /// Names the area for tests that look it up.
    TestTag(Rc<str>),
    IntrinsicWidth(IntrinsicSize),
    IntrinsicHeight(IntrinsicSize),
    Layout(Rc<LayoutFn>),
    /// Read by rows and columns; ignored when measuring the node itself.
    Weight {
        /// The area's part of the space left, against the other weighted children.
        weight: f32,
        /// Whether the area is made exactly as long as its share, not at most that long.
        fill: bool,
    },
    /// Read by stacks; ignored when measuring the node itself.
    Align(BoxAlignment),
}

/// A chain of modifiers, applied from the first call (outermost) to the last.
///
/// Order matters: `padding` before `background` leaves a margin outside the
/// background, and after it pads inside.
#[derive(Clone, Default)]
pub struct Modifier {
    pub(crate) elements: Vec<Element>,
}

impl std::fmt::Debug for Modifier {
    /// How many elements the chain holds, so types that keep a modifier can derive `Debug`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modifier")
            .field("elements", &self.elements.len())
            .finish()
    }
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub(super) fn then(mut self, element: Element) -> Self {
        self.elements.push(element);
        self
    }

    /// Appends all of `other` after this chain.
    pub fn chain(mut self, other: Modifier) -> Self {
        self.elements.extend(other.elements);
        self
    }
}
