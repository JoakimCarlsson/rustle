//! Keyboard input: key events, focus, and the clipboard.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_runtime::{State, group, mutable_state, remember};

/// A key, as far as text editing and navigation care.
///
/// This is the logical key: what the key means with the keyboard layout and
/// modifiers applied. For where the key sits on the keyboard, see [`KeyEvent::physical_key`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
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
    /// A function key, `F(1)` to `F(24)` (higher ones the keyboard has are `F(25)` to `F(35)`).
    F(u8),
    Insert,
    PrintScreen,
    /// The key that opens a context menu, between the right Alt and Control keys.
    ContextMenu,
    Pause,
    Help,
    Clear,
    CapsLock,
    NumLock,
    ScrollLock,
    /// A modifier key pressed or released on its own.
    Shift,
    /// A modifier key pressed or released on its own.
    Control,
    /// A modifier key pressed or released on its own.
    Alt,
    /// The logo key (Windows, Command or Super) pressed or released on its own.
    Super,
    /// A key with no counterpart above, or an input method's text.
    Other,
}

macro_rules! key_codes {
    ($($name:ident),* $(,)?) => {
        /// A key by where it is on the keyboard, whatever the layout makes of it.
        /// Named after the W3C UI Events `code` values, such as `KeyA` or `ArrowLeft`.
        ///
        /// Use these for shortcuts that should stay in the same place on every layout, such as
        /// WASD movement. Use [`Key`] for shortcuts that should follow what the key says.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum KeyCode {
            $(
                #[allow(missing_docs)]
                $name,
            )*
        }

        impl KeyCode {
            /// The variant's name, such as `"KeyA"`.
            pub fn name(self) -> &'static str {
                match self {
                    $(Self::$name => stringify!($name),)*
                }
            }

            /// The key called `name` (case-sensitive), such as `"KeyA"` or `"F5"`.
            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $(stringify!($name) => Some(Self::$name),)*
                    _ => None,
                }
            }
        }
    };
}

key_codes! {
        Backquote,
        Backslash,
        BracketLeft,
        BracketRight,
        Comma,
        Digit0,
        Digit1,
        Digit2,
        Digit3,
        Digit4,
        Digit5,
        Digit6,
        Digit7,
        Digit8,
        Digit9,
        Equal,
        IntlBackslash,
        IntlRo,
        IntlYen,
        KeyA,
        KeyB,
        KeyC,
        KeyD,
        KeyE,
        KeyF,
        KeyG,
        KeyH,
        KeyI,
        KeyJ,
        KeyK,
        KeyL,
        KeyM,
        KeyN,
        KeyO,
        KeyP,
        KeyQ,
        KeyR,
        KeyS,
        KeyT,
        KeyU,
        KeyV,
        KeyW,
        KeyX,
        KeyY,
        KeyZ,
        Minus,
        Period,
        Quote,
        Semicolon,
        Slash,
        AltLeft,
        AltRight,
        Backspace,
        CapsLock,
        ContextMenu,
        ControlLeft,
        ControlRight,
        Enter,
        SuperLeft,
        SuperRight,
        ShiftLeft,
        ShiftRight,
        Space,
        Tab,
        Convert,
        KanaMode,
        Lang1,
        Lang2,
        Lang3,
        Lang4,
        Lang5,
        NonConvert,
        Delete,
        End,
        Help,
        Home,
        Insert,
        PageDown,
        PageUp,
        ArrowDown,
        ArrowLeft,
        ArrowRight,
        ArrowUp,
        NumLock,
        Numpad0,
        Numpad1,
        Numpad2,
        Numpad3,
        Numpad4,
        Numpad5,
        Numpad6,
        Numpad7,
        Numpad8,
        Numpad9,
        NumpadAdd,
        NumpadBackspace,
        NumpadClear,
        NumpadClearEntry,
        NumpadComma,
        NumpadDecimal,
        NumpadDivide,
        NumpadEnter,
        NumpadEqual,
        NumpadHash,
        NumpadMemoryAdd,
        NumpadMemoryClear,
        NumpadMemoryRecall,
        NumpadMemoryStore,
        NumpadMemorySubtract,
        NumpadMultiply,
        NumpadParenLeft,
        NumpadParenRight,
        NumpadStar,
        NumpadSubtract,
        Escape,
        Fn,
        FnLock,
        PrintScreen,
        ScrollLock,
        Pause,
        BrowserBack,
        BrowserFavorites,
        BrowserForward,
        BrowserHome,
        BrowserRefresh,
        BrowserSearch,
        BrowserStop,
        Eject,
        LaunchApp1,
        LaunchApp2,
        LaunchMail,
        MediaPlayPause,
        MediaSelect,
        MediaStop,
        MediaTrackNext,
        MediaTrackPrevious,
        Power,
        Sleep,
        AudioVolumeDown,
        AudioVolumeMute,
        AudioVolumeUp,
        WakeUp,
        Meta,
        Hyper,
        Turbo,
        Abort,
        Resume,
        Suspend,
        Again,
        Copy,
        Cut,
        Find,
        Open,
        Paste,
        Props,
        Select,
        Undo,
        Hiragana,
        Katakana,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        F13,
        F14,
        F15,
        F16,
        F17,
        F18,
        F19,
        F20,
        F21,
        F22,
        F23,
        F24,
        F25,
        F26,
        F27,
        F28,
        F29,
        F30,
        F31,
        F32,
        F33,
        F34,
        F35,
}

