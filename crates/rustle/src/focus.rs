//! Keyboard focus and key events.

use std::rc::Rc;

use rustle_foundation::{Focus, Key, KeyEvent};
use rustle_ui::take_focus_request;

use crate::app::App;

impl App {
    /// Moves focus to `focus`, telling the old and new owners.
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
        }
        if let Some(new) = &focus {
            new.set_focused(true);
        }
        self.focused = focus;
        // An input method only gets keys while something wants text, so it
        // cannot take keys from the rest of the application.
        if let Some(state) = &self.window {
            state.window.set_ime_allowed(self.focused.is_some());
        }
    }

    pub(crate) fn apply_focus_request(&mut self) {
        if let Some(focus) = take_focus_request() {
            self.set_focus(Some(focus));
        }
    }

    /// Gives a key event to the focused area, then does its default if unhandled:
    /// Tab moves focus along and Escape drops it.
    pub(crate) fn key_event(&mut self, event: KeyEvent) {
        let handler = self.focused.as_ref().and_then(|focus| {
            self.display
                .focusables
                .iter()
                .find(|region| region.focus.id() == focus.id())
                .map(|region| Rc::clone(&region.on_key))
        });
        if let Some(handler) = handler
            && handler(&event)
        {
            return;
        }
        if !event.pressed {
            return;
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
            }
            Key::Escape => self.set_focus(None),
            _ => {}
        }
    }
}
