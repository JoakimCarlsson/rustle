//! Modifiers: an ordered chain of changes to how an element is sized, drawn and laid out.

use std::path::PathBuf;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::Color;
use rustle_unit::Dp;

use crate::input::{DropHandler, Focus, KeyHandler, PressEvent};
use crate::layout::BoxAlignment;
use crate::scroll::ScrollState;
use crate::semantics::Semantics;

#[derive(Clone)]
pub(crate) enum Element {
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
    Ring {
        color: Color,
        thickness: f32,
        start: f32,
        sweep: f32,
    },
    Hover(Rc<dyn Fn(bool)>),
    Placed(Rc<dyn Fn(Rect, Size)>),
    SizeChanged(Rc<dyn Fn(Size)>),
    LongPress(Rc<dyn Fn()>),
    PointerMove(Rc<dyn Fn(Option<Offset>)>),
    FileDrop(Rc<dyn Fn(PathBuf)>),
    DragEnd(Rc<dyn Fn()>),
    DragSource(Rc<dyn std::any::Any>),
    DropTarget(DropHandler),
    Pinch(Rc<dyn Fn(f32)>),
    Clip,
    Scroll {
        state: ScrollState,
        vertical: bool,
    },
    Clickable(Rc<dyn Fn()>),
    Semantics(Semantics),
    Press(Rc<dyn Fn(PressEvent)>),
    Drag(Rc<dyn Fn(Offset)>),
    Focusable {
        focus: Focus,
        on_key: KeyHandler,
    },
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

    fn then(mut self, element: Element) -> Self {
        self.elements.push(element);
        self
    }

    /// Appends all of `other` after this chain.
    pub fn chain(mut self, other: Modifier) -> Self {
        self.elements.extend(other.elements);
        self
    }

    /// Adds the same space on all sides.
    pub fn padding(self, all: impl Into<Dp>) -> Self {
        let all = all.into();
        self.padding_xy(all, all)
    }

    /// Adds `horizontal` space left and right and `vertical` space above and below.
    pub fn padding_xy(self, horizontal: impl Into<Dp>, vertical: impl Into<Dp>) -> Self {
        let (horizontal, vertical) = (horizontal.into().0, vertical.into().0);
        self.padding_each(horizontal, vertical, horizontal, vertical)
    }

