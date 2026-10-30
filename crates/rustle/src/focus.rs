//! Keyboard focus and key events.

use rustle_foundation::{Focus, Key, KeyEvent};
use rustle_ui::{Preedit, take_focus_request};
use winit::dpi::{LogicalPosition, LogicalSize};

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
            old.set_preedit(None);
        }
        self.ime_area = None;
        if let Some(new) = &focus {
            new.set_focused(true);
        }
        self.focused = focus;
        // An input method only gets keys while something wants text, so it
        // cannot take keys from the rest of the application.
        if let Some(state) = &self.window {
            state
                .window
                .set_ime_allowed(self.focused.is_some() && self.ime_allowed());
        }
    }

    pub(crate) fn apply_focus_request(&mut self) {
        if let Some(focus) = take_focus_request() {
            self.set_focus(Some(focus));
        }
    }

    /// Shows the input method's composition in the focused area, or ends it.
    pub(crate) fn set_preedit(&mut self, text: String, cursor: Option<(usize, usize)>) {
        if let Some(focus) = &self.focused {
            focus.set_preedit((!text.is_empty()).then_some(Preedit { text, cursor }));
        }
    }

    /// Tells the window where the caret is, once per move, so the input method's
    /// candidate popup appears next to it.
    pub(crate) fn sync_ime_area(&mut self) {
        let area = self.display.ime_area.filter(|_| self.focused.is_some());
        if area == self.ime_area {
            return;
        }
        self.ime_area = area;
        if let (Some(area), Some(state)) = (area, &self.window) {
            let scale = state.window.scale_factor();
            state.window.set_ime_cursor_area(
                LogicalPosition::new(area.origin.x as f64, area.origin.y as f64)
                    .to_physical::<f64>(scale),
                LogicalSize::new(area.size.width as f64, area.size.height as f64)
                    .to_physical::<f64>(scale),
            );
        }
    }

    /// Gives a key event to the areas that preview keys (outermost first), then the focused
    /// area, then those that see what it left unhandled (innermost first). If none handled
    /// it, does its default: Tab moves focus along and Escape drops it.
    pub(crate) fn key_event(&mut self, event: KeyEvent) {
        if self.display.dispatch_key(self.focused.as_ref(), &event) {
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
