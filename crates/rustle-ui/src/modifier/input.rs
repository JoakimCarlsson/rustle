use std::path::PathBuf;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};

use super::{Element, Modifier};
use crate::cursor::CursorIcon;
use crate::input::{Focus, PointerButton, PointerEvent, PointerPhase, PressEvent};
use crate::semantics::Semantics;

impl Modifier {
    /// Runs `on_long_press` when the primary button is held down on the area without moving
    /// for the [`long_press_timeout_millis`](crate::ViewConfiguration::long_press_timeout_millis)
    /// of the [`LOCAL_VIEW_CONFIGURATION`](crate::LOCAL_VIEW_CONFIGURATION); moving it past the
    /// [`touch_slop`](crate::ViewConfiguration::touch_slop) is not a long press. The click
    /// that would follow is cancelled.
    pub fn on_long_press(self, on_long_press: impl Fn() + 'static) -> Self {
        self.then(Element::LongPress(
            Rc::new(on_long_press),
            crate::view_configuration::current_view_configuration(),
        ))
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
        self.drag_source_button(PointerButton::Primary, payload)
    }

    /// Like [`drag_source`](Self::drag_source), picked up by holding `button` instead of the
    /// primary one, and dropped when that button is released. A secondary-button drag does not
    /// move focus or press anything.
    pub fn drag_source_button(self, button: PointerButton, payload: impl std::any::Any) -> Self {
        self.then(Element::DragSource {
            payload: Rc::new(payload),
            button,
            touch_slop: crate::view_configuration::current_view_configuration().touch_slop,
        })
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

    /// Makes the area count as used by the interface wherever the pointer is over it, even
    /// where nothing in it reacts to the pointer, such as the empty space of a panel.
    ///
    /// A host that shares its window with the interface, and gives the pointer to whatever
    /// the interface does not use, asks [`DisplayList::interactive_at`](crate::DisplayList::interactive_at).
    /// Without this, only the parts of a panel that react to the pointer answer yes, and a
    /// click on its background would go through to what lies beneath. It changes nothing
    /// inside the interface: areas under this one still get their events.
    pub fn blocks_pointer(self) -> Self {
        self.then(Element::BlocksPointer)
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

    /// Calls `on_text_layout` with the shaped text of this node's own text or rich text, each
    /// time layout measures it, at the width the text was given: its lines, glyph positions
    /// and hit testing ([`TextLayout::hit_test`](rustle_text::TextLayout::hit_test)), for
    /// links and selection in text that is not edited. Offsets in it are bytes. Nothing is
    /// called for a node whose content is not text. A block holding such text keeps its
    /// layout as any other does, and when it is reused the handler is told the layout kept with
    /// it again. Not a place to change state; write to a cell instead.
    pub fn on_text_layout(
        self,
        on_text_layout: impl Fn(Rc<rustle_text::TextLayout>) + 'static,
    ) -> Self {
        self.then(Element::TextLayout(Rc::new(on_text_layout)))
    }

    /// Calls `on_placed` with where the area is in the window and how large the
    /// window is, each time the display is built. For popups that have to stay
    /// on screen. Not a place to change state; write to a cell instead.
    pub fn on_placed(self, on_placed: impl Fn(Rect, Size) + 'static) -> Self {
        self.then(Element::Placed(Rc::new(on_placed)))
    }

    /// Calls `on_visible` with the part of the area the clips around it leave visible, in
    /// window coordinates, each time the display is built; an area scrolled or clipped out of
    /// sight is told an empty rectangle. For hit areas an application keeps itself, which must
    /// not catch the pointer where the area cannot be seen. Not a place to change state; write
    /// to a cell instead.
    pub fn on_placed_visible(self, on_visible: impl Fn(Rect) + 'static) -> Self {
        self.then(Element::PlacedVisible(Rc::new(on_visible)))
    }

    /// Calls `on_globally_positioned` with the area's rectangle in the window,
    /// in logical pixels, each time the display is built. Like
    /// [`Modifier::on_placed`] without the window size. Not a place to
    /// change state; write to a cell instead.
    pub fn on_globally_positioned(self, on_positioned: impl Fn(Rect) + 'static) -> Self {
        self.on_placed(move |rect, _| on_positioned(rect))
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

    /// Describes the area for assistive technology and drops every description inside it,
    /// so the area is read as `semantics` alone, whatever it holds.
    pub fn clear_and_set_semantics(self, semantics: Semantics) -> Self {
        self.semantics(semantics.clearing_descendants())
    }

    /// Runs `on_press` as soon as the primary button goes down on the area, with
    /// the pointer position relative to the area's top-left corner and how many
    /// presses in a row this is: presses within the
    /// [`double_tap_timeout_millis`](crate::ViewConfiguration::double_tap_timeout_millis) and
    /// [`touch_slop`](crate::ViewConfiguration::touch_slop) of the
    /// [`LOCAL_VIEW_CONFIGURATION`](crate::LOCAL_VIEW_CONFIGURATION) of each other add up.
    pub fn on_press(self, on_press: impl Fn(PressEvent) + 'static) -> Self {
        self.then(Element::Press(
            Rc::new(on_press),
            crate::view_configuration::current_view_configuration(),
        ))
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
            takes_press: true,
            pin: crate::pinnable::current_pinnable_container(),
        })
    }

    /// Makes the area a focus target, Compose's `Modifier.focusTarget()` with
    /// `focusRequester(focus)`: it can hold keyboard focus when Tab walks to it or `focus` is
    /// requested with [`Focus::request_focus`], and while it does, key events bubble from it
    /// through the [`on_key_event`](Self::on_key_event)s around it. As in Compose, a press of
    /// the pointer does not give it focus: a press on it keeps focus where it is when the area
    /// holds it, and otherwise takes focus away as a press anywhere else does.
    pub fn focus_target(self, focus: &Focus) -> Self {
        self.then(Element::Focusable {
            focus: focus.clone(),
            on_key: Rc::new(|_| false),
            takes_press: false,
            pin: crate::pinnable::current_pinnable_container(),
        })
    }

    /// Calls `on_focus_changed` with whether the area's focus target holds keyboard focus,
    /// each time that changes, Compose's `Modifier.onFocusChanged { it.isFocused }`. The focus
    /// target is the first [`focusable`](Self::focusable) or [`focus_target`](Self::focus_target)
    /// painted inside this modifier: put it before (outside) the one it observes.
    ///
    /// It is told `false` too when the focused area leaves the composition, which takes focus
    /// away, as Compose clears focus from a removed node. It runs while the interface handles
    /// input or applies a focus request; it may change state, which composes again.
    pub fn on_focus_changed(self, on_focus_changed: impl Fn(bool) + 'static) -> Self {
        self.then(Element::FocusChanged(Rc::new(on_focus_changed)))
    }

    /// Names the area `tag` for tests, Compose's `Modifier.testTag`: a test finds where the
    /// area was laid out with [`DisplayList::nodes_with_tag`](crate::DisplayList::nodes_with_tag)
    /// (and `Embedded::node_with_tag` in `rustle`). It changes nothing else.
    pub fn test_tag(self, tag: impl Into<String>) -> Self {
        let tag: String = tag.into();
        self.then(Element::TestTag(Rc::from(tag)))
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

    /// Runs `on_click` when the area is pressed and released with the primary button, or
    /// activated from the keyboard: Compose's `Modifier.clickable(onClick)`.
    ///
    /// As in Compose, the area is focusable: Tab reaches it, while a press of the pointer
    /// does not focus it. While it holds focus, Enter, the keypad's Enter or the space bar
    /// going down and coming up again runs `on_click`; repeats of the held key are left to
    /// the handlers around it. A [`focus_requester`](Self::focus_requester) before this gives
    /// it that focus; otherwise it remembers its own, so call it while composing.
    ///
    /// The area is whatever the modifiers before this one add up to, so
    /// `padding` before `clickable` makes the padding clickable too.
    #[track_caller]
    pub fn clickable(self, on_click: impl Fn() + 'static) -> Self {
        let on_click: Rc<dyn Fn()> = Rc::new(on_click);
        self.clicked_by_keys(Rc::clone(&on_click), None)
            .then(Element::Clickable(on_click, true, true))
    }

    /// Gives the first focus target inside the area `focus` in place of its own, Compose's
    /// `Modifier.focusRequester(focusRequester)`: requesting it with
    /// [`Focus::request_focus`] focuses that target, and the target's focus is read through
    /// it. Put it before the [`clickable`](Self::clickable) or [`focusable`](Self::focusable)
    /// it is for.
    pub fn focus_requester(self, focus: &Focus) -> Self {
        self.then(Element::FocusRequester(focus.clone()))
    }

    /// Reports `rect`, in the area's own coordinates, as where text input goes while the
    /// area has keyboard focus, for an area that draws its own text, such as a terminal or
    /// an editor drawn on a canvas.
    ///
    /// The focus is that of the nearest [`focusable`](Self::focusable) around this modifier,
    /// or, when there is none, of the first one inside the area. While it holds focus,
    /// [`DisplayList::focused_ime_area`](crate::DisplayList::focused_ime_area) gives `rect`
    /// in the window, cut to what is visible, so the host can place an input method's
    /// candidate popup there.
    pub fn ime_caret(self, rect: Rect) -> Self {
        self.then(Element::ImeCaret(rect))
    }

    /// Like [`clickable`](Self::clickable), without the hover and press tint.
    ///
    /// For areas that only take clicks so they do not reach what is behind,
    /// such as a dialog's backdrop.
    ///
    /// Unlike `clickable`, it is not focusable: it is for the pointer alone.
    pub fn clickable_plain(self, on_click: impl Fn() + 'static) -> Self {
        self.then(Element::Clickable(Rc::new(on_click), false, true))
    }
}
