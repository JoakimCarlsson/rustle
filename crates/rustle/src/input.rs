//! Turns winit's keyboard events into Rustle's, and provides the system clipboard.

use std::cell::RefCell;

use rustle_ui::{Clipboard, Key, KeyEvent, Modifiers};
use winit::event::{ElementState, KeyEvent as WinitKeyEvent};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};

/// The system clipboard, opened when first used.
pub(crate) struct SystemClipboard {
    clipboard: RefCell<Option<arboard::Clipboard>>,
}

impl SystemClipboard {
    pub(crate) fn new() -> Self {
        Self {
            clipboard: RefCell::new(None),
        }
    }

    fn with<R>(&self, access: impl FnOnce(&mut arboard::Clipboard) -> Option<R>) -> Option<R> {
        let mut clipboard = self.clipboard.borrow_mut();
        if clipboard.is_none() {
            *clipboard = arboard::Clipboard::new().ok();
        }
        access(clipboard.as_mut()?)
    }
}

impl Clipboard for SystemClipboard {
    fn text(&self) -> Option<String> {
        self.with(|clipboard| clipboard.get_text().ok())
    }

    fn set_text(&self, text: &str) {
        self.with(|clipboard| clipboard.set_text(text).ok());
    }
}

pub(crate) fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.super_key(),
    }
}

pub(crate) fn key_event(event: &WinitKeyEvent, modifiers: Modifiers) -> KeyEvent {
    let key = match &event.logical_key {
        WinitKey::Named(named) => match named {
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Enter => Key::Enter,
            NamedKey::Tab => Key::Tab,
            NamedKey::Escape => Key::Escape,
            NamedKey::ArrowLeft => Key::ArrowLeft,
            NamedKey::ArrowRight => Key::ArrowRight,
            NamedKey::ArrowUp => Key::ArrowUp,
            NamedKey::ArrowDown => Key::ArrowDown,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Space => Key::Character(" ".to_string()),
            _ => Key::Other,
        },
        WinitKey::Character(text) => Key::Character(text.to_string()),
        _ => Key::Other,
    };
    KeyEvent {
        key,
        text: event.text.as_ref().map(ToString::to_string),
        modifiers,
        pressed: event.state == ElementState::Pressed,
    }
}