/// Where a key is on the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalKey {
    /// A key the platform could identify.
    Code(KeyCode),
    /// A key the platform could not identify, or an input method's text.
    Unidentified,
}

impl PhysicalKey {
    /// The key code, if the key was identified.
    pub fn code(self) -> Option<KeyCode> {
        match self {
            Self::Code(code) => Some(code),
            Self::Unidentified => None,
        }
    }
}

/// Which instance of a key was used, for keys that appear more than once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyLocation {
    /// The only key of its kind, or the main one.
    #[default]
    Standard,
    /// The left one of a pair, such as the left Shift.
    Left,
    /// The right one of a pair.
    Right,
    /// On the numeric keypad.
    Numpad,
}

/// The modifier keys held down.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// The logo key: Windows, Command or Super.
    pub meta: bool,
}

impl Modifiers {
    /// No modifier held.
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
    };
    /// Only Shift.
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        ..Self::NONE
    };
    /// Only Control.
    pub const CTRL: Modifiers = Modifiers {
        ctrl: true,
        ..Self::NONE
    };
    /// Only Alt.
    pub const ALT: Modifiers = Modifiers {
        alt: true,
        ..Self::NONE
    };
    /// Only the logo key.
    pub const META: Modifiers = Modifiers {
        meta: true,
        ..Self::NONE
    };

    /// Control, or Command on a keyboard that uses it for shortcuts.
    pub fn command(&self) -> bool {
        self.ctrl || self.meta
    }

    /// Whether no modifier is held.
    pub fn is_empty(&self) -> bool {
        *self == Self::NONE
    }

    /// These modifiers plus Shift.
    pub const fn with_shift(self) -> Self {
        Self {
            shift: true,
            ..self
        }
    }

    /// These modifiers plus Control.
    pub const fn with_ctrl(self) -> Self {
        Self { ctrl: true, ..self }
    }

    /// These modifiers plus Alt.
    pub const fn with_alt(self) -> Self {
        Self { alt: true, ..self }
    }

    /// These modifiers plus the logo key.
    pub const fn with_meta(self) -> Self {
        Self { meta: true, ..self }
    }
}

/// A key going down or up.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// What the key means with the layout and modifiers applied.
    pub key: Key,
    /// Where the key is on the keyboard, whatever the layout.
    pub physical_key: PhysicalKey,
    /// Which of several equal keys it is, such as left or right Shift.
    pub location: KeyLocation,
    /// The text the key typed, with modifiers and the keyboard layout applied.
    /// Text from an input method arrives here with a `Key::Other`.
    pub text: Option<String>,
    /// The modifiers held when the key went down or up.
    pub modifiers: Modifiers,
    /// True when the key went down (including repeats), false when it was released.
    pub pressed: bool,
    /// True when this press is the keyboard repeating a key held down.
    pub repeat: bool,
}

impl KeyEvent {
    /// A press of no particular key that typed `text`, as an input method's commit does.
    pub fn typed(text: impl Into<String>, modifiers: Modifiers) -> Self {
        Self {
            key: Key::Other,
            physical_key: PhysicalKey::Unidentified,
            location: KeyLocation::Standard,
            text: Some(text.into()),
            modifiers,
            pressed: true,
            repeat: false,
        }
    }

