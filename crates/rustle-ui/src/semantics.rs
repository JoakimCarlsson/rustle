//! Semantics: what an area means, for assistive technology such as screen readers.

use crate::input::Focus;

/// What kind of thing an area is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Role {
    /// Static text.
    Label,
    Button,
    CheckBox,
    Switch,
    RadioButton,
    Slider,
    TextInput,
    ProgressBar,
    Image,
    Dialog,
    Tab,
    /// A container, such as a group of related controls.
    Group,
}

/// A description of an area for assistive technology.
#[derive(Clone)]
pub struct Semantics {
    pub role: Role,
    /// What it is called, such as a button's caption.
    pub label: Option<String>,
    /// What it currently holds, such as a slider's position or a field's text.
    pub value: Option<String>,
    /// Whether it is on, for checkboxes, switches and radio buttons.
    pub toggled: Option<bool>,
    /// Its focus, so assistive technology can tell when it is focused.
    pub focus: Option<Focus>,
    /// Whether the text inside it is part of this description instead of being
    /// described on its own. A button's caption is read once, as the button.
    pub merge_descendants: bool,
}

impl Semantics {
    pub fn new(role: Role) -> Self {
        Self {
            role,
            label: None,
            value: None,
            toggled: None,
            focus: None,
            merge_descendants: false,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub fn toggled(mut self, toggled: bool) -> Self {
        self.toggled = Some(toggled);
        self
    }

    pub fn focus(mut self, focus: &Focus) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// Describes the text inside as part of this, not separately.
    pub fn merging_descendants(mut self) -> Self {
        self.merge_descendants = true;
        self
    }
}
