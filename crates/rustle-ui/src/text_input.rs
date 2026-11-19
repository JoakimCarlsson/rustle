//! Text input for areas that draw their own text, as Compose's
//! `PlatformTextInputModifierNode` and `establishTextInputSession` give it: an area starts an
//! input method session while it holds focus, hears the input method's composition and the
//! text it commits, and says where its caret is so the system's candidate window sits by it.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect};

use crate::input::{Focus, Preedit};
use crate::layout::DisplayList;
use crate::layout::regions::intersect;

/// What an area that takes text from the platform's input method does with it: Compose's
/// `PlatformTextInputMethodRequest`.
///
/// Started with [`PlatformTextInputSession::start_input_method`]; while it is, and the area
/// holds focus, the input method's composition and committed text go here instead of the
/// key handlers.
pub trait PlatformTextInputMethodRequest {
    /// The input method is composing `preedit` (with the selected part or caret within it),
    /// or stopped composing when it is `None`. The area draws the composition itself.
    fn on_preedit(&self, preedit: Option<&Preedit>);

    /// The input method produced `text`, which ends any composition.
    fn on_commit(&self, text: &str);

    /// Where the caret or the composition is, in the area's own coordinates, so the
    /// platform places its candidate window next to it: Compose's `focusedRectInRoot`, made
    /// relative to the area. `None` leaves the candidate window where it was.
    fn focused_rect(&self) -> Option<Rect> {
        None
    }
}

/// The session an area that draws its own text holds with the platform's input method:
/// Compose's `PlatformTextInputSession`, as `establishTextInputSession` opens it.
///
/// Attach it to the area with [`Modifier::platform_text_input`](crate::Modifier::platform_text_input),
/// then call [`start_input_method`](Self::start_input_method) once the area holds focus
/// (from [`Modifier::on_focus_changed`](crate::Modifier::on_focus_changed), say) and
/// [`stop_input_method`](Self::stop_input_method) when it loses it. Cheap to clone; clones
/// are the same session.
#[derive(Clone, Default)]
pub struct PlatformTextInputSession {
    request: Rc<RefCell<Option<Rc<dyn PlatformTextInputMethodRequest>>>>,
}

impl PlatformTextInputSession {
    /// A session with no input method started.
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts taking text from the input method with `request`, replacing the request
    /// started before: Compose's `startInputMethod`.
    pub fn start_input_method(&self, request: impl PlatformTextInputMethodRequest + 'static) {
        *self.request.borrow_mut() = Some(Rc::new(request));
    }

    /// Stops taking text from the input method: what Compose does when the
    /// `establishTextInputSession` block ends.
    pub fn stop_input_method(&self) {
        self.request.borrow_mut().take();
    }

    /// Whether an input method is started.
    pub fn is_active(&self) -> bool {
        self.request.borrow().is_some()
    }

    /// The request started with [`start_input_method`](Self::start_input_method), if any.
    pub fn request(&self) -> Option<Rc<dyn PlatformTextInputMethodRequest>> {
        self.request.borrow().clone()
    }
}

impl PartialEq for PlatformTextInputSession {
    /// Whether both are the same session.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.request, &other.request)
    }
}

impl std::fmt::Debug for PlatformTextInputSession {
    /// Whether an input method is started.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformTextInputSession")
            .field("active", &self.is_active())
            .finish()
    }
}

/// A session remembered across recompositions, for the area that holds it.
#[track_caller]
pub fn remember_platform_text_input_session() -> PlatformTextInputSession {
    rustle_runtime::remember(PlatformTextInputSession::new).get()
}

/// An area that takes text from the input method through a session, cut to what is visible.
pub struct TextInputRegion {
    /// The visible part of the area, in the window.
    pub rect: Rect,
    /// The area's top-left corner in the window.
    pub origin: Offset,
    /// The focus that must be held for the session to take text: the nearest focusable
    /// around the area, or the first inside it.
    pub focus: Option<Focus>,
    /// The session.
    pub session: PlatformTextInputSession,
}

impl TextInputRegion {
    /// Where the caret of the started request is in the window, cut to the visible area.
    pub fn caret(&self) -> Option<Rect> {
        let caret = self.session.request()?.focused_rect()?;
        let moved = Rect::new(
            Offset::new(
                self.origin.x + caret.origin.x,
                self.origin.y + caret.origin.y,
            ),
            caret.size,
        );
        let cut = intersect(moved, self.rect);
        Some(if cut.size.width > 0.0 || cut.size.height > 0.0 {
            cut
        } else {
            moved
        })
    }
}

impl DisplayList {
    /// The topmost area whose session has an input method started and whose focus is held:
    /// where the input method's text goes.
    pub fn focused_text_input(&self) -> Option<&TextInputRegion> {
        self.text_inputs.iter().rev().find(|region| {
            region.session.is_active() && region.focus.as_ref().is_some_and(Focus::is_focused)
        })
    }
}
