//! What shapes a text field's input and output, and how its keyboard behaves: Compose's
//! `InputTransformation`, `OutputTransformation`, `TextObfuscationMode`, `KeyboardOptions`,
//! `KeyboardActionHandler` and `TextFieldLineLimits`.

use std::rc::Rc;

use crate::text_field::state::{ChangeList, TextFieldBuffer, TextFieldValue};

/// Changes or refuses what the user types into a field before it reaches the field's
/// state, Compose's `InputTransformation`. It sees each change the user makes, typing,
/// deleting, pasting or an input method's commit, in a buffer that holds the change; it
/// may change the buffer further or take the change back with
/// [`TextFieldBuffer::revert_all_changes`]. Changes the program makes with
/// [`TextFieldState::edit`](crate::TextFieldState::edit) do not pass through it.
pub trait InputTransformation {
    /// Changes or refuses the change `buffer` holds.
    fn transform_input(&self, buffer: &mut TextFieldBuffer);
}

impl<F: Fn(&mut TextFieldBuffer)> InputTransformation for F {
    /// Runs the closure on the buffer.
    fn transform_input(&self, buffer: &mut TextFieldBuffer) {
        self(buffer);
    }
}

/// Refuses a change that would make the text longer than a number of characters,
/// Compose's `InputTransformation.maxLength`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxLength(pub usize);

impl InputTransformation for MaxLength {
    /// Takes the change back when the text would be longer than allowed.
    fn transform_input(&self, buffer: &mut TextFieldBuffer) {
        if buffer.length() > self.0 {
            buffer.revert_all_changes();
        }
    }
}

/// Decides the text from the text before a change and the text the change proposes,
/// Compose's `InputTransformation.byValue`: what the closure returns becomes the text,
/// and when it differs from the proposal the caret goes to its end.
pub struct ByValue<F>(pub F);

impl<F: Fn(&str, &str) -> String> InputTransformation for ByValue<F> {
    /// Replaces the proposed text with what the closure decides.
    fn transform_input(&self, buffer: &mut TextFieldBuffer) {
        let proposed = buffer.as_str().to_string();
        let decided = (self.0)(buffer.original_text(), &proposed);
        if decided != proposed {
            buffer.replace(0, buffer.length(), &decided);
            buffer.place_cursor_at_end();
        }
    }
}

/// One transformation and then another, Compose's `InputTransformation.then`.
pub struct Then(
    pub Rc<dyn InputTransformation>,
    pub Rc<dyn InputTransformation>,
);

impl InputTransformation for Then {
    /// Runs the first, then the second unless the first took the change back.
    fn transform_input(&self, buffer: &mut TextFieldBuffer) {
        self.0.transform_input(buffer);
        if buffer.as_str() != buffer.original_text() {
            self.1.transform_input(buffer);
        }
    }
}

/// Changes how a field's text is shown without changing the text, Compose's
/// `OutputTransformation`: it edits a buffer holding the text, and the caret,
/// selection and pointer move through what it shows as the changes it made say.
/// A place inside text it put in stands for the end of what that text replaced.
pub trait OutputTransformation {
    /// Changes the text `buffer` shows.
    fn transform_output(&self, buffer: &mut TextFieldBuffer);
}

impl<F: Fn(&mut TextFieldBuffer)> OutputTransformation for F {
    /// Runs the closure on the buffer.
    fn transform_output(&self, buffer: &mut TextFieldBuffer) {
        self(buffer);
    }
}

/// How a secure field shows what is typed, Compose's `TextObfuscationMode`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextObfuscationMode {
    /// Every character is shown as the obfuscation character.
    #[default]
    Hidden,
    /// The text is shown as it is.
    Visible,
}

/// The character a secure field shows for each hidden one by default, Compose's
/// `DefaultObfuscationCharacter`.
pub const DEFAULT_OBFUSCATION_CHARACTER: char = '\u{2022}';

/// What a field shows for a value: the text after its output transformation, and the
/// changes that map places between the two.
#[derive(Debug, Clone, Default)]
pub(crate) struct Shown {
    /// The text shown.
    pub(crate) text: String,
    /// The changes from the value's text to the text shown, empty when it is shown as it is.
    pub(crate) changes: Option<ChangeList>,
}

impl Shown {
    /// What `value` shows: each character as `obfuscation` when it is given, one for one
    /// so places are the same in both, otherwise the text through `transformation`, or the
    /// text itself without one.
    pub(crate) fn of(
        value: &TextFieldValue,
        transformation: Option<&Rc<dyn OutputTransformation>>,
        obfuscation: Option<char>,
    ) -> Self {
        if let Some(character) = obfuscation {
            return Self {
                text: std::iter::repeat_n(character, value.length()).collect(),
                changes: None,
            };
        }
        let Some(transformation) = transformation else {
            return Self {
                text: value.text.clone(),
                changes: None,
            };
        };
        let mut buffer = TextFieldBuffer::new(value.clone());
        transformation.transform_output(&mut buffer);
        let changes = buffer.changes().clone();
        Self {
            text: buffer.as_str().to_string(),
            changes: (changes.change_count() > 0).then_some(changes),
        }
    }

    /// Where character `index` of the value's text is shown.
    pub(crate) fn to_shown(&self, index: usize) -> usize {
        self.changes
            .as_ref()
            .map_or(index, |changes| changes.map_forward(index))
    }