    /// Adds a different space on each side.
    pub fn padding_each(
        self,
        left: impl Into<Dp>,
        top: impl Into<Dp>,
        right: impl Into<Dp>,
        bottom: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Padding {
            left: left.into().0,
            top: top.into().0,
            right: right.into().0,
            bottom: bottom.into().0,
        })
    }

    /// Adds a different space on each side, with `start` and `end` on the left
    /// and right in a left-to-right layout and swapped in a right-to-left one.
    pub fn padding_directional(
        self,
        start: impl Into<Dp>,
        top: impl Into<Dp>,
        end: impl Into<Dp>,
        bottom: impl Into<Dp>,
    ) -> Self {
        if crate::is_rtl() {
            self.padding_each(end, top, start, bottom)
        } else {
            self.padding_each(start, top, end, bottom)
        }
    }

    /// Requests an exact size, within what the parent allows.
    pub fn size(self, width: impl Into<Dp>, height: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: Some(width.into().0),
            height: Some(height.into().0),
        })
    }

    /// Requests an exact width, within what the parent allows.
    pub fn width(self, width: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: Some(width.into().0),
            height: None,
        })
    }

    /// Requests an exact height, within what the parent allows.
    pub fn height(self, height: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: None,
            height: Some(height.into().0),
        })
    }

    /// Bounds the width to at least `min` and at most `max`.
    pub fn width_in(self, min: impl Into<Dp>, max: impl Into<Dp>) -> Self {
        self.then(Element::SizeIn {
            min_width: Some(min.into().0),
            max_width: Some(max.into().0),
            min_height: None,
            max_height: None,
        })
    }

    /// Bounds the height to at least `min` and at most `max`.
    pub fn height_in(self, min: impl Into<Dp>, max: impl Into<Dp>) -> Self {
        self.then(Element::SizeIn {
            min_width: None,
            max_width: None,
            min_height: Some(min.into().0),
            max_height: Some(max.into().0),
        })
    }

    /// Takes all the width the parent offers, when it is bounded.
    pub fn fill_max_width(self) -> Self {
        self.then(Element::FillMax {
            width: true,
            height: false,
        })
    }

    /// Takes all the height the parent offers, when it is bounded.
    pub fn fill_max_height(self) -> Self {
        self.then(Element::FillMax {
            width: false,
            height: true,
        })
    }

    /// Takes all the space the parent offers, when it is bounded.
    pub fn fill_max_size(self) -> Self {
        self.then(Element::FillMax {
            width: true,
            height: true,
        })
    }

    /// Sizes the element to the largest `width / height = ratio` that fits.
    pub fn aspect_ratio(self, ratio: f32) -> Self {
        self.then(Element::AspectRatio(ratio))
    }

    /// Moves the element without changing the space it takes.
    pub fn offset(self, x: impl Into<Dp>, y: impl Into<Dp>) -> Self {
        self.then(Element::Offset {
            x: x.into().0,
            y: y.into().0,
        })
    }

    /// Shares the space left in a row or column with the other weighted
    /// children, in proportion to `weight`. Ignored anywhere else.
    pub fn weight(self, weight: f32) -> Self {
        self.then(Element::Weight(weight))
    }

    /// Places the element within a stack. Ignored anywhere else.
    pub fn align(self, alignment: BoxAlignment) -> Self {
        self.then(Element::Align(alignment))
    }

    /// Fills the area with `color`.
    pub fn background(self, color: Color) -> Self {
        self.background_rounded(color, 0.0)
    }

    /// Fills the area with `color`, with corners rounded to `corner_radius`.
    pub fn background_rounded(self, color: Color, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Background {
            color,
            corner_radius: corner_radius.into().0,
        })
    }

    /// Fills the area with a blend from `from` to `to` along `degrees`, clockwise
    /// from pointing right (90 blends downward), with corners rounded to `corner_radius`.
    pub fn background_gradient(
        self,
        from: Color,
        to: Color,
        degrees: f32,
        corner_radius: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Gradient {
            from,
            to,
            angle: degrees.to_radians(),
            corner_radius: corner_radius.into().0,
        })
    }

    /// Draws the content translucent, from 0 (invisible) to 1 (as is). Each
    /// thing is faded on its own, so overlapping things show through each other.
    pub fn alpha(self, alpha: f32) -> Self {
        self.then(Element::Alpha(alpha.clamp(0.0, 1.0)))
    }

    /// Draws the content as one picture, then puts it on what is below turned
    /// `rotation_degrees` clockwise and scaled by `scale` about the center of the
    /// area, and faded to `alpha`. Unlike [`Modifier::alpha`], overlapping parts
    /// of the content do not show through each other, and text and pictures
    /// turn and scale too. Pointer input still goes where the area was laid out.
    pub fn graphics_layer(self, alpha: f32, rotation_degrees: f32, scale: f32) -> Self {
        self.then(Element::GraphicsLayer {
            alpha: alpha.clamp(0.0, 1.0),
            rotation: rotation_degrees.to_radians(),
            scale,
        })
    }

    /// Turns the content `degrees` clockwise about the center of the area. See [`Modifier::graphics_layer`].
    pub fn rotate(self, degrees: f32) -> Self {
        self.graphics_layer(1.0, degrees, 1.0)
    }

    /// Scales the content by `factor` about the center of the area. See [`Modifier::graphics_layer`].
    pub fn scale(self, factor: f32) -> Self {
        self.graphics_layer(1.0, 0.0, factor)
    }

    /// Draws an outline of `width` inside the area, drawn over the content.
    pub fn border(self, width: impl Into<Dp>, color: Color, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Border {
            width: width.into().0,
            color,
            corner_radius: corner_radius.into().0,
        })
    }

    /// Draws a one-pixel line along the right edge of the area, over the content.
    pub fn border_right_hairline(self, color: Color) -> Self {
        self.then(Element::BorderSide { width: 1.0, color })
    }

    /// Draws a one-pixel line along the bottom edge of the area, over the content.
    pub fn border_bottom_hairline(self, color: Color) -> Self {
        self.border_bottom(1.0, color)
    }

    /// Draws a line of `width` along the bottom edge of the area, over the content.
    pub fn border_bottom(self, width: impl Into<Dp>, color: Color) -> Self {
        self.then(Element::BorderBottom {
            width: width.into().0,
            color,
        })
    }

    /// Casts a soft shadow behind the area; a larger `elevation` is softer and lower.
    pub fn shadow(self, elevation: impl Into<Dp>, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Shadow {
            elevation: elevation.into().0,
            corner_radius: corner_radius.into().0,
        })
    }

    /// Draws a ring segment in the largest circle that fits the area: `sweep`
    /// of the circle (0 to 1) clockwise from `start_degrees` (0 at the top), with round ends.
    pub fn ring(
        self,
        color: Color,
        thickness: impl Into<Dp>,
        start_degrees: f32,
        sweep: f32,
    ) -> Self {
        self.then(Element::Ring {
            color,
            thickness: thickness.into().0,
            start: start_degrees.to_radians(),
            sweep: sweep.clamp(0.0, 1.0),
        })
    }

    /// Runs `on_long_press` when the primary button is held down on the area for
    /// about half a second without moving. The click that would follow is cancelled.
    pub fn on_long_press(self, on_long_press: impl Fn() + 'static) -> Self {
        self.then(Element::LongPress(Rc::new(on_long_press)))
    }

    /// Calls `on_drop` with the path of each file dropped on the area from another application.
    pub fn on_file_drop(self, on_drop: impl Fn(PathBuf) + 'static) -> Self {
        self.then(Element::FileDrop(Rc::new(on_drop)))
    }

    /// Runs `on_end` when the drag handled by the next [`Modifier::on_drag`] after this
    /// one in the chain ends: when the button is released. Put it before `on_drag`.
    pub fn on_drag_end(self, on_end: impl Fn() + 'static) -> Self {
        self.then(Element::DragEnd(Rc::new(on_end)))
    }

    /// Makes the area something that can be picked up and dragged onto a
    /// [`Modifier::drop_target`], carrying `payload`.
    pub fn drag_source(self, payload: impl std::any::Any) -> Self {
        self.then(Element::DragSource(Rc::new(payload)))
    }

    /// Calls `on_drop` with the payload of a [`Modifier::drag_source`] released over
    /// the area. The payload is `dyn Any`: downcast it to the type that was dragged.
    pub fn drop_target(self, on_drop: impl Fn(&dyn std::any::Any) + 'static) -> Self {
        self.then(Element::DropTarget(Rc::new(on_drop)))
    }

    /// Calls `on_pinch` with how much a pinch gesture over the area changed the scale: positive
    /// when the fingers spread, negative when they close, as a fraction of the current scale.
    pub fn on_pinch(self, on_pinch: impl Fn(f32) + 'static) -> Self {
        self.then(Element::Pinch(Rc::new(on_pinch)))
    }

    /// A click, a double click and a long press on the same area.
    pub fn combined_clickable(
        self,
        on_click: impl Fn() + 'static,
        on_double_click: impl Fn() + 'static,
        on_long_press: impl Fn() + 'static,
    ) -> Self {
        self.clickable(on_click)
            .on_double_click(on_double_click)
            .on_long_press(on_long_press)
    }

    /// Calls `on_size_changed` with the size the area was laid out at, each
    /// time layout runs. Not a place to change state that changes the size.
    pub fn on_size_changed(self, on_size_changed: impl Fn(Size) + 'static) -> Self {
        self.then(Element::SizeChanged(Rc::new(on_size_changed)))
    }

    /// Calls `on_placed` with where the area is in the window and how large the
    /// window is, each time the display is built. For popups that have to stay
    /// on screen. Not a place to change state; write to a cell instead.
    pub fn on_placed(self, on_placed: impl Fn(Rect, Size) + 'static) -> Self {
        self.then(Element::Placed(Rc::new(on_placed)))
    }

    /// Calls `on_move` with where the pointer is, relative to the area's top-left
    /// corner, each time it moves over the area, and with `None` when it leaves.
    pub fn on_pointer_move(self, on_move: impl Fn(Option<Offset>) + 'static) -> Self {
        self.then(Element::PointerMove(Rc::new(on_move)))
    }

    /// Calls `on_hover` with true when the pointer enters the area and with false when it leaves.
    pub fn on_hover(self, on_hover: impl Fn(bool) + 'static) -> Self {
        self.then(Element::Hover(Rc::new(on_hover)))
    }

    /// Paints the area over everything with a lower `z_index`, whatever its
    /// place in the tree, and lets it receive input first. The default is 0.
    /// For popups, menus and tooltips that must not be covered by later siblings.
    pub fn z_index(self, z_index: i32) -> Self {
        self.then(Element::ZIndex(z_index))
    }

    /// Cuts off whatever the content draws outside the area. The cut is
    /// rectangular, whatever rounding the background has.
    pub fn clip(self) -> Self {
        self.then(Element::Clip)
    }

    /// Lets the content scroll vertically, however tall it is.
    pub fn vertical_scroll(self, state: &ScrollState) -> Self {
        self.then(Element::Scroll {
            state: state.clone(),
            vertical: true,
        })
    }

    /// Lets the content scroll horizontally, however wide it is.
    pub fn horizontal_scroll(self, state: &ScrollState) -> Self {
        self.then(Element::Scroll {
            state: state.clone(),
            vertical: false,
        })
    }

    /// Describes the area for assistive technology such as screen readers.
    ///
    /// Place it before `clickable` and the other input modifiers so the
    /// description covers the same area and can be activated.
    pub fn semantics(self, semantics: Semantics) -> Self {
        self.then(Element::Semantics(semantics))
    }

    /// Runs `on_press` as soon as the primary button goes down on the area, with
    /// the pointer position relative to the area's top-left corner and how many
    /// presses in a row this is.
    pub fn on_press(self, on_press: impl Fn(PressEvent) + 'static) -> Self {
        self.then(Element::Press(Rc::new(on_press)))
    }

    /// Runs `on_double_click` on the second press in quick succession.
    pub fn on_double_click(self, on_double_click: impl Fn() + 'static) -> Self {
        self.on_press(move |press| {
            if press.click_count == 2 {
                on_double_click();
            }
        })
    }

    /// Runs `on_drag` when the primary button goes down on the area and again
    /// each time the pointer moves while it stays down, even outside the area.
    /// The position is relative to the area's top-left corner.
    pub fn on_drag(self, on_drag: impl Fn(Offset) + 'static) -> Self {
        self.then(Element::Drag(Rc::new(on_drag)))
    }

    /// Makes the area able to hold keyboard focus, which it takes when clicked
    /// or tabbed to. While focused it receives key events through `on_key`,
    /// which returns whether it handled one.
    pub fn focusable(
        self,
        focus: &Focus,
        on_key: impl Fn(&crate::KeyEvent) -> bool + 'static,
    ) -> Self {
        self.then(Element::Focusable {
            focus: focus.clone(),
            on_key: Rc::new(on_key),
        })
    }

    /// Runs `on_click` when the area is pressed and released with the primary button.
    ///
    /// The area is whatever the modifiers before this one add up to, so
    /// `padding` before `clickable` makes the padding clickable too.
    pub fn clickable(self, on_click: impl Fn() + 'static) -> Self {
        self.then(Element::Clickable(Rc::new(on_click)))
    }
}
