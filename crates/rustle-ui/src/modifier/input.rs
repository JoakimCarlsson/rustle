use std::path::PathBuf;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};

use super::{Element, Modifier};
use crate::cursor::CursorIcon;
use crate::input::{Focus, PointerButton, PointerEvent, PointerPhase, PressEvent};
use crate::semantics::Semantics;

impl Modifier {
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

    /// Shows `icon` as the mouse cursor while the pointer is over the area, unless
    /// something painted later inside or above it asks for another one. Without it,
    /// clickable areas show a pointing hand and everything else the arrow.
    pub fn cursor(self, icon: CursorIcon) -> Self {
        self.then(Element::Cursor(icon))
    }

    /// Calls `on_hover` with true when the pointer enters the area and with false when it leaves.
    pub fn on_hover(self, on_hover: impl Fn(bool) + 'static) -> Self {
        self.then(Element::Hover(Rc::new(on_hover)))
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

    /// Runs `on_event` for each press, drag, release and click of `button` on the area,
    /// with the position and the modifier keys held. Other buttons are not affected: an
    /// area that reacts to the secondary button does not stop a parent's primary
    /// `clickable` from seeing the primary button, and the topmost area reacting to
    /// `button` is the one that gets it.
    pub fn on_pointer_button(
        self,
        button: PointerButton,
        on_event: impl Fn(&PointerEvent) + 'static,
    ) -> Self {
        self.then(Element::Button {
            button,
            on_event: Rc::new(on_event),
        })
    }

    /// Runs `on_click` when `button` is pressed and released on the area, with the
    /// position and modifiers at the release.
    pub fn on_button_click(
        self,
        button: PointerButton,
        on_click: impl Fn(&PointerEvent) + 'static,
    ) -> Self {
        self.on_pointer_button(button, move |event| {
            if event.phase == PointerPhase::Click {
                on_click(event);
            }
        })
    }

    /// Runs `on_click` for a primary-button click with the position and the modifier
    /// keys held, such as Ctrl+click. Does not replace [`clickable`](Self::clickable).
    pub fn on_primary_click(self, on_click: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_button_click(PointerButton::Primary, on_click)
    }

    /// Runs `on_click` when the secondary (right) button is pressed and released on the area.
    pub fn on_secondary_click(self, on_click: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_button_click(PointerButton::Secondary, on_click)
    }

    /// Runs `on_menu` when a context menu is asked for on the area: a secondary click.
    /// The event's `window_position` is where to put the menu.
    pub fn on_context_menu(self, on_menu: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_secondary_click(on_menu)
    }

    /// Runs `on_click` when the middle (wheel) button is pressed and released on the area.
    pub fn on_middle_click(self, on_click: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_button_click(PointerButton::Middle, on_click)
    }

    /// Runs `on_click` when the back thumb button is pressed and released on the area.
    pub fn on_back_click(self, on_click: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_button_click(PointerButton::Back, on_click)
    }

    /// Runs `on_click` when the forward thumb button is pressed and released on the area.
    pub fn on_forward_click(self, on_click: impl Fn(&PointerEvent) + 'static) -> Self {
        self.on_button_click(PointerButton::Forward, on_click)
    }

    /// Runs `on_drag` with the position relative to the area when `button` goes down on
    /// it and each time the pointer moves while it stays down, even outside the area.
    pub fn on_button_drag(
        self,
        button: PointerButton,
        on_drag: impl Fn(&PointerEvent) + 'static,
    ) -> Self {
        self.on_pointer_button(button, move |event| {
            if matches!(event.phase, PointerPhase::Press | PointerPhase::Drag) {
                on_drag(event);
            }
        })
    }

    /// Sees key events that happen inside the area, after the focused area inside it had
    /// its turn and did not handle the event: the event bubbles up from the focused area
    /// through every `on_key_event` around it, innermost first, until one returns true.
    /// With nothing focused, every `on_key_event` in the window sees it, topmost-painted
    /// first. Unhandled Tab and Escape still do their default afterwards.
    ///
    /// The area includes the focusable areas inside it, so put this before (outside)
    /// [`focusable`](Self::focusable) if both are on the same element. Match the event
    /// with [`KeyEvent::is`](crate::KeyEvent::is) or [`KeyEvent::is_code`](crate::KeyEvent::is_code).
    pub fn on_key_event(self, on_key: impl Fn(&crate::KeyEvent) -> bool + 'static) -> Self {
        self.then(Element::KeyEvent {
            on_key: Rc::new(on_key),
            preview: false,
        })
    }

    /// Like [`on_key_event`](Self::on_key_event), but sees the event before the focused
    /// area does, outermost first, so it can take shortcuts a text field would otherwise
    /// use. Returning true stops the event. Releases and repeats are delivered too.
    pub fn on_preview_key_event(self, on_key: impl Fn(&crate::KeyEvent) -> bool + 'static) -> Self {
        self.then(Element::KeyEvent {
            on_key: Rc::new(on_key),
            preview: true,
        })
    }

    /// Runs `on_click` when the area is pressed and released with the primary button.
    ///
    /// The area is whatever the modifiers before this one add up to, so
    /// `padding` before `clickable` makes the padding clickable too.
    pub fn clickable(self, on_click: impl Fn() + 'static) -> Self {
        self.then(Element::Clickable(Rc::new(on_click), true))
    }

    /// Like [`clickable`](Self::clickable), without the hover and press tint.
    ///
    /// For areas that only take clicks so they do not reach what is behind,
    /// such as a dialog's backdrop.
    pub fn clickable_plain(self, on_click: impl Fn() + 'static) -> Self {
        self.then(Element::Clickable(Rc::new(on_click), false))
    }
}
