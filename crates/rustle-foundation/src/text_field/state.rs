//! What a text field holds: its value, the buffer edits are made in, the changes an
//! edit made and the history undo walks, as Compose's `TextFieldState`,
//! `TextFieldBuffer` and `UndoState` have them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustle_runtime::{State, mutable_state};
use rustle_text::Affinity;

/// A range of characters, Compose's `TextRange`: `start` is where it begins and `end`
/// where it ends, which may come first for a selection made backwards.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TextRange {
    /// Where the range begins: the anchor of a selection.
    pub start: usize,
    /// Where the range ends: the caret of a selection.
    pub end: usize,
}

impl TextRange {
    /// The range from `start` to `end`.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The empty range at `index`, a caret there.
    pub const fn collapsed_at(index: usize) -> Self {
        Self::new(index, index)
    }

    /// The lesser end.
    pub fn min(&self) -> usize {
        self.start.min(self.end)
    }

    /// The greater end.
    pub fn max(&self) -> usize {
        self.start.max(self.end)
    }

    /// Whether the range is empty.
    pub fn collapsed(&self) -> bool {
        self.start == self.end
    }

    /// How many characters it covers.
    pub fn length(&self) -> usize {
        self.max() - self.min()
    }
}

/// The text of a field with its caret and selection, counted in characters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFieldValue {
    /// The text.
    pub text: String,
    /// Where the caret is, from 0 to the number of characters.
    pub cursor: usize,
    /// The other end of the selection. Equal to `cursor` when nothing is selected.
    pub anchor: usize,
}

impl TextFieldValue {
    /// The selected characters as a start and end, or `None` if nothing is selected.
    pub fn selection(&self) -> Option<(usize, usize)> {
        (self.cursor != self.anchor)
            .then(|| (self.cursor.min(self.anchor), self.cursor.max(self.anchor)))
    }

    /// The number of characters.
    pub(crate) fn length(&self) -> usize {
        self.text.chars().count()
    }

    /// The byte character `index` starts at, or the length for the end.
    pub(crate) fn byte(&self, index: usize) -> usize {
        byte_of(&self.text, index)
    }

    /// The selected text, or `None` if nothing is selected.
    pub fn selected_text(&self) -> Option<&str> {
        let (start, end) = self.selection()?;
        Some(&self.text[self.byte(start)..self.byte(end)])
    }

    /// Where the line the caret is on starts, as a character index.
    pub(crate) fn line_start(&self) -> usize {
        line_start_of(&self.text, self.cursor)
    }

    /// Where the line the caret is on ends, as a character index.
    pub(crate) fn line_end(&self) -> usize {
        line_end_of(&self.text, self.cursor)
    }

    /// Moves the caret to `to`, dragging the selection along if `extend` is set.
    pub(crate) fn move_to(&mut self, to: usize, extend: bool) {
        self.cursor = to.min(self.length());
        if !extend {
            self.anchor = self.cursor;
        }
    }

    /// The selection as a range from the anchor to the caret.
    pub fn selection_range(&self) -> TextRange {
        TextRange::new(self.anchor, self.cursor)
    }
}

/// `text` with the control characters a field does not hold taken out: all of them in a
/// single line, all but line breaks and tabs in several.
pub(crate) fn typeable(text: &str, multiline: bool) -> String {
    text.chars()
        .filter(|c| !c.is_control() || (multiline && (*c == '\n' || *c == '\t')))
        .collect()
}