    /// A press of `key` with `modifiers` and nothing else known. For tests and replays.
    pub fn press(key: Key, modifiers: Modifiers) -> Self {
        let text = match &key {
            Key::Character(text) => Some(text.clone()),
            _ => None,
        };
        Self {
            key,
            physical_key: PhysicalKey::Unidentified,
            location: KeyLocation::Standard,
            text,
            modifiers,
            pressed: true,
            repeat: false,
        }
    }

    /// The character the key typed, with Shift and the keyboard layout applied: Compose's
    /// `KeyEvent.utf16CodePoint`. It is the first character of [`text`](Self::text) that is
    /// not a control character, or, for a character key that typed no text (such as one
    /// pressed with Control), the first character of the key's own text. `None` for keys
    /// that type nothing, such as the arrows.
    pub fn code_point(&self) -> Option<char> {
        let typed = self
            .text
            .as_deref()
            .and_then(|text| text.chars().next())
            .filter(|character| !character.is_control());
        match &self.key {
            Key::Character(bare) => typed.or_else(|| bare.chars().next()),
            Key::Other => typed,
            _ => None,
        }
    }

    /// Whether this is a press (not a release, but a repeat counts) of the logical `key`
    /// with exactly `modifiers` held. Letters match whatever their case.
    ///
    /// `Shift` is only compared when the key is not a character, because Shift
    /// changes what a character is: `is(Key::Character("+"), Modifiers::CTRL)` matches
    /// Ctrl+Shift+= on a layout where that types a plus.
    pub fn is(&self, key: Key, modifiers: Modifiers) -> bool {
        if !self.pressed {
            return false;
        }
        let same_key = match (&self.key, &key) {
            (Key::Character(a), Key::Character(b)) => a.to_lowercase() == b.to_lowercase(),
            (a, b) => a == b,
        };
        let shift_matters = !matches!(key, Key::Character(_));
        same_key
            && self.modifiers.ctrl == modifiers.ctrl
            && self.modifiers.alt == modifiers.alt
            && self.modifiers.meta == modifiers.meta
            && (!shift_matters || self.modifiers.shift == modifiers.shift)
    }

    /// Whether this is a press (a repeat counts) of the physical key `code` with exactly
    /// `modifiers` held, whatever the keyboard layout.
    pub fn is_code(&self, code: KeyCode, modifiers: Modifiers) -> bool {
        self.pressed && self.physical_key == PhysicalKey::Code(code) && self.modifiers == modifiers
    }
}

/// A button of a mouse or other pointing device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// The left button, a touch, or a pen tip.
    Primary,
    /// The right button.
    Secondary,
    /// The wheel button.
    Middle,
    /// The back thumb button.
    Back,
    /// The forward thumb button.
    Forward,
    /// Any other button, by the platform's number.
    Other(u16),
}

/// What happened to a pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    /// The button went down on the area.
    Press,
    /// The pointer moved with the button held, after it went down on the area.
    /// Also delivered outside the area.
    Drag,
    /// The button was released, wherever the pointer is. Not all releases are clicks.
    Release,
    /// The button went down and up on the same area.
    Click,
}

/// A pointer button changing state on an area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    /// Which button.
    pub button: PointerButton,
    /// What happened to it.
    pub phase: PointerPhase,
    /// Where the pointer is, relative to the area's top-left corner.
    pub position: rustle_geometry::Offset,
    /// Where the pointer is in the window, in logical pixels.
    pub window_position: rustle_geometry::Offset,
    /// The modifier keys held at the time.
    pub modifiers: Modifiers,
    /// 1 for a single press, 2 for the second in quick succession, 3 for the third.
    pub click_count: u32,
}

/// A primary-button press on an area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressEvent {
    /// Where the pointer is, relative to the area's top-left corner.
    pub position: rustle_geometry::Offset,
    /// 1 for a single press, 2 for the second in quick succession, 3 for the third.
    pub click_count: u32,
    /// The button that went down: always [`PointerButton::Primary`] for [`Modifier::on_press`](crate::Modifier::on_press).
    pub button: PointerButton,
    /// The modifier keys held when it went down.
    pub modifiers: Modifiers,
}

