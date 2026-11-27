//! The window-free core of an application: its display list, and the pointer, hover, drag,
//! focus and key state that input changes.
//!
//! [`App`](crate::app::App) keeps one per native window and [`Embedded`](crate::Embedded)
//! keeps one for the area its host gives it. Nothing here touches a window: what a window
//! would be asked to do (redraw, a different cursor, allowing an input method) is recorded
//! and left for the owner to apply.

use std::borrow::Cow;
use std::mem;
use std::rc::Rc;
use std::time::{Duration, Instant};

use rustle_foundation::{Focus, KeyEvent, Modifiers};
use rustle_geometry::{Offset, Rect};
use rustle_graphics::DrawCommand;
use rustle_ui::{CursorIcon, DisplayList, LaidOut, PointerButton, PointerId, PointerState};

use crate::contact::ActivePointer;
use crate::overlay::{carrying_overlay, feedback};
use crate::pointing::HeldButton;

/// An area following the pointer, and what to tell it when the pointer leaves.
pub(crate) type FollowEntry = (Rect, Rc<dyn Fn(Option<Offset>)>);

/// An area the pointer is over, and what to tell it when the pointer leaves.
pub(crate) type HoverEntry = (Rect, Rc<dyn Fn(bool)>);

/// A press that becomes a long press if it is held: when, what, where it began, and how far
/// the pointer may move from there before it is not a long press.
pub(crate) type LongPress = (Instant, Rc<dyn Fn()>, Offset, f32);

/// Something picked up by a drag source: what it carries, where the press began, and
/// whether the pointer has moved far enough for it to count as a drag.
pub(crate) struct Carrying {
    pub(crate) payload: Rc<dyn std::any::Any>,
    /// The button that holds it.
    pub(crate) button: PointerButton,
    pub(crate) began: Offset,
    pub(crate) active: bool,
    /// Where the picked-up area is, and which of the display list's commands draw it.
    pub(crate) rect: Rect,
    pub(crate) commands: std::ops::Range<usize>,
    /// How far the pointer moves from `began` before the area is picked up.
    pub(crate) touch_slop: f32,
}

/// A drag in progress: its handler, and where its area's corner is.
pub(crate) type Drag = (Rc<dyn Fn(Offset)>, Offset, Option<Rc<dyn Fn()>>);

