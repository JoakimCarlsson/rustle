//! Turns winit's keyboard events into Rustle's, and provides the system clipboard.

use std::cell::RefCell;

use rustle_ui::{
    Clipboard, Key, KeyCode, KeyEvent, KeyLocation, Modifiers, PhysicalKey, PointerButton,
};
use winit::event::{ElementState, KeyEvent as WinitKeyEvent, MouseButton};
use winit::keyboard::{
    Key as WinitKey, KeyLocation as WinitKeyLocation, ModifiersState, NamedKey,
    PhysicalKey as WinitPhysicalKey,
};

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

/// Installs the system clipboard as the one text editing uses, unless the program installed
/// its own with [`set_clipboard`](rustle_ui::set_clipboard) first, such as a test's fake.
pub(crate) fn install_system_clipboard() {
    if rustle_ui::clipboard().is_none() {
        rustle_ui::set_clipboard(std::rc::Rc::new(SystemClipboard::new()));
    }
}

impl Clipboard for SystemClipboard {
    fn text(&self) -> Option<String> {
        self.with(|clipboard| clipboard.get_text().ok())
    }

    fn set_text(&self, text: &str) {
        self.with(|clipboard| clipboard.set_text(text).ok());
    }

    fn set_html(&self, html: &str, text: &str) {
        self.with(|clipboard| clipboard.set().html(html, Some(text)).ok());
    }

    fn files(&self) -> Vec<std::path::PathBuf> {
        self.with(|clipboard| clipboard.get().file_list().ok())
            .unwrap_or_default()
    }

    fn image(&self) -> Option<rustle_ui::ClipboardImage> {
        self.with(|clipboard| {
            let image = clipboard.get_image().ok()?;
            Some(rustle_ui::ClipboardImage {
                width: u32::try_from(image.width).ok()?,
                height: u32::try_from(image.height).ok()?,
                rgba: image.bytes.into_owned(),
            })
        })
    }
}

/// Turns winit's modifier state into Rustle's.
pub(crate) fn modifiers(state: ModifiersState) -> Modifiers {
    Modifiers {
        shift: state.shift_key(),
        ctrl: state.control_key(),
        alt: state.alt_key(),
        meta: state.meta_key(),
    }
}

/// Turns winit's key event into Rustle's, with the modifiers held as `modifiers`, less
/// those the keyboard layout spent composing the character the key typed, such as
/// Option turning `a` into `å` on a Mac or AltGr turning `2` into `@` elsewhere, so the
/// character is typed rather than taken for a shortcut.
pub(crate) fn key_event(event: &WinitKeyEvent, modifiers: Modifiers) -> KeyEvent {
    let composed = modifiers.alt
        && matches!(
            (&event.logical_key, &event.key_without_modifiers),
            (WinitKey::Character(typed), WinitKey::Character(bare))
                if !typed.eq_ignore_ascii_case(bare)
        );
    let modifiers = if composed {
        Modifiers {
            alt: false,
            ctrl: false,
            ..modifiers
        }
    } else {
        modifiers
    };
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
            NamedKey::Insert => Key::Insert,
            NamedKey::PrintScreen => Key::PrintScreen,
            NamedKey::ContextMenu => Key::ContextMenu,
            NamedKey::Pause => Key::Pause,
            NamedKey::Help => Key::Help,
            NamedKey::Clear => Key::Clear,
            NamedKey::CapsLock => Key::CapsLock,
            NamedKey::NumLock => Key::NumLock,
            NamedKey::ScrollLock => Key::ScrollLock,
            NamedKey::Shift => Key::Shift,
            NamedKey::Control => Key::Control,
            NamedKey::Alt => Key::Alt,
            NamedKey::Meta => Key::Super,
            other => function_key(*other).map_or(Key::Other, Key::F),
        },
        WinitKey::Character(text) => Key::Character(text.to_string()),
        _ => Key::Other,
    };
    let physical_key = match event.physical_key {
        WinitPhysicalKey::Code(code) => KeyCode::from_name(&format!("{code:?}"))
            .map_or(PhysicalKey::Unidentified, PhysicalKey::Code),
        WinitPhysicalKey::Unidentified(_) => PhysicalKey::Unidentified,
    };
    KeyEvent {
        key,
        physical_key,
        location: match event.location {
            WinitKeyLocation::Standard => KeyLocation::Standard,
            WinitKeyLocation::Left => KeyLocation::Left,
            WinitKeyLocation::Right => KeyLocation::Right,
            WinitKeyLocation::Numpad => KeyLocation::Numpad,
        },
        text: event.text.as_ref().map(ToString::to_string),
        modifiers,
        pressed: event.state == ElementState::Pressed,
        repeat: event.repeat,
    }
}

/// The number of a function key such as F5, up to F35.
fn function_key(named: NamedKey) -> Option<u8> {
    format!("{named:?}")
        .strip_prefix('F')
        .and_then(|number| number.parse().ok())
}

/// Turns winit's mouse button into Rustle's.
pub(crate) fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Right => PointerButton::Secondary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        other => PointerButton::Other(other as u16),
    }
}