    /// The character of the value's text that place `index` of what is shown stands for.
    pub(crate) fn to_value(&self, index: usize) -> usize {
        self.changes
            .as_ref()
            .map_or(index, |changes| changes.map_back(index))
    }
}

/// The action an input method offers on its key for a field, Compose's `ImeAction`. On a
/// desktop keyboard it is what Enter does in a single-line field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ImeAction {
    /// The platform's default: submitting a single line.
    #[default]
    Default,
    /// No action.
    None,
    /// Go to what was typed, such as an address.
    Go,
    /// Search for what was typed.
    Search,
    /// Send what was typed.
    Send,
    /// Move to the field before.
    Previous,
    /// Move to the field after.
    Next,
    /// Finish typing.
    Done,
}

/// What kind of text a field takes, Compose's `KeyboardType`: a hint for on-screen
/// keyboards, which a desktop keyboard does not change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KeyboardType {
    /// Any text.
    #[default]
    Text,
    /// Plain characters, with no suggestions.
    Ascii,
    /// A number.
    Number,
    /// A telephone number.
    Phone,
    /// A web address.
    Uri,
    /// An email address.
    Email,
    /// A password.
    Password,
    /// A number password.
    NumberPassword,
    /// A decimal number.
    Decimal,
}

/// How the keyboard behaves for a field, Compose's `KeyboardOptions`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyboardOptions {
    /// What kind of text it takes.
    pub keyboard_type: KeyboardType,
    /// What its action key does.
    pub ime_action: ImeAction,
}

/// What a field does when its action is asked for, Compose's `KeyboardActionHandler`:
/// it is given the default action, which it may run or skip. Pressing Enter in a single
/// line asks for it; the default runs [`TextFieldState::on_submit`](crate::TextFieldState::on_submit)'s handler.
pub type KeyboardActionHandler = Rc<dyn Fn(&dyn Fn())>;

/// How many lines a field shows, Compose's `TextFieldLineLimits`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextFieldLineLimits {
    /// One line that scrolls sideways; Enter does not break it.
    #[default]
    SingleLine,
    /// Several lines that wrap at the field's width, at least `min_height_in_lines` tall
    /// and at most `max_height_in_lines`, scrolling down beyond that.
    MultiLine {
        /// The fewest lines tall it is.
        min_height_in_lines: usize,
        /// The most lines tall it is.
        max_height_in_lines: usize,
    },
}

impl TextFieldLineLimits {
    /// Several lines with no bounds on how tall the field grows, Compose's `MultiLine()`.
    pub const MULTI_LINE: Self = Self::MultiLine {
        min_height_in_lines: 1,
        max_height_in_lines: usize::MAX,
    };

    /// Whether the field holds several lines.
    pub fn is_multiline(&self) -> bool {
        matches!(self, Self::MultiLine { .. })
    }
}

/// Tests of the module.
#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer of `original` with `edit` made in it.
    fn changed(original: &str, edit: impl FnOnce(&mut TextFieldBuffer)) -> TextFieldBuffer {
        let mut buffer = TextFieldBuffer::new(TextFieldValue {
            text: original.into(),
            cursor: original.chars().count(),
            anchor: original.chars().count(),
        });
        edit(&mut buffer);
        buffer
    }

    /// A length limit refuses a change that makes the text too long.
    #[test]
    fn max_length_refuses_a_change_that_is_too_long() {
        let mut buffer = changed("abc", |buffer| buffer.append("de"));
        MaxLength(4).transform_input(&mut buffer);
        assert_eq!(buffer.as_str(), "abc");
        let mut buffer = changed("abc", |buffer| buffer.append("d"));
        MaxLength(4).transform_input(&mut buffer);
        assert_eq!(buffer.as_str(), "abcd");
    }

    /// A transformation by value decides the text, and chained transformations run in turn.
    #[test]
    fn by_value_decides_the_text_and_then_chains() {
        let upper: Rc<dyn InputTransformation> =
            Rc::new(ByValue(|_: &str, proposed: &str| proposed.to_uppercase()));
        let short: Rc<dyn InputTransformation> = Rc::new(MaxLength(3));
        let both = Then(upper, short);
        let mut buffer = changed("ab", |buffer| buffer.append("c"));
        both.transform_input(&mut buffer);
        assert_eq!(buffer.as_str(), "ABC");
        let mut buffer = changed("ab", |buffer| buffer.append("cd"));
        both.transform_input(&mut buffer);
        assert_eq!(buffer.as_str(), "ab");
    }

    /// An output transformation maps places through what it put in, and obfuscation maps one for one.
    #[test]
    fn output_maps_places_through_what_it_inserted() {
        let value = TextFieldValue {
            text: "1234567".into(),
            cursor: 4,
            anchor: 4,
        };
        let grouped: Rc<dyn OutputTransformation> = Rc::new(|buffer: &mut TextFieldBuffer| {
            buffer.insert(3, "-");
        });
        let shown = Shown::of(&value, Some(&grouped), None);
        assert_eq!(shown.text, "123-4567");
        assert_eq!(shown.to_shown(4), 5);
        assert_eq!(shown.to_shown(2), 2);
        assert_eq!(shown.to_value(5), 4);
        assert_eq!(shown.to_value(4), 3);
        let shown = Shown::of(&value, Some(&grouped), Some('*'));
        assert_eq!(shown.text, "*******");
        assert_eq!(shown.to_value(3), 3);
        assert_eq!(shown.to_shown(4), 4);
    }
}