/// What an area of the screen shows and what input has done to it.
#[derive(Default)]
pub(crate) struct Interface {
    pub(crate) display: DisplayList,
    /// The measured tree `display` was painted from, kept to paint it again when only the
    /// interaction state of its areas changed.
    pub(crate) laid_out: Option<LaidOut>,
    /// Where the primary button (or the first finger) went down, while it is held.
    pub(crate) pressed_at: Option<Offset>,
    /// Where the pointer is, in logical pixels, while it is over the area.
    pub(crate) pointer: Option<Offset>,
    /// Where the pointer last was over the area, kept after it leaves, so a button released
    /// outside the area is still told somewhere to its areas.
    pub(crate) pointer_last: Option<Offset>,
    /// Buttons whose release was made up because the window lost them, such as when it lost
    /// focus while they were held, so the real release, if it still arrives, is not told twice.
    pub(crate) released_early: Vec<PointerButton>,
    /// What has keyboard focus.
    pub(crate) focused: Option<Focus>,
    /// What asked to be told when the focus held now goes: the focus-change observers of the
    /// area holding it, as the last display painted them.
    pub(crate) focus_observers: Vec<Rc<dyn Fn(bool)>>,
    /// Whether focus was given to an area not painted yet, whose observers are still to be
    /// told when it is.
    pub(crate) focus_unseen: bool,
    /// The pin the area holding focus keeps on what it belongs to, such as the item of a
    /// lazy list, so it stays composed while it holds focus.
    pub(crate) focus_pin: Option<rustle_ui::PinnedHandle>,
    /// Text a key press just typed, so the same text arriving again from an input method is not typed twice.
    pub(crate) typed: Option<(Instant, String)>,
    /// Whether to print the input received (`RUSTLE_DEBUG_INPUT`).
    pub(crate) debug_input: bool,
    /// A long press waiting for its time: when it is due, what to run, and where it began.
    pub(crate) long_press: Option<LongPress>,
    /// The hover handlers of the areas the pointer is over.
    pub(crate) hovered: Vec<HoverEntry>,
    /// The areas whose pointer-move handlers have been told where the pointer is.
    pub(crate) following: Vec<FollowEntry>,
    /// The handler following a drag, and where its area's corner is.
    pub(crate) dragging: Option<Drag>,
    /// What was picked up with `drag_source`, and where, until the button is released.
    pub(crate) carrying: Option<Carrying>,
    /// The press on a `drag_and_drop_source` and the drag it became, until the button is
    /// released.
    pub(crate) drag_and_drop: Option<crate::drag_and_drop::DragAndDrop>,
    /// Files from another application dragged over the area, until they are let go or leave.
    pub(crate) file_drag_and_drop: Option<crate::drag_and_drop::FileDragAndDrop>,
    pub(crate) modifiers: Modifiers,
    /// When and where the primary button last went down, and how many in a row.
    pub(crate) last_press: Option<(Instant, Offset, u32)>,
    /// Pointer buttons that are down, for the areas that react to them.
    pub(crate) held_buttons: Vec<HeldButton>,
    /// Pointers that are down on areas that watch the raw pointer stream.
    pub(crate) pointers: Vec<ActivePointer>,
    /// The contact that stands in for the primary button while it is down.
    pub(crate) primary_contact: Option<PointerId>,
    /// The caret area last reported to the input method, so it is only sent when it moves.
    pub(crate) ime_area: Option<Rect>,
    /// Where the primary button went down on something clickable. Areas are told apart by
    /// where they are, not by their handlers, because every recomposition makes new
    /// handlers, and an animation recomposes every frame.
    pub(crate) pressed: Option<Rect>,
    /// Whether what is shown may have changed since the owner last drew it.
    pub(crate) redraw: bool,
    /// Whether focus moved since the owner last looked, so it can tell the input method.
    pub(crate) focus_changed: bool,
    /// The composition whose tree is shown, through which painting counts the state it
    /// reads as read while drawing, so a change to it draws again without composing.
    pub(crate) phases: Option<rustle_runtime::PhaseObserver>,
    /// The mouse buttons held, as pointer events report them.
    pub(crate) pointer_buttons: rustle_ui::PointerButtons,
    /// The areas listening to pointer events the pointer is over, outermost first.
    pub(crate) pointer_event_hovered: Vec<crate::pointer_events::ListenerKey>,
    /// The listening areas the first press of the buttons held landed on, which hear every
    /// pointer event until the last button is released.
    pub(crate) pointer_event_captured: Option<Vec<crate::pointer_events::ListenerKey>>,
    /// Where the pointer was at the pointer event before.
    pub(crate) pointer_event_previous: Option<Offset>,
}

impl Interface {
    /// An interface with nothing laid out yet.
    pub(crate) fn new() -> Self {
        Self {
            debug_input: std::env::var_os("RUSTLE_DEBUG_INPUT").is_some(),
            ..Self::default()
        }
    }