/// The byte character `index` of `text` starts at, or the length for the end.
pub(crate) fn byte_of(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// The character index of byte `byte` of `text`.
pub(crate) fn char_index(text: &str, byte: usize) -> usize {
    text.get(..byte)
        .map_or_else(|| text.chars().count(), |s| s.chars().count())
}

/// Where the line holding character `index` of `text` starts.
pub(crate) fn line_start_of(text: &str, index: usize) -> usize {
    let before: Vec<char> = text.chars().take(index).collect();
    before
        .iter()
        .rposition(|c| *c == '\n')
        .map_or(0, |at| at + 1)
}

/// Where the line holding character `index` of `text` ends, before its line break.
pub(crate) fn line_end_of(text: &str, index: usize) -> usize {
    text.chars()
        .enumerate()
        .skip(index)
        .find(|(_, c)| *c == '\n')
        .map_or_else(|| text.chars().count(), |(at, _)| at)
}

/// Whether `c` is part of a word: a letter, a digit or an underscore.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word around character `index` of `text`: the run of word characters it touches,
/// or the single character after it otherwise.
pub(crate) fn word_around(text: &str, index: usize) -> TextRange {
    let characters: Vec<char> = text.chars().collect();
    let index = index.min(characters.len());
    let mut start = index;
    while start > 0 && is_word(characters[start - 1]) {
        start -= 1;
    }
    let mut end = index;
    while end < characters.len() && is_word(characters[end]) {
        end += 1;
    }
    if start == end && end < characters.len() {
        end += 1;
    }
    TextRange::new(start, end)
}

/// The line around character `index` of `text`, without its line break.
pub(crate) fn line_around(text: &str, index: usize) -> TextRange {
    TextRange::new(line_start_of(text, index), line_end_of(text, index))
}

/// Where a move one word back from character `index` of `text` lands: past the spaces
/// before it, then to the start of the word or run of punctuation before those.
pub(crate) fn previous_word(text: &str, index: usize) -> usize {
    let characters: Vec<char> = text.chars().collect();
    let mut at = index.min(characters.len());
    while at > 0 && characters[at - 1].is_whitespace() {
        at -= 1;
    }
    let word = at > 0 && is_word(characters[at - 1]);
    while at > 0 && !characters[at - 1].is_whitespace() && is_word(characters[at - 1]) == word {
        at -= 1;
    }
    at
}

/// Where a move one word on from character `index` of `text` lands: past the spaces
/// after it, then to the end of the word or run of punctuation after those.
pub(crate) fn next_word(text: &str, index: usize) -> usize {
    let characters: Vec<char> = text.chars().collect();
    let mut at = index.min(characters.len());
    while at < characters.len() && characters[at].is_whitespace() {
        at += 1;
    }
    let word = at < characters.len() && is_word(characters[at]);
    while at < characters.len()
        && !characters[at].is_whitespace()
        && is_word(characters[at]) == word
    {
        at += 1;
    }
    at
}

/// One change an edit made: the characters it replaced in the text before the edit and
/// the ones that took their place in the text after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Change {
    /// The characters replaced, in the text before the edit.
    original: TextRange,
    /// The characters that replaced them, in the text after the edit.
    current: TextRange,
}

/// The changes an edit made to a [`TextFieldBuffer`], Compose's `TextFieldBuffer.ChangeList`:
/// each a range of the text before the edit and the range of the text now that took its
/// place, ordered and never overlapping. Changes that touch are merged into one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeList {
    /// The changes, in order through the text.
    changes: Vec<Change>,
}

impl ChangeList {
    /// How many changes there are.
    pub fn change_count(&self) -> usize {
        self.changes.len()
    }

    /// The range of the text now that change `index` put in.
    ///
    /// # Panics
    ///
    /// When there are not more than `index` changes.
    pub fn get_range(&self, index: usize) -> TextRange {
        self.changes[index].current
    }

    /// The range of the text before the edit that change `index` replaced.
    ///
    /// # Panics
    ///
    /// When there are not more than `index` changes.
    pub fn get_original_range(&self, index: usize) -> TextRange {
        self.changes[index].original
    }

    /// Records that characters `start..end` of the text now became `length` characters.
    fn record(&mut self, start: usize, end: usize, length: usize) {
        let touching: Vec<usize> = (0..self.changes.len())
            .filter(|&index| {
                let current = self.changes[index].current;
                current.start <= end && current.end >= start
            })
            .collect();
        let delta = length as isize - (end - start) as isize;
        let (original, current_start, current_end) = match (touching.first(), touching.last()) {
            (Some(&first), Some(&last)) => {
                let (first, last) = (self.changes[first], self.changes[last]);
                let original_start = if start < first.current.start {
                    self.to_original(start)
                } else {
                    first.original.start
                };
                let original_end = if end > last.current.end {
                    self.to_original(end)
                } else {
                    last.original.end
                };
                (
                    TextRange::new(original_start, original_end),
                    start.min(first.current.start),
                    end.max(last.current.end),
                )
            }
            _ => (
                TextRange::new(self.to_original(start), self.to_original(end)),
                start,
                end,
            ),
        };
        let merged = Change {
            original,
            current: TextRange::new(
                current_start,
                (current_end as isize + delta).max(current_start as isize) as usize,
            ),
        };
        self.changes
            .retain(|change| !(change.current.start <= end && change.current.end >= start));
        for change in &mut self.changes {
            if change.current.start > end {
                change.current.start = (change.current.start as isize + delta) as usize;
                change.current.end = (change.current.end as isize + delta) as usize;
            }
        }
        let at = self
            .changes
            .iter()
            .position(|change| change.current.start > merged.current.start)
            .unwrap_or(self.changes.len());
        self.changes.insert(at, merged);
    }

