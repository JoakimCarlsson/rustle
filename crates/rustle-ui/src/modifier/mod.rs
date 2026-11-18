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
    SizeChanged(Rc<dyn Fn(Size)>),
    LongPress(Rc<dyn Fn()>),
    PointerMove(Rc<dyn Fn(Option<Offset>)>),
    FileDrop(Rc<dyn Fn(PathBuf)>),
    DragEnd(Rc<dyn Fn()>),
    DragSource {
        payload: Rc<dyn std::any::Any>,
        button: PointerButton,
    },
    PointerInput(PointerInputHandler),
    DropTarget(DropHandler),
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
    Press(Rc<dyn Fn(PressEvent)>),
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
    },
    IntrinsicWidth(IntrinsicSize),
    IntrinsicHeight(IntrinsicSize),
    Layout(Rc<LayoutFn>),
    /// Read by rows and columns; ignored when measuring the node itself.
    Weight(f32),
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