    /// Swaps what belongs to one native window with `other`'s, leaving what is shared by all
    /// windows (the modifier keys, the buttons held and the caret area last reported).
    pub(crate) fn exchange(&mut self, other: &mut Self) {
        mem::swap(&mut self.display, &mut other.display);
        mem::swap(&mut self.laid_out, &mut other.laid_out);
        mem::swap(&mut self.pressed_at, &mut other.pressed_at);
        mem::swap(&mut self.pointer, &mut other.pointer);
        mem::swap(&mut self.pointer_last, &mut other.pointer_last);
        mem::swap(&mut self.focused, &mut other.focused);
        mem::swap(&mut self.focus_observers, &mut other.focus_observers);
        mem::swap(&mut self.focus_unseen, &mut other.focus_unseen);
        mem::swap(&mut self.focus_pin, &mut other.focus_pin);
        mem::swap(&mut self.typed, &mut other.typed);
        mem::swap(&mut self.long_press, &mut other.long_press);
        mem::swap(&mut self.hovered, &mut other.hovered);
        mem::swap(&mut self.following, &mut other.following);
        mem::swap(&mut self.dragging, &mut other.dragging);
        mem::swap(&mut self.carrying, &mut other.carrying);
        mem::swap(&mut self.drag_and_drop, &mut other.drag_and_drop);
        mem::swap(&mut self.file_drag_and_drop, &mut other.file_drag_and_drop);
        mem::swap(&mut self.last_press, &mut other.last_press);
        mem::swap(&mut self.pointers, &mut other.pointers);
        mem::swap(&mut self.primary_contact, &mut other.primary_contact);
        mem::swap(&mut self.pressed, &mut other.pressed);
        mem::swap(&mut self.redraw, &mut other.redraw);
        mem::swap(&mut self.focus_changed, &mut other.focus_changed);
        mem::swap(
            &mut self.pointer_event_hovered,
            &mut other.pointer_event_hovered,
        );
        mem::swap(
            &mut self.pointer_event_captured,
            &mut other.pointer_event_captured,
        );
        mem::swap(
            &mut self.pointer_event_previous,
            &mut other.pointer_event_previous,
        );
    }

    /// Forgets everything tied to the tree that was shown, which no longer exists, and keeps
    /// where the pointer is and which modifier keys are held.
    pub(crate) fn clear(&mut self) {
        *self = Self {
            pointer: self.pointer,
            pointer_event_previous: self.pointer,
            pointer_buttons: self.pointer_buttons,
            modifiers: self.modifiers,
            debug_input: self.debug_input,
            redraw: true,
            phases: self.phases.take(),
            ..Self::default()
        };
    }

    /// Shows `laid_out`, a tree just measured, painting its interactive areas in the state
    /// the pointer and focus put them in now.
    pub(crate) fn show(&mut self, laid_out: LaidOut) {
        let previous = mem::take(&mut self.display);
        self.display = self.paint(&laid_out, self.pointer_state(), previous);
        self.follow_listeners();
        self.laid_out = Some(laid_out);
        self.follow_focused_area();
        self.update_pointer_event_hover();
    }

    /// Paints `laid_out` with its interactive areas in the state `pointer` puts them in,
    /// reusing what `previous`, the list shown before, holds of the subtrees that did not
    /// change, and counting the state the drawing reads as read while drawing the composition
    /// shown.
    fn paint(
        &self,
        laid_out: &LaidOut,
        pointer: PointerState,
        previous: DisplayList,
    ) -> DisplayList {
        let paint = || laid_out.paint_reusing(pointer, previous);
        match &self.phases {
            Some(phases) => phases.run(rustle_runtime::Phase::Drawing, paint),
            None => paint(),
        }
    }

    /// Where the pointer is and where the primary button went down, for interactive areas.
    pub(crate) fn pointer_state(&self) -> PointerState {
        PointerState {
            position: self.pointer,
            pressed_at: self.pressed_at,
        }
    }

    /// Paints the shown tree again, without composing or measuring it, when the hover,
    /// press or focus state of one of its interactive areas changed, and asks for a redraw.
    /// Does nothing when no area is drawn by its interaction state.
    pub(crate) fn refresh_interactions(&mut self) {
        let pointer = self.pointer_state();
        if let Some(laid_out) = &self.laid_out
            && self.display.interactions_changed(pointer)
        {
            let previous = mem::take(&mut self.display);
            self.display = self.paint(laid_out, pointer, previous);
            self.follow_listeners();
            self.redraw = true;
        }
    }