    /// The index of the text before the edit that index `current` of the text now, which
    /// lies outside every change, stands for.
    fn to_original(&self, current: usize) -> usize {
        let shift: isize = self
            .changes
            .iter()
            .filter(|change| change.current.end <= current)
            .map(|change| change.current.length() as isize - change.original.length() as isize)
            .sum();
        (current as isize - shift).max(0) as usize
    }

    /// Where index `original` of the text before the edit is in the text now: moved by the
    /// changes before it, and to the end of the change that replaced it when one did.
    pub(crate) fn map_forward(&self, original: usize) -> usize {
        let mut shift: isize = 0;
        for change in &self.changes {
            if original < change.original.start
                || (original == change.original.start && !change.original.collapsed())
            {
                break;
            }
            if original < change.original.end {
                return change.current.end;
            }
            shift += change.current.length() as isize - change.original.length() as isize;
        }
        (original as isize + shift).max(0) as usize
    }

    /// Where index `current` of the text now is in the text before the edit: moved by the
    /// changes before it, and to the end of the range a change replaced when it lies in
    /// what that change put in.
    pub(crate) fn map_back(&self, current: usize) -> usize {
        let mut shift: isize = 0;
        for change in &self.changes {
            if current <= change.current.start {
                break;
            }
            if current < change.current.end {
                return change.original.end;
            }
            shift += change.current.length() as isize - change.original.length() as isize;
        }
        (current as isize - shift).max(0) as usize
    }
}

/// The text and selection of a field being changed, Compose's `TextFieldBuffer`: what
/// [`TextFieldState::edit`] hands its block, and what an
/// [`InputTransformation`](crate::InputTransformation) or
/// [`OutputTransformation`](crate::OutputTransformation) changes. Indices count characters.
#[derive(Debug, Clone)]
pub struct TextFieldBuffer {
    /// The text and selection as the edit has them so far.
    value: TextFieldValue,
    /// The text and selection before the edit.
    original: TextFieldValue,
    /// What the edit changed.
    changes: ChangeList,
}

impl TextFieldBuffer {
    /// A buffer holding `value`, with nothing changed yet.
    pub(crate) fn new(value: TextFieldValue) -> Self {
        Self {
            original: value.clone(),
            value,
            changes: ChangeList::default(),
        }
    }

    /// The value the buffer holds now, giving the buffer up.
    pub(crate) fn into_value(self) -> TextFieldValue {
        self.value
    }

    /// The text as it is now.
    pub fn as_str(&self) -> &str {
        &self.value.text
    }

    /// How many characters the text has now.
    pub fn length(&self) -> usize {
        self.value.length()
    }

    /// The text before the edit.
    pub fn original_text(&self) -> &str {
        &self.original.text
    }

    /// The selection before the edit.
    pub fn original_selection(&self) -> TextRange {
        self.original.selection_range()
    }

    /// The selection now, from its anchor to its caret.
    pub fn selection(&self) -> TextRange {
        self.value.selection_range()
    }

    /// Selects `range`, kept within the text: its `end` is where the caret goes.
    pub fn set_selection(&mut self, range: TextRange) {
        let length = self.length();
        self.value.anchor = range.start.min(length);
        self.value.cursor = range.end.min(length);
    }

    /// Whether some text is selected.
    pub fn has_selection(&self) -> bool {
        !self.selection().collapsed()
    }

    /// What changed since the edit began.
    pub fn changes(&self) -> &ChangeList {
        &self.changes
    }

