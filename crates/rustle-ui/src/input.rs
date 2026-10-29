//! Keyboard input: key events, focus, and the clipboard.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_runtime::{State, group, remember};

/// A key, as far as text editing and navigation care.
#[derive(Debug, Clone, PartialEq)]
pub enum Key {
    /// A key that types text, such as a letter, with its unmodified text.
    Character(String),
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Other,
}

/// The modifier keys held down.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// The logo key: Windows, Command or Super.
    pub meta: bool,
}

impl Modifiers {
    /// Control, or Command on a keyboard that uses it for shortcuts.
    pub fn command(&self) -> bool {
        self.ctrl || self.meta
    }
}

/// A key going down or up.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    pub key: Key,
    /// The text the key typed, with modifiers and the keyboard layout applied.
    /// Text from an input method arrives here with a `Key::Other`.
    pub text: Option<String>,
    pub modifiers: Modifiers,
    pub pressed: bool,
}

/// A primary-button press on an area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressEvent {
    /// Where the pointer is, relative to the area's top-left corner.
    pub position: rustle_geometry::Offset,
    /// 1 for a single press, 2 for the second in quick succession, 3 for the third.
    pub click_count: u32,
}

/// Handles what was dropped on an area: the payload of the drag source it came from.
pub type DropHandler = Rc<dyn Fn(&dyn std::any::Any)>;

/// Handles a key event and says whether it did, so unhandled keys can do their default.
pub type KeyHandler = Rc<dyn Fn(&KeyEvent) -> bool>;

thread_local! {
    static NEXT_FOCUS_ID: Cell<u64> = const { Cell::new(1) };
    static FOCUS_REQUEST: RefCell<Option<Focus>> = const { RefCell::new(None) };
    static CLIPBOARD: RefCell<Option<Rc<dyn Clipboard>>> = const { RefCell::new(None) };
}

/// Identifies something that can have keyboard focus, and tells whether it has it.
#[derive(Clone)]
pub struct Focus {
    id: u64,
    focused: State<bool>,
}

impl Focus {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }

    /// Records whether this has focus. Called by the host when focus moves.
    pub fn set_focused(&self, focused: bool) {
        if self.focused.get() != focused {
            self.focused.set(focused);
        }
    }

    /// Asks the host to give this focus at its next opportunity.
    pub fn request_focus(&self) {
        FOCUS_REQUEST.with(|request| *request.borrow_mut() = Some(self.clone()));
    }
}

/// The focus the application asked for with [`Focus::request_focus`], if any.
pub fn take_focus_request() -> Option<Focus> {
    FOCUS_REQUEST.with(|request| request.borrow_mut().take())
}

/// Remembers a [`Focus`] at this call site.
#[track_caller]
pub fn remember_focus() -> Focus {
    group(|| Focus {
        id: remember(|| NEXT_FOCUS_ID.with(|next| next.replace(next.get() + 1))).get(),
        focused: remember(|| false),
    })
}

/// Reads and writes the system clipboard as text.
pub trait Clipboard {
    fn text(&self) -> Option<String>;
    fn set_text(&self, text: &str);
}

/// Installs the clipboard text editing uses. The host does this at startup.
pub fn set_clipboard(clipboard: Rc<dyn Clipboard>) {
    CLIPBOARD.with(|current| *current.borrow_mut() = Some(clipboard));
}

/// The installed clipboard, if the host provided one.
pub fn clipboard() -> Option<Rc<dyn Clipboard>> {
    CLIPBOARD.with(|current| current.borrow().clone())
}