    /// Paints the shown tree again, without composing or measuring it, and asks for a redraw.
    /// With `reuse`, the subtrees whose drawing read no state that changed are reused, which
    /// suits a redraw asked for by state read while drawing; without, every drawing closure
    /// runs again, so those that read something outside composition, such as a clock the
    /// host keeps, see it change.
    pub(crate) fn repaint(&mut self, reuse: bool) {
        if let Some(laid_out) = &self.laid_out {
            let previous = if reuse {
                mem::take(&mut self.display)
            } else {
                DisplayList::default()
            };
            self.display = self.paint(laid_out, self.pointer_state(), previous);
            self.follow_listeners();
            self.redraw = true;
        }
    }

    /// Settles what input may have changed: moves focus to what the application asked for,
    /// then paints interactive areas again if their state changed.
    pub(crate) fn after_input(&mut self) {
        self.apply_focus_request();
        self.refresh_interactions();
    }

    /// Takes whether the owner should draw again.
    pub(crate) fn take_redraw(&mut self) -> bool {
        mem::take(&mut self.redraw)
    }

    /// Takes whether focus moved, so the owner can tell the input method.
    pub(crate) fn take_focus_changed(&mut self) -> bool {
        mem::take(&mut self.focus_changed)
    }

    /// Whether the pointer is over something in the interface: an area that reacts to it, or
    /// one that blocks it.
    pub(crate) fn pointer_over_interface(&self) -> bool {
        self.pointer
            .is_some_and(|pointer| self.display.interactive_at(pointer))
    }

    /// The cursor the area under the pointer asks for, or a grabbing hand while something
    /// is carried.
    pub(crate) fn cursor(&self) -> CursorIcon {
        if self
            .carrying
            .as_ref()
            .is_some_and(|carrying| carrying.active)
        {
            return CursorIcon::Grabbing;
        }
        let Some(pointer) = self.pointer else {
            return CursorIcon::Default;
        };
        self.display
            .cursor_at(pointer)
            .or_else(|| {
                self.display
                    .hit_test(pointer)
                    .filter(|region| region.hand)
                    .map(|_| CursorIcon::Pointer)
            })
            .unwrap_or_default()
    }