    /// Puts `text` in place of characters `start..end`. A caret or selection end after
    /// the range moves with the text, one inside it goes to the end of what was put in.
    pub fn replace(&mut self, start: usize, end: usize, text: &str) {
        let length = self.length();
        let (start, end) = (start.min(end).min(length), end.max(start).min(length));
        let (from, to) = (self.value.byte(start), self.value.byte(end));
        self.value.text.replace_range(from..to, text);
        let added = text.chars().count();
        let moved = |index: usize| {
            if index >= end {
                index - (end - start) + added
            } else if index > start {
                start + added
            } else {
                index
            }
        };
        self.value.cursor = moved(self.value.cursor);
        self.value.anchor = moved(self.value.anchor);
        self.changes.record(start, end, added);
    }

    /// Puts `text` before character `index`.
    pub fn insert(&mut self, index: usize, text: &str) {
        self.replace(index, index, text);
    }

    /// Puts `text` at the end.
    pub fn append(&mut self, text: &str) {
        let end = self.length();
        self.replace(end, end, text);
    }

    /// Takes out characters `start..end`.
    pub fn delete(&mut self, start: usize, end: usize) {
        self.replace(start, end, "");
    }

    /// Puts the caret before character `index`, selecting nothing.
    pub fn place_cursor_before_char_at(&mut self, index: usize) {
        self.set_selection(TextRange::collapsed_at(index));
    }

    /// Puts the caret after character `index`, selecting nothing.
    pub fn place_cursor_after_char_at(&mut self, index: usize) {
        self.set_selection(TextRange::collapsed_at(index + 1));
    }

    /// Puts the caret at the end of the text, selecting nothing.
    pub fn place_cursor_at_end(&mut self) {
        self.set_selection(TextRange::collapsed_at(self.length()));
    }

    /// Selects all of the text.
    pub fn select_all(&mut self) {
        self.set_selection(TextRange::new(0, self.length()));
    }

    /// Takes back every change of the edit, text and selection.
    pub fn revert_all_changes(&mut self) {
        self.value = self.original.clone();
        self.changes = ChangeList::default();
    }
}

/// How a change joins the undo history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UndoBehavior {
    /// Typing: joins the step before when that was typing that ended where this begins.
    MergeTyping,
    /// A step of its own.
    NeverMerge,
}

/// One step undo takes back: the value before it and after it.
#[derive(Debug, Clone)]
struct Step {
    /// The value before the step.
    before: TextFieldValue,
    /// The value after the step.
    after: TextFieldValue,
    /// Whether typing may join it.
    typing: bool,
}

/// The steps undo and redo walk.
#[derive(Debug, Default)]
pub(crate) struct History {
    /// The steps that can be undone, the latest last.
    past: Vec<Step>,
    /// The steps undone that can be redone, the latest undone last.
    future: Vec<Step>,
}

/// How many steps undo remembers.
const HISTORY_LIMIT: usize = 100;

impl History {
    /// Records a change from `before` to `after`, joined to the step before as `behavior` says.
    fn record(&mut self, before: TextFieldValue, after: TextFieldValue, behavior: UndoBehavior) {
        self.future.clear();
        let typing = behavior == UndoBehavior::MergeTyping;
        if typing
            && let Some(last) = self.past.last_mut()
            && last.typing
            && last.after == before
        {
            last.after = after;
            return;
        }
        self.past.push(Step {
            before,
            after,
            typing,
        });
        if self.past.len() > HISTORY_LIMIT {
            self.past.remove(0);
        }
    }
}

/// What Enter does in a single-line field.
type SubmitHandler = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// What a text field holds, Compose's `TextFieldState`: its text and selection, which
/// side of a wrap the caret is on and the history of its changes.
///
/// Make one outside composition with [`TextFieldState::new`], for a state holder that
/// owns it, or remember one at a call site with
/// [`remember_text_field_state`](crate::remember_text_field_state). Change it with
/// [`edit`](Self::edit). Two handles are equal when they are the same state.
#[derive(Clone)]
pub struct TextFieldState {
    /// The text and selection.
    pub(crate) value: State<TextFieldValue>,
    /// What Enter does in a single-line field, set each pass by whoever owns the field.
    pub(crate) submit: SubmitHandler,
    /// Which side of a wrap the caret is on when it is at the end of a wrapped line.
    pub(crate) affinity: State<Affinity>,
    /// The x Up and Down keep to while they move the caret through shorter lines.
    pub(crate) goal_x: Rc<Cell<Option<f32>>>,
    /// The steps undo and redo walk.
    pub(crate) history: Rc<RefCell<History>>,
    /// Counts changes to the history, read by whoever asks whether undo can run, so
    /// they compose again when it changes.
    pub(crate) history_version: State<u64>,
}