/// An input method's composition in progress, such as the text of a pending Japanese
/// or Chinese word before it is committed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Preedit {
    /// The composition text, shown inline and underlined.
    pub text: String,
    /// The selected part or caret within `text`, as byte offsets. `None` hides the caret.
    pub cursor: Option<(usize, usize)>,
}

/// Handles what was dropped on an area: the payload of the drag source it came from.
pub type DropHandler = Rc<dyn Fn(&dyn std::any::Any)>;

/// Handles a key event and says whether it did, so unhandled keys can do their default
/// or bubble to the next handler up.
pub type KeyHandler = Rc<dyn Fn(&KeyEvent) -> bool>;

thread_local! {
    static NEXT_FOCUS_ID: Cell<u64> = const { Cell::new(1) };
    static FOCUS_REQUEST: RefCell<Option<Focus>> = const { RefCell::new(None) };
    static FOCUS_RELEASE: Cell<Option<u64>> = const { Cell::new(None) };
    static CLIPBOARD: RefCell<Option<Rc<dyn Clipboard>>> = const { RefCell::new(None) };
}

/// Identifies something that can have keyboard focus, and tells whether it has it.
#[derive(Clone)]
pub struct Focus {
    id: u64,
    focused: State<bool>,
    preedit: State<Option<Preedit>>,
}

impl Focus {
    /// A focus that belongs to no composition, as Compose's `FocusRequester()` made outside
    /// one: a state holder keeps it and hands it to the composable that takes focus with
    /// it, so it can ask for focus, and read whether it has it, before that composable is
    /// composed and after it has left.
    pub fn new() -> Self {
        Self {
            id: NEXT_FOCUS_ID.with(|next| next.replace(next.get() + 1)),
            focused: mutable_state(false),
            preedit: mutable_state(None),
        }
    }

    /// A number that is the same for as long as the focus is remembered.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Whether this has keyboard focus now.
    pub fn is_focused(&self) -> bool {
        self.focused.get()
    }

    /// Records whether this has focus. Called by the host when focus moves.
    pub fn set_focused(&self, focused: bool) {
        if self.focused.get() != focused {
            self.focused.set(focused);
        }
    }

    /// The input method composition in progress for this focus, if any. Text fields draw it inline.
    pub fn preedit(&self) -> Option<Preedit> {
        self.preedit.get()
    }

    /// Sets the composition in progress. Called by the host on input method preedit events.
    pub fn set_preedit(&self, preedit: Option<Preedit>) {
        if self.preedit.with(|current| *current != preedit) {
            self.preedit.set(preedit);
        }
    }

    /// Asks the host to give this focus at its next opportunity.
    pub fn request_focus(&self) {
        FOCUS_REQUEST.with(|request| *request.borrow_mut() = Some(self.clone()));
    }

    /// Asks the host to take focus away from this at its next opportunity, if it has it
    /// then, so nothing has focus: Compose's `FocusRequester.freeFocus`. A request for
    /// focus made after this one, for this or anything else, wins over it.
    pub fn free_focus(&self) {
        FOCUS_RELEASE.with(|release| release.set(Some(self.id)));
        FOCUS_REQUEST.with(|request| {
            let mut request = request.borrow_mut();
            if request.as_ref().is_some_and(|asked| asked.id == self.id) {
                *request = None;
            }
        });
    }
}

impl Default for Focus {
    /// A focus that belongs to no composition; see [`Focus::new`].
    fn default() -> Self {
        Self::new()
    }
}

/// The focus the application asked to free with [`Focus::free_focus`], by its
/// [`id`](Focus::id), if it asked.
pub fn take_focus_release() -> Option<u64> {
    FOCUS_RELEASE.with(Cell::take)
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
        preedit: remember(|| None),
    })
}

/// A picture on the clipboard: its size in pixels and its pixels as RGBA rows, top first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardImage {
    /// How many pixels across it is.
    pub width: u32,
    /// How many pixels down it is.
    pub height: u32,
    /// Four bytes per pixel, red, green, blue and alpha, row after row.
    pub rgba: Vec<u8>,
}

/// Reads and writes the system clipboard as text, and reads files and pictures copied in other programs.
pub trait Clipboard {
    /// The text on the clipboard, if there is any.
    fn text(&self) -> Option<String>;
    /// Puts `text` on the clipboard.
    fn set_text(&self, text: &str);
    /// The files copied in a file manager, if the clipboard holds a list of them.
    fn files(&self) -> Vec<std::path::PathBuf> {
        Vec::new()
    }
    /// The picture on the clipboard, if it holds one.
    fn image(&self) -> Option<ClipboardImage> {
        None
    }
}

