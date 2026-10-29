use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Color, DrawCommand, ImageData};
use rustle_text::{StyledSpan, TextStyle};

use super::ContentScale;
use crate::input::{DropHandler, Focus, KeyHandler, PressEvent};
use crate::scroll::ScrollState;
use crate::semantics::Semantics;
use crate::{CharPosition, TextEditing};

pub(super) struct Measured {
    pub(super) size: Size,
    pub(super) paint: Paint,
}

pub(super) enum Paint {
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
        feedback: bool,
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