impl PartialEq for TextFieldState {
    /// Whether both handles are the same state.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.history, &other.history)
    }
}

impl std::fmt::Debug for TextFieldState {
    /// The text and selection.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TextFieldState")
            .field("value", &self.value.get())
            .finish()
    }
}

impl Default for TextFieldState {
    /// An empty field.
    fn default() -> Self {
        Self::new("")
    }
}

impl TextFieldState {
    /// A field holding `initial` with the caret at its end, which belongs to no
    /// composition: Compose's `TextFieldState(initialText)`.
    pub fn new(initial: impl AsRef<str>) -> Self {
        let initial = initial.as_ref();
        Self::with_selection(initial, TextRange::collapsed_at(initial.chars().count()))
    }

    /// A field holding `initial` with `selection` selected, kept within the text.
    pub fn with_selection(initial: impl AsRef<str>, selection: TextRange) -> Self {
        let initial = initial.as_ref();
        let length = initial.chars().count();
        Self {
            value: mutable_state(TextFieldValue {
                text: initial.to_string(),
                cursor: selection.end.min(length),
                anchor: selection.start.min(length),
            }),
            submit: Rc::default(),
            affinity: mutable_state(Affinity::Downstream),
            goal_x: Rc::default(),
            history: Rc::default(),
            history_version: mutable_state(0),
        }
    }

    /// Runs `handler` when Enter is pressed in the field, if it is a single line.
    pub fn on_submit(&self, handler: impl Fn() + 'static) {
        *self.submit.borrow_mut() = Some(Rc::new(handler));
    }

    /// Empties the field, as one step undo can take back: Compose's `clearText`.
    pub fn clear(&self) {
        self.clear_text();
    }

    /// Empties the field, as one step undo can take back: Compose's `clearText`.
    pub fn clear_text(&self) {
        self.edit(|buffer| {
            buffer.delete(0, buffer.length());
            buffer.place_cursor_at_end();
        });
    }

    /// The text and selection.
    pub fn value(&self) -> TextFieldValue {
        self.value.get()
    }

    /// The text.
    pub fn text(&self) -> String {
        self.value.with(|value| value.text.clone())
    }

    /// Reads the text without copying it.
    pub fn with_text<R>(&self, read: impl FnOnce(&str) -> R) -> R {
        self.value.with(|value| read(&value.text))
    }

    /// The selection, from its anchor to its caret.
    pub fn selection(&self) -> TextRange {
        self.value.with(TextFieldValue::selection_range)
    }

    /// The selected text, empty when nothing is selected.
    pub fn selected_text(&self) -> String {
        self.value
            .with(|value| value.selected_text().unwrap_or_default().to_string())
    }

    /// Replaces the text and puts the caret at its end, as one step undo can take back:
    /// Compose's `setTextAndPlaceCursorAtEnd`.
    pub fn set_text(&self, text: impl Into<String>) {
        self.set_text_and_place_cursor_at_end(text);
    }

    /// Replaces the text and puts the caret at its end, as one step undo can take back.
    pub fn set_text_and_place_cursor_at_end(&self, text: impl Into<String>) {
        let text = text.into();
        self.edit(|buffer| {
            buffer.replace(0, buffer.length(), &text);
            buffer.place_cursor_at_end();
        });
    }

    /// Replaces the text and selects all of it, as one step undo can take back:
    /// Compose's `setTextAndSelectAll`.
    pub fn set_text_and_select_all(&self, text: impl Into<String>) {
        let text = text.into();
        self.edit(|buffer| {
            buffer.replace(0, buffer.length(), &text);
            buffer.select_all();
        });
    }

    /// Replaces the text, caret and selection with `value`, as one step undo can take back.
    pub fn set_value(&self, value: TextFieldValue) {
        self.edit(|buffer| {
            buffer.replace(0, buffer.length(), &value.text);
            buffer.set_selection(value.selection_range());
        });
    }

