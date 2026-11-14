//! Semantics: what an area means, for assistive technology such as screen readers.

use std::rc::Rc;

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

/// How urgently assistive technology announces a change to an area's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveRegion {
    /// Announced when the user is idle.
    Polite,
    /// Announced at once, interrupting what is being read.
    Assertive,
}

/// A direction assistive technology asks a scrolling area to move in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    /// Show what lies above.
    Up,
    /// Show what lies below.
    Down,
    /// Show what lies to the left.
    Left,
    /// Show what lies to the right.
    Right,
}

/// An action beyond click and focus that assistive technology can offer on an area, such as
/// "Archive" on a mail row.
#[derive(Clone)]
pub struct CustomAction {
    /// What the action is called when it is listed to the user.
    pub label: String,
    /// Runs when the user picks the action.
    pub handler: Rc<dyn Fn()>,
}

/// A description of an area for assistive technology.
///
/// Descriptions nest: an area described inside another is its child in the tree assistive
/// technology sees. [`merging_descendants`](Self::merging_descendants) folds the children
/// into the parent and [`clearing_descendants`](Self::clearing_descendants) drops them.
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
    /// Whether everything described inside it is dropped, so only this description is seen.
    pub clear_descendants: bool,
    /// The heading level (1 is the highest), if the area is a heading.
    pub heading: Option<u8>,
    /// Whether changes to the area are announced as they happen, and how urgently.
    pub live_region: Option<LiveRegion>,
    /// Extra actions offered on the area.
    pub custom_actions: Vec<CustomAction>,
    /// Runs when assistive technology asks the area to scroll.
    pub on_scroll: Option<Rc<dyn Fn(ScrollDirection)>>,
    /// Runs when assistive technology sets the area's value, such as a field's text.
    pub on_set_value: Option<Rc<dyn Fn(String)>>,
    /// The selected range of `value`, as character indices (anchor, focus). Equal ends
    /// are a caret.
    pub text_selection: Option<(usize, usize)>,
    /// Runs when assistive technology moves the selection in `value`, with character
    /// indices (anchor, focus).
    pub on_set_text_selection: Option<Rc<dyn Fn(usize, usize)>>,
    /// Where the area comes in reading order among its siblings. Lower is earlier; areas
    /// without one count as 0 and keep their paint order.
    pub traversal_index: Option<f32>,
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
            clear_descendants: false,
            heading: None,
            live_region: None,
            custom_actions: Vec::new(),
            on_scroll: None,
            on_set_value: None,
            text_selection: None,
            on_set_text_selection: None,
            traversal_index: None,
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

    /// Describes the text inside as part of this, and drops every description inside, so
    /// only this one is seen.
    pub fn clearing_descendants(mut self) -> Self {
        self.clear_descendants = true;
        self
    }

    /// Marks the area as a heading of `level`, 1 being the highest.
    pub fn heading(mut self, level: u8) -> Self {
        self.heading = Some(level.max(1));
        self
    }

    /// Announces changes to the area as they happen.
    pub fn live_region(mut self, live: LiveRegion) -> Self {
        self.live_region = Some(live);
        self
    }

    /// Offers an extra action called `label` that runs `handler`.
    pub fn custom_action(mut self, label: impl Into<String>, handler: impl Fn() + 'static) -> Self {
        self.custom_actions.push(CustomAction {
            label: label.into(),
            handler: Rc::new(handler),
        });
        self
    }

    /// Runs `handler` when assistive technology asks the area to scroll.
    pub fn on_scroll(mut self, handler: impl Fn(ScrollDirection) + 'static) -> Self {
        self.on_scroll = Some(Rc::new(handler));
        self
    }

    /// Runs `handler` with the new text or value when assistive technology sets it.
    pub fn on_set_value(mut self, handler: impl Fn(String) + 'static) -> Self {
        self.on_set_value = Some(Rc::new(handler));
        self
    }

    /// Says which characters of the value are selected, by index: `anchor` is where the
    /// selection started and `focus` is the caret end.
    pub fn text_selection(mut self, anchor: usize, focus: usize) -> Self {
        self.text_selection = Some((anchor, focus));
        self
    }

    /// Runs `handler` with the anchor and focus character indices when assistive
    /// technology moves the selection.
    pub fn on_set_text_selection(mut self, handler: impl Fn(usize, usize) + 'static) -> Self {
        self.on_set_text_selection = Some(Rc::new(handler));
        self
    }

    /// Sets where the area comes in reading order among its siblings.
    pub fn traversal_index(mut self, index: f32) -> Self {
        self.traversal_index = Some(index);
        self
    }
}
