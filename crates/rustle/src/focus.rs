//! Keyboard focus and key events.

use std::rc::Rc;

use rustle_foundation::{Focus, Key, KeyEvent};
use rustle_ui::{
    DisplayList, FocusRegion, PinnableContainer, Preedit, take_focus_release, take_focus_request,
};

use crate::interface::Interface;

impl Interface {
    /// Moves focus to `focus`, telling the old and new owners and the focus-change observers
    /// of their areas.
    pub(crate) fn set_focus(&mut self, focus: Option<Focus>) {
        let same = match (&self.focused, &focus) {
            (Some(old), Some(new)) => old.id() == new.id(),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        if let Some(old) = &self.focused {
            old.set_focused(false);
            old.set_preedit(None);
        }
        self.ime_area = None;
        if let Some(new) = &focus {
            new.set_focused(true);
        }
        let lost = std::mem::take(&mut self.focus_observers);
        self.focus_pin = None;
        let region = focus
            .as_ref()
            .and_then(|new| self.display.focus_region(new));
        self.focus_unseen = focus.is_some() && region.is_none();
        self.focus_observers = region
            .map(|region| region.on_focus_changed.clone())
            .unwrap_or_default();
        if let Some(region) = region {
            self.focus_pin = region.pin.as_ref().map(PinnableContainer::pin);
            bring_into_view(&self.display, region);
        }
        let gained = self.focus_observers.clone();
        self.focused = focus;
        self.focus_changed = true;
        tell(&lost, false);
        tell(&gained, true);
    }

    /// Follows the area holding focus into the display just painted: takes focus away when
    /// that area is no longer in it, as Compose clears focus from a node that left the
    /// composition, and otherwise keeps its focus-change observers as painted now, telling
    /// them it has focus when it was given focus before it was painted.
    pub(crate) fn follow_focused_area(&mut self) {
        let Some(focused) = &self.focused else {
            return;
        };
        match self.display.focus_region(focused) {
            Some(region) => {
                self.focus_observers = region.on_focus_changed.clone();
                if std::mem::take(&mut self.focus_unseen) {
                    self.focus_pin = region.pin.as_ref().map(PinnableContainer::pin);
                    bring_into_view(&self.display, region);
                    tell(&self.focus_observers.clone(), true);
                }
            }
            None => self.set_focus(None),
        }
    }

    /// Moves focus to what the application asked for, if it asked, after taking it from
    /// what the application asked to free, if that has it.
    pub(crate) fn apply_focus_request(&mut self) {
        self.apply_focus(take_focus_release(), take_focus_request());
    }

    /// Takes focus from the area whose focus has the id `release`, if that has it, then gives
    /// it to `request`, if given.
    pub(crate) fn apply_focus(&mut self, release: Option<u64>, request: Option<Focus>) {
        if let Some(freed) = release
            && self
                .focused
                .as_ref()
                .is_some_and(|focus| focus.id() == freed)
        {
            self.set_focus(None);
        }
        if let Some(focus) = request {
            self.set_focus(Some(focus));
        }
    }

    /// Whether the last display painted an area holding `focus`.
    pub(crate) fn shows_focus(&self, focus: &Focus) -> bool {
        self.display.focus_region(focus).is_some()
    }

    /// Shows the input method's composition in the focused area, or ends it.
    pub(crate) fn set_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        if let Some(focus) = &self.focused {
            focus.set_preedit((!text.is_empty()).then_some(Preedit { text, cursor }));
        }
    }

    /// Gives a key event to the areas that preview keys (outermost first), then the focused
    /// area, then those that see what it left unhandled (innermost first). If none handled
    /// it, does its default: Tab moves focus along and Escape drops it.
    ///
    /// A focusable popup takes the keyboard while it is shown, as Compose's does: Escape
    /// dismisses the topmost one that asks for it before anything else sees the key.
    ///
    /// Returns whether the key was used: handled by an area, or acted on by its default.
    pub(crate) fn key_event(&mut self, event: KeyEvent) -> bool {
        if event.pressed
            && event.key == Key::Escape
            && let Some(dismiss) = self.display.popup_dismissed_by_escape()
        {
            dismiss();
            return true;
        }
        if self.display.dispatch_key(self.focused.as_ref(), &event) {
            return true;
        }
        if !event.pressed {
            return false;
        }
        match event.key {
            Key::Tab => {
                let regions = &self.display.focusables;
                let current = self
                    .focused
                    .as_ref()
                    .and_then(|focus| regions.iter().position(|r| r.focus.id() == focus.id()));
                let count = regions.len();
                let next = match (current, event.modifiers.shift) {
                    _ if count == 0 => None,
                    (None, false) => Some(0),
                    (None, true) => Some(count - 1),
                    (Some(index), false) => Some((index + 1) % count),
                    (Some(index), true) => Some((index + count - 1) % count),
                };
                let next = next.map(|index| regions[index].focus.clone());
                self.set_focus(next);
                true
            }
            Key::Escape => {
                let had_focus = self.focused.is_some();
                self.set_focus(None);
                had_focus
            }
            _ => false,
        }
    }
}

/// Scrolls the scrolling areas `region` is inside, innermost first, so as much of it as fits
/// is in view, as Compose's focus target asks its scrollable parents to bring it into view
/// when it takes focus: an area already wholly in view, or covering the whole view, stays
/// where it is, and one partly out of view moves the shorter way in. A scrolling area with
/// nothing of it in view, such as one laid out before the window had a size, does not move.
fn bring_into_view(display: &DisplayList, region: &FocusRegion) {
    let mut area = region.bounds;
    for &index in region.scrolls.iter().rev() {
        let Some(scroll) = display.scrolls.get(index) else {
            continue;
        };
        let (start, length, view_start, view_length) = if scroll.vertical {
            (
                area.origin.y,
                area.size.height,
                scroll.rect.origin.y,
                scroll.rect.size.height,
            )
        } else {
            (
                area.origin.x,
                area.size.width,
                scroll.rect.origin.x,
                scroll.rect.size.width,
            )
        };
        let distance = scroll_distance(start - view_start, length, view_length);
        if distance == 0.0 || view_length <= 0.0 {
            continue;
        }
        scroll.state.scroll_drawn_by(distance);
        if scroll.vertical {
            area.origin.y -= distance;
        } else {
            area.origin.x -= distance;
        }
    }
}

/// How far to scroll so an area `length` long whose near edge is `leading` past the near edge
/// of a view `view` long comes into view: Compose's default `BringIntoViewSpec`.
fn scroll_distance(leading: f32, length: f32, view: f32) -> f32 {
    let trailing = leading + length;
    if (leading >= 0.0 && trailing <= view) || (leading < 0.0 && trailing > view) {
        0.0
    } else if leading.abs() < (trailing - view).abs() {
        leading
    } else {
        trailing - view
    }
}

/// Tells each of `observers` whether its area holds focus now.
fn tell(observers: &[Rc<dyn Fn(bool)>], focused: bool) {
    for observer in observers {
        observer(focused);
    }
}