    /// Puts `text` in place of the selection, or at the caret when nothing is
    /// selected, keeping line breaks, and leaves the caret after it.
    pub fn replace_selection(&self, text: &str) {
        self.edit(|buffer| {
            let selection = buffer.selection();
            buffer.replace(selection.min(), selection.max(), text);
            buffer.place_cursor_before_char_at(selection.min() + text.chars().count());
        });
    }

    /// Selects all of the text.
    pub fn select_all(&self) {
        self.edit(TextFieldBuffer::select_all);
    }

    /// Changes the field through a [`TextFieldBuffer`], Compose's `TextFieldState.edit`:
    /// `block` changes the text and selection, and what it leaves becomes the field's value
    /// at once, as one step undo can take back when it changed the text. Edits made here are
    /// the program's, so no [`InputTransformation`](crate::InputTransformation) sees them.
    pub fn edit<R>(&self, block: impl FnOnce(&mut TextFieldBuffer) -> R) -> R {
        let mut buffer = TextFieldBuffer::new(self.value());
        let result = block(&mut buffer);
        self.commit(buffer, UndoBehavior::NeverMerge);
        result
    }

    /// Makes what `buffer` holds the field's value, recording a change of text in the
    /// history as `behavior` says, and only marking the composition dirty when it changed.
    pub(crate) fn commit(&self, buffer: TextFieldBuffer, behavior: UndoBehavior) {
        let before = buffer.original.clone();
        let after = buffer.into_value();
        if after == before {
            return;
        }
        if after.text != before.text {
            self.history
                .borrow_mut()
                .record(before, after.clone(), behavior);
            self.history_version.update(|version| *version += 1);
        }
        self.value.set(after);
    }

    /// The history of the field's changes: Compose's `TextFieldState.undoState`.
    pub fn undo_state(&self) -> UndoState {
        UndoState {
            state: self.clone(),
        }
    }
}

/// Undoes and redoes the changes of a [`TextFieldState`], Compose's `UndoState`. Typing
/// that follows on from typing is one step; every other change, and every
/// [`TextFieldState::edit`], is a step of its own.
#[derive(Clone)]
pub struct UndoState {
    /// The field whose history this is.
    state: TextFieldState,
}

impl UndoState {
    /// Whether there is a step to undo. Reading it in composition composes again when it changes.
    pub fn can_undo(&self) -> bool {
        self.state.history_version.get();
        !self.state.history.borrow().past.is_empty()
    }

    /// Whether there is an undone step to redo. Reading it in composition composes again when it changes.
    pub fn can_redo(&self) -> bool {
        self.state.history_version.get();
        !self.state.history.borrow().future.is_empty()
    }

    /// Takes back the latest step, restoring the text and selection before it.
    pub fn undo(&self) {
        let step = self.state.history.borrow_mut().past.pop();
        if let Some(step) = step {
            self.state.value.set(step.before.clone());
            self.state.history.borrow_mut().future.push(step);
            self.state.history_version.update(|version| *version += 1);
        }
    }

    /// Makes the latest undone step again, restoring the text and selection after it.
    pub fn redo(&self) {
        let step = self.state.history.borrow_mut().future.pop();
        if let Some(step) = step {
            self.state.value.set(step.after.clone());
            self.state.history.borrow_mut().past.push(Step {
                typing: false,
                ..step
            });
            self.state.history_version.update(|version| *version += 1);
        }
    }

    /// Forgets every step, so neither undo nor redo has anything to do.
    pub fn clear_history(&self) {
        let mut history = self.state.history.borrow_mut();
        history.past.clear();
        history.future.clear();
        drop(history);
        self.state.history_version.update(|version| *version += 1);
    }
}

/// Tests of the module.
#[cfg(test)]
mod tests {
    use super::*;

    /// The field holding `text` with the caret at its end.
    fn field(text: &str) -> TextFieldState {
        TextFieldState::new(text)
    }

    /// An edit changes the text and the selection through its buffer, and returns what its block returns.
    #[test]
    fn an_edit_changes_text_and_selection_through_the_buffer() {
        let state = field("hello");
        let found = state.edit(|buffer| {
            buffer.insert(0, "oh, ");
            buffer.append("!");
            buffer.set_selection(TextRange::new(0, 2));
            buffer.length()
        });
        assert_eq!(found, 10);
        assert_eq!(state.text(), "oh, hello!");
        assert_eq!(state.selection(), TextRange::new(0, 2));
        assert_eq!(state.selected_text(), "oh");
    }

