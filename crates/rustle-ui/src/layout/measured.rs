use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, DrawCommand, ImageData, LayerEffect};
use rustle_text::{StyledSpan, TextStyle};

use super::ContentScale;
use crate::cursor::CursorIcon;
use crate::gesture::PointerInputHandler;
use crate::input::{DropHandler, Focus, KeyHandler, PointerButton, PointerEvent, PressEvent};
use crate::modifier::{DrawFn, Fill, FocusedFn, InteractiveDrawFn};
use crate::scroll::ScrollState;
use crate::semantics::Semantics;
use crate::{CharPosition, TextEditing};

/// A transient layout result retained until painting finishes.
pub(super) struct Measured {
    pub(super) size: Size,
    pub(super) paint: Paint,
}

/// Painting data retained until normal and deferred painting finish.
/// Rich spans are taken once by the painter and transferred into the display list.
pub(super) enum Paint {
    Canvas {
        draw: Rc<dyn Fn(Size) -> Vec<DrawCommand>>,
    },
    RichText {
        spans: Vec<StyledSpan>,
        style: TextStyle,
        editing: Option<TextEditing>,
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
    /// A measured subtree kept from an earlier frame, shared with the cache of its scope.
    Shared(Rc<Measured>),
    Background {
        color: Color,
        corner_radius: f32,
        inner: Box<Measured>,
    },
    Draw {
        draw: DrawFn,
        inner: Box<Measured>,
    },
    Interactive {
        draw: InteractiveDrawFn,
        inner: Box<Measured>,
    },
    InteractionFocused {
        focused: FocusedFn,
        inner: Box<Measured>,
    },
    InteractionGroup {
        inner: Box<Measured>,
    },
    KeepInWindow {
        inner: Box<Measured>,
    },
    InWindow {
        at: Offset,
        inner: Box<Measured>,
    },
    /// A popup, painted where its provider puts it against the anchor it is declared in.
    Popup {
        placement: crate::popup::PopupPlacement,
        inner: Box<Measured>,
    },
    Fill {
        fill: Fill,
        inner: Box<Measured>,
    },
    Effect {
        effect: LayerEffect,
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
        /// Whether the clips around the area also cut it.
        clipped: bool,
        inner: Box<Measured>,
    },
    Ring {
        color: Color,
        thickness: f32,
        start: f32,
        sweep: f32,
        inner: Box<Measured>,
    },
    Cursor {
        icon: CursorIcon,
        inner: Box<Measured>,
    },
    ImeCaret {
        rect: Rect,
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
    PlacedVisible {
        on_visible: Rc<dyn Fn(Rect)>,
        inner: Box<Measured>,
    },
    LongPress {
        on_long_press: Rc<dyn Fn()>,
        view_configuration: crate::ViewConfiguration,
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
        button: PointerButton,
        touch_slop: f32,
        inner: Box<Measured>,
    },
    DropTarget {
        on_drop: DropHandler,
        inner: Box<Measured>,
    },
    DragAndDropSource {
        transfer_data: crate::drag_and_drop::TransferDataFn,
        touch_slop: f32,
        inner: Box<Measured>,
    },
    DragAndDropTarget {
        should_start: crate::drag_and_drop::ShouldStartFn,
        target: Rc<dyn crate::drag_and_drop::DragAndDropTarget>,
        inner: Box<Measured>,
    },
    PointerInput {
        on_event: PointerInputHandler,
        inner: Box<Measured>,
    },
    Pinch {
        on_pinch: Rc<dyn Fn(f32)>,
        inner: Box<Measured>,
    },
    BlocksPointer(Box<Measured>),
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
    /// Content drawn as one layer turned by the angle, in degrees, `degrees` gives when it is
    /// painted.
    RotateWith {
        degrees: Rc<dyn Fn() -> f32>,
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
    ClipRounded {
        radii: [f32; 4],
        inner: Box<Measured>,
    },
    Scroll {
        state: ScrollState,
        vertical: bool,
        offset: f32,
        inner: Box<Measured>,
    },
    Clickable {
        on_click: Rc<dyn Fn()>,
        feedback: bool,
        inner: Box<Measured>,
    },
    Semantics {
        semantics: Semantics,
        inner: Box<Measured>,
    },
    Press {
        on_press: Rc<dyn Fn(PressEvent)>,
        view_configuration: crate::ViewConfiguration,
        inner: Box<Measured>,
    },
    Button {
        button: PointerButton,
        on_event: Rc<dyn Fn(&PointerEvent)>,
        inner: Box<Measured>,
    },
    KeyEvent {
        on_key: KeyHandler,
        preview: bool,
        inner: Box<Measured>,
    },
    Drag {
        on_drag: Rc<dyn Fn(Offset)>,
        inner: Box<Measured>,
    },
    Focusable {
        focus: Focus,
        on_key: KeyHandler,
        takes_press: bool,
        inner: Box<Measured>,
    },
    FocusChanged {
        on_focus_changed: Rc<dyn Fn(bool)>,
        inner: Box<Measured>,
    },
    TestTag {
        tag: Rc<str>,
        inner: Box<Measured>,
    },
}