/// Installs the clipboard text editing uses. A window or embedded interface installs the
/// system clipboard when it opens, unless one was installed before, so a program or a test
/// that installs its own first, such as a fake kept in memory, keeps it.
pub fn set_clipboard(clipboard: Rc<dyn Clipboard>) {
    CLIPBOARD.with(|current| *current.borrow_mut() = Some(clipboard));
}

/// The installed clipboard, if the host provided one.
pub fn clipboard() -> Option<Rc<dyn Clipboard>> {
    CLIPBOARD.with(|current| current.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A focus made outside any composition has its own id, starts without focus, can be
    /// asked for and freed, and tells what the host records about it.
    #[test]
    fn a_focus_made_outside_composition_asks_and_tells() {
        let (first, second) = (Focus::new(), Focus::default());
        assert_ne!(first.id(), second.id());
        assert!(!first.is_focused());
        first.request_focus();
        assert_eq!(
            take_focus_request().map(|asked| asked.id()),
            Some(first.id())
        );
        first.set_focused(true);
        assert!(first.is_focused() && !second.is_focused());
        first.free_focus();
        assert_eq!(take_focus_release(), Some(first.id()));
    }

    fn event(key: Key, code: KeyCode, modifiers: Modifiers) -> KeyEvent {
        KeyEvent {
            physical_key: PhysicalKey::Code(code),
            ..KeyEvent::press(key, modifiers)
        }
    }

    /// The code point is the typed text with Shift applied, the key's own text when the
    /// key typed none, and nothing for keys that type nothing.
    #[test]
    fn the_code_point_is_what_the_key_typed() {
        let shifted = KeyEvent {
            text: Some("A".into()),
            ..KeyEvent::press(Key::Character("a".into()), Modifiers::SHIFT)
        };
        assert_eq!(shifted.code_point(), Some('A'));
        let chord = KeyEvent {
            text: Some("\u{1}".into()),
            ..KeyEvent::press(Key::Character("a".into()), Modifiers::CTRL)
        };
        assert_eq!(chord.code_point(), Some('a'));
        assert_eq!(
            KeyEvent::press(Key::ArrowUp, Modifiers::NONE).code_point(),
            None
        );
        assert_eq!(
            KeyEvent::typed("é", Modifiers::NONE).code_point(),
            Some('é')
        );
    }

    #[test]
    fn logical_shortcuts_ignore_letter_case_and_need_exact_modifiers() {
        let save = event(Key::Character("S".into()), KeyCode::KeyS, Modifiers::CTRL);
        assert!(save.is(Key::Character("s".into()), Modifiers::CTRL));
        assert!(!save.is(Key::Character("s".into()), Modifiers::NONE));
        assert!(!save.is(Key::Character("s".into()), Modifiers::CTRL.with_alt()));
        let with_shift = event(Key::F(5), KeyCode::F5, Modifiers::SHIFT);
        assert!(with_shift.is(Key::F(5), Modifiers::SHIFT));
        assert!(!with_shift.is(Key::F(5), Modifiers::NONE));
    }

    #[test]
    fn physical_shortcuts_follow_the_key_not_the_layout() {
        // The key in the Q position of a French layout types "a".
        let key = event(Key::Character("a".into()), KeyCode::KeyQ, Modifiers::NONE);
        assert!(key.is_code(KeyCode::KeyQ, Modifiers::NONE));
        assert!(!key.is_code(KeyCode::KeyA, Modifiers::NONE));
    }

    #[test]
    fn releases_do_not_match_shortcuts() {
        let mut release = event(Key::Escape, KeyCode::Escape, Modifiers::NONE);
        release.pressed = false;
        assert!(!release.is(Key::Escape, Modifiers::NONE));
        assert!(!release.is_code(KeyCode::Escape, Modifiers::NONE));
    }

    #[test]
    fn key_codes_round_trip_through_their_names() {
        for code in [
            KeyCode::KeyA,
            KeyCode::F24,
            KeyCode::Insert,
            KeyCode::NumpadEnter,
        ] {
            assert_eq!(KeyCode::from_name(code.name()), Some(code));
        }
        assert_eq!(KeyCode::from_name("Nope"), None);
    }
}