    /// Everything that is drawn: the display list's commands, the feedback over the area
    /// under the pointer and what is being carried.
    pub(crate) fn commands(&self) -> Cow<'_, [DrawCommand]> {
        let display = &self.display;
        let carried = carrying_overlay(display, self.carrying.as_ref(), self.pointer);
        let feedback = if carried.is_some() {
            None
        } else {
            feedback(display, self.pointer, &self.pressed)
        };
        match (feedback, carried) {
            (None, None) => Cow::Borrowed(display.commands.as_slice()),
            (feedback, carried) => {
                let mut commands = display.commands.clone();
                commands.extend(feedback);
                commands.extend(carried.into_iter().flatten());
                Cow::Owned(commands)
            }
        }
    }

    /// Calls `draw` with everything that is drawn, as [`Interface::commands`] gives it, without
    /// copying the display list: the feedback and what is carried are appended to it for the
    /// call and taken off again after. Returns what `draw` returned.
    pub(crate) fn with_commands<R>(&mut self, draw: impl FnOnce(&[DrawCommand]) -> R) -> R {
        let display = &self.display;
        let carried = carrying_overlay(display, self.carrying.as_ref(), self.pointer);
        let feedback = if carried.is_some() {
            None
        } else {
            feedback(display, self.pointer, &self.pressed)
        };
        if feedback.is_none() && carried.is_none() {
            return draw(&self.display.commands);
        }
        let painted = self.display.commands.len();
        self.display.commands.extend(feedback);
        self.display.commands.extend(carried.into_iter().flatten());
        let drawn = draw(&self.display.commands);
        self.display.commands.truncate(painted);
        drawn
    }

    /// The pointer left the area.
    pub(crate) fn pointer_left(&mut self) {
        self.pointer = None;
        self.redraw = true;
        self.update_following();
        self.update_hover();
        self.pointer_event_left();
    }

    /// The wheel or a trackpad scrolled by `delta` logical pixels where the pointer is.
    /// Returns whether a scrolling area took it.
    pub(crate) fn wheel(&mut self, delta: Offset) -> bool {
        let (consumed, held) = self.pointer_event_scroll(delta);
        let taken = consumed || self.scroll_areas(delta);
        self.finish_pointer_event(held);
        taken
    }

    /// Scrolls the areas under the pointer by `delta`, as the wheel does when no listening
    /// area consumed it. Returns whether one took it.
    fn scroll_areas(&mut self, delta: Offset) -> bool {
        if self.pointer_input_scroll(delta) {
            return true;
        }
        let Some(pointer) = self.pointer else {
            return false;
        };
        let mut taken = false;
        for (vertical, amount) in [(true, delta.y), (false, delta.x)] {
            if amount != 0.0
                && let Some(region) = self.display.scroll_at(pointer, vertical)
            {
                region.state.scroll_drawn_by(-amount);
                taken = true;
            }
        }
        taken
    }

    /// A pinch gesture changed the scale by `delta` where the pointer is.
    pub(crate) fn pinch(&mut self, delta: f32) {
        if let Some(pointer) = self.pointer
            && let Some(region) = self.display.pinch_at(pointer)
        {
            let on_pinch = Rc::clone(&region.on_pinch);
            on_pinch(delta);
        }
    }

    /// A key went down or up, or text was typed. Returns whether the key was used.
    pub(crate) fn keyboard(&mut self, event: KeyEvent) -> bool {
        if event.pressed
            && let Some(text) = &event.text
        {
            self.typed = Some((Instant::now(), text.clone()));
        }
        self.key_event(event)
    }

    /// The input method is composing `text`, with the highlighted part at `cursor`.
    /// An empty `text` ends the composition.
    pub(crate) fn preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        if self.debug_input {
            eprintln!("rustle input: input method composing {text:?} {cursor:?}");
        }
        if let Some(region) = self.display.focused_text_input()
            && let Some(request) = region.session.request()
        {
            let preedit = (!text.is_empty()).then(|| rustle_ui::Preedit {
                text: text.clone(),
                cursor,
            });
            request.on_preedit(preedit.as_ref());
            self.redraw = true;
        }
        self.set_preedit(text, cursor);
    }

    /// The input method produced the final `text`, which ends the composition.
    /// Returns whether a field took it.
    pub(crate) fn commit(&mut self, text: String) -> bool {
        self.set_preedit(String::new(), None);
        if self.debug_input {
            eprintln!("rustle input: input method committed {text:?}");
        }
        // Some input methods deliver text a key press already typed.
        let repeated = self.typed.as_ref().is_some_and(|(when, typed)| {
            *typed == text && when.elapsed() < Duration::from_millis(60)
        });
        if repeated {
            return false;
        }
        if let Some(region) = self.display.focused_text_input()
            && let Some(request) = region.session.request()
        {
            request.on_commit(&text);
            self.redraw = true;
            return true;
        }
        let event = KeyEvent::typed(text, self.modifiers);
        self.key_event(event) || self.focused.is_some()
    }

    /// Where text input goes, in logical pixels from the area's top-left corner, for placing
    /// an input method's candidate popup: the caret of the focused field, or the
    /// [`ime_caret`](rustle_ui::Modifier::ime_caret) of the focused area that draws its own
    /// text. `None` when nothing that takes text has focus.
    pub(crate) fn caret_area(&self) -> Option<Rect> {
        self.display
            .focused_ime_area()
            .filter(|_| self.focused.is_some())
    }
}