    /// A replace moves a caret after it by the change and leaves one before it.
    #[test]
    fn a_replace_moves_a_caret_after_it_and_keeps_one_before_it() {
        let state = field("abcdef");
        state.edit(|buffer| {
            buffer.place_cursor_before_char_at(5);
            buffer.replace(1, 3, "XYZW");
            assert_eq!(buffer.selection(), TextRange::collapsed_at(7));
            buffer.place_cursor_before_char_at(1);
            buffer.replace(2, 3, "");
            assert_eq!(buffer.selection(), TextRange::collapsed_at(1));
        });
    }

    /// The change list maps places from before an edit to after it and back.
    #[test]
    fn the_change_list_maps_both_ways() {
        let state = field("0123456789");
        state.edit(|buffer| {
            buffer.insert(2, "--");
            buffer.replace(8, 10, "x");
            let changes = buffer.changes();
            assert_eq!(changes.change_count(), 2);
            assert_eq!(changes.get_original_range(0), TextRange::new(2, 2));
            assert_eq!(changes.get_range(0), TextRange::new(2, 4));
            assert_eq!(changes.get_original_range(1), TextRange::new(6, 8));
            assert_eq!(changes.get_range(1), TextRange::new(8, 9));
            assert_eq!(changes.map_forward(5), 7);
            assert_eq!(changes.map_forward(9), 10);
            assert_eq!(changes.map_back(7), 5);
            assert_eq!(changes.map_back(3), 2);
        });
    }

    /// Changes that touch are one change.
    #[test]
    fn touching_changes_merge() {
        let state = field("abc");
        state.edit(|buffer| {
            buffer.insert(1, "X");
            buffer.insert(2, "Y");
            let changes = buffer.changes();
            assert_eq!(changes.change_count(), 1);
            assert_eq!(changes.get_original_range(0), TextRange::new(1, 1));
            assert_eq!(changes.get_range(0), TextRange::new(1, 3));
        });
    }

    /// Reverting takes every change back, leaving nothing to undo.
    #[test]
    fn reverting_takes_every_change_back() {
        let state = field("keep");
        state.edit(|buffer| {
            buffer.append(" more");
            buffer.revert_all_changes();
        });
        assert_eq!(state.text(), "keep");
        assert!(!state.undo_state().can_undo());
    }

    /// Undo and redo walk the history, typing that follows typing being one step.
    #[test]
    fn undo_and_redo_walk_the_history() {
        let state = field("");
        let mut buffer = TextFieldBuffer::new(state.value());
        buffer.insert(0, "a");
        buffer.place_cursor_at_end();
        state.commit(buffer, UndoBehavior::MergeTyping);
        let mut buffer = TextFieldBuffer::new(state.value());
        buffer.append("b");
        buffer.place_cursor_at_end();
        state.commit(buffer, UndoBehavior::MergeTyping);
        state.set_text("other");
        let undo = state.undo_state();
        assert!(undo.can_undo());
        undo.undo();
        assert_eq!(state.text(), "ab");
        undo.undo();
        assert_eq!(state.text(), "");
        assert!(!undo.can_undo());
        undo.redo();
        assert_eq!(state.text(), "ab");
        undo.redo();
        assert_eq!(state.text(), "other");
        assert!(!undo.can_redo());
        undo.clear_history();
        assert!(!undo.can_undo());
    }

    /// Words, lines and word moves are found around a place.
    #[test]
    fn words_and_lines_are_found_around_an_index() {
        let text = "one two_three, four\nnext";
        assert_eq!(word_around(text, 5), TextRange::new(4, 13));
        assert_eq!(line_around(text, 22), TextRange::new(20, 24));
        assert_eq!(previous_word("one two", 7), 4);
        assert_eq!(previous_word("one two ", 8), 4);
        assert_eq!(next_word("one two", 0), 3);
        assert_eq!(next_word("one two", 3), 7);
    }

    /// Handles to one state are equal, and to two states not.
    #[test]
    fn handles_to_one_state_are_equal() {
        let state = field("x");
        assert_eq!(state, state.clone());
        assert_ne!(state, field("x"));
    }
}
