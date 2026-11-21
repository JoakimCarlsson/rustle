//! What a text field offers a context menu: Compose's `TextContextMenu`, its `TextManager`
//! and `LocalTextContextMenu`. A field hands the menu provided around it the operations it
//! can do on its text now, and the menu decides how to offer them.

use std::rc::Rc;

use rustle_runtime::CompositionLocal;

/// The operations a text field can do on its text now, for a context menu to offer:
/// Compose's `TextContextMenu.TextManager`. An operation the field cannot do now is `None`.
#[derive(Clone, Default)]
pub struct TextManager {
    /// The text selected, empty when nothing is or the field keeps its text secret.
    pub selected_text: String,
    /// Puts the selection on the clipboard and takes it out of the field; `None` while
    /// nothing is selected or the field is read-only or secure.
    pub cut: Option<Rc<dyn Fn()>>,
    /// Puts the selection on the clipboard; `None` while nothing is selected or the field is
    /// secure.
    pub copy: Option<Rc<dyn Fn()>>,
    /// Puts the text on the clipboard in place of the selection, as the field takes typed
    /// text, and does nothing when the clipboard holds no text; `None` while the field is
    /// read-only.
    pub paste: Option<Rc<dyn Fn()>>,
    /// Selects the whole text; `None` while it is all selected.
    pub select_all: Option<Rc<dyn Fn()>>,
}

impl std::fmt::Debug for TextManager {
    /// The selected text and which operations the field can do.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextManager")
            .field("selected_text", &self.selected_text)
            .field("cut", &self.cut.is_some())
            .field("copy", &self.copy.is_some())
            .field("paste", &self.paste.is_some())
            .field("select_all", &self.select_all.is_some())
            .finish()
    }
}

/// What a [`TextContextMenu`] does with a field: emits the field's content, given what the
/// field can do on its text.
type Area = Rc<dyn Fn(TextManager, &mut dyn FnMut())>;

/// What a context menu does with the text fields composed inside it: Compose's
/// `TextContextMenu`. Each field calls [`area`](Self::area) with its [`TextManager`] and
/// its content, and the menu emits the content where it offers the operations, such as
/// inside an area a secondary press opens a menu over.
#[derive(Clone)]
pub struct TextContextMenu {
    /// Emits a field's content with its operations offered.
    area: Area,
}

impl TextContextMenu {
    /// A menu whose `area` is given each field's operations and content, and emits the content
    /// once.
    pub fn new(area: impl Fn(TextManager, &mut dyn FnMut()) + 'static) -> Self {
        Self {
            area: Rc::new(area),
        }
    }

    /// Emits `content`, a field's, with the operations of `text_manager` offered: Compose's
    /// `TextContextMenu.Area`.
    pub fn area(&self, text_manager: TextManager, content: &mut dyn FnMut()) {
        (self.area)(text_manager, content);
    }
}

impl Default for TextContextMenu {
    /// A menu that offers nothing and emits each field's content as it is.
    fn default() -> Self {
        Self::new(|_, content| content())
    }
}

impl PartialEq for TextContextMenu {
    /// Whether both are the same menu.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.area, &other.area)
    }
}

impl std::fmt::Debug for TextContextMenu {
    /// Names the type only.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("TextContextMenu").finish()
    }
}

/// The [`TextContextMenu`] the text fields composed below give their operations to:
/// Compose's `LocalTextContextMenu`. The default offers nothing.
pub static LOCAL_TEXT_CONTEXT_MENU: CompositionLocal<TextContextMenu> =
    CompositionLocal::new(TextContextMenu::default);
