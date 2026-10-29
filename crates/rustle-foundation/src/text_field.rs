//! A single-line text field: editing, caret, selection and clipboard.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use rustle_geometry::Offset;
use rustle_graphics::Color;
use rustle_runtime::{State, group, launched_effect, remember};
use rustle_ui::{
    CharPosition, Content, Focus, Key, KeyEvent, Modifier, Node, Role, ScrollState, Semantics,
    TextEditing, clipboard, emit, remember_focus, remember_scroll_state,
};

use crate::LOCAL_TEXT_STYLE;

/// The text of a field with its caret and selection, counted in characters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextFieldValue {
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

    fn length(&self) -> usize {
        self.text.chars().count()
    }

    fn byte(&self, index: usize) -> usize {
        self.text
            .char_indices()
            .nth(index)
            .map_or(self.text.len(), |(byte, _)| byte)
    }

    pub fn selected_text(&self) -> Option<&str> {
        let (start, end) = self.selection()?;
        Some(&self.text[self.byte(start)..self.byte(end)])
    }

    /// Removes the selection. Returns whether there was one.
    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.selection() else {
            return false;
        };
        let (from, to) = (self.byte(start), self.byte(end));
        self.text.replace_range(from..to, "");
        self.cursor = start;
        self.anchor = start;
        true
    }

    /// Types `text` over the selection, without line breaks or control characters.
    fn insert(&mut self, text: &str, multiline: bool) {
        let text: String = text
            .chars()
            .filter(|c| !c.is_control() || (multiline && *c == '\n'))
            .collect();
        self.delete_selection();
        let at = self.byte(self.cursor);
        self.text.insert_str(at, &text);
        self.cursor += text.chars().count();
        self.anchor = self.cursor;
    }

    /// Where the line the caret is on starts, as a character index.
    fn line_start(&self) -> usize {
        let before: Vec<char> = self.text.chars().take(self.cursor).collect();
        before
            .iter()
            .rposition(|c| *c == '\n')
            .map_or(0, |at| at + 1)
    }

    /// Where the line the caret is on ends, as a character index.
    fn line_end(&self) -> usize {
        self.text
            .chars()
            .enumerate()
            .skip(self.cursor)
            .find(|(_, c)| *c == '\n')
            .map_or_else(|| self.length(), |(at, _)| at)
    }

    /// Selects the word around `index`: the run of letters and digits it touches, or the single character otherwise.
    fn select_word(&mut self, index: usize) {
        let characters: Vec<char> = self.text.chars().collect();
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let index = index.min(characters.len());
        let mut start = index;
        while start > 0 && word(characters[start - 1]) {
            start -= 1;
        }
        let mut end = index;
        while end < characters.len() && word(characters[end]) {
            end += 1;
        }
        if start == end && end < characters.len() {
            end += 1;
        }
        self.anchor = start;
        self.cursor = end;
    }

    /// Moves the caret to `to`, dragging the selection along if `extend` is set.
    fn move_to(&mut self, to: usize, extend: bool) {
        self.cursor = to.min(self.length());
        if !extend {
            self.anchor = self.cursor;
        }
    }
}

/// How a text field behaves.
#[derive(Debug, Clone, Copy, Default)]
pub struct TextFieldOptions {
    /// The text can be selected and copied but not changed.
    pub read_only: bool,
    /// Enter starts a new line, and Up and Down move between lines.
    pub multiline: bool,
}

/// The boundary on `line` nearest to `x`, as a character index.
fn nearest_on_line(positions: &[CharPosition], line: u32, x: f32) -> usize {
    positions
        .iter()
        .enumerate()
        .filter(|(_, position)| position.line == line)
        .min_by(|(_, a), (_, b)| (a.x - x).abs().total_cmp(&(b.x - x).abs()))
        .map_or(0, |(index, _)| index)
}

/// Scrolls `scroll` the least that brings `position`, with `margin` after it, into view.
fn follow(scroll: &ScrollState, position: f32, margin: f32) {
    let viewport = scroll.viewport_length();
    if viewport <= 0.0 {
        return;
    }
    let offset = scroll.offset();
    if position + margin > offset + viewport {
        scroll.scroll_to(position + margin - viewport);
    } else if position < offset {
        scroll.scroll_to(position);
    }
}

/// The modifiers that give a field's text room to be longer than the field:
/// it fills the field and scrolls, without wrapping, along the axes it can grow.
fn scrolling(multiline: bool, across: &ScrollState, down: &ScrollState) -> Modifier {
    if multiline {
        Modifier::new()
            .fill_max_size()
            .vertical_scroll(down)
            .horizontal_scroll(across)
    } else {
        Modifier::new().fill_max_width().horizontal_scroll(across)
    }
}

type SubmitHandler = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// What a text field remembers: its value.
#[derive(Clone)]
pub struct TextFieldState {
    value: State<TextFieldValue>,
    /// What Enter does in a single-line field, set each pass by whoever owns the field.
    submit: SubmitHandler,
}

impl TextFieldState {
    /// Runs `handler` when Enter is pressed in the field, if it is a single line.
    pub fn on_submit(&self, handler: impl Fn() + 'static) {
        *self.submit.borrow_mut() = Some(Rc::new(handler));
    }

    /// Empties the field.
    pub fn clear(&self) {
        self.set_text("");
    }

    pub fn value(&self) -> TextFieldValue {
        self.value.get()
    }

    pub fn text(&self) -> String {
        self.value.with(|value| value.text.clone())
    }

    /// Replaces the text and puts the caret at its end.
    pub fn set_text(&self, text: impl Into<String>) {
        let text = text.into();
        let end = text.chars().count();
        self.value.set(TextFieldValue {
            text,
            cursor: end,
            anchor: end,
        });
    }

    /// Applies an edit and only marks the composition dirty if it changed anything.
    fn edit(&self, change: impl FnOnce(&mut TextFieldValue)) {
        let mut value = self.value();
        let before = value.clone();
        change(&mut value);
        if value != before {
            self.value.set(value);
        }
    }

    /// Handles a key event aimed at the field. Returns whether it was used.
    /// A read-only field moves, selects and copies but does not change.
    fn handle_key(
        &self,
        event: &KeyEvent,
        options: TextFieldOptions,
        boundaries: &RefCell<Vec<CharPosition>>,
    ) -> bool {
        let TextFieldOptions {
            read_only,
            multiline,
        } = options;
        if !event.pressed {
            return false;
        }
        let shift = event.modifiers.shift;
        let command = event.modifiers.command();
        let mut handled = true;
        let mut submit: Option<Rc<dyn Fn()>> = None;
        self.edit(|value| match &event.key {
            Key::ArrowLeft => match value.selection() {
                Some((start, _)) if !shift => value.move_to(start, false),
                _ => value.move_to(value.cursor.saturating_sub(1), shift),
            },
            Key::ArrowRight => match value.selection() {
                Some((_, end)) if !shift => value.move_to(end, false),
                _ => value.move_to(value.cursor + 1, shift),
            },
            Key::Home if multiline => value.move_to(value.line_start(), shift),
            Key::End if multiline => value.move_to(value.line_end(), shift),
            Key::Home => value.move_to(0, shift),
            Key::End => value.move_to(value.length(), shift),
            Key::ArrowUp | Key::ArrowDown if multiline => {
                let positions = boundaries.borrow();
                let Some(here) = positions.get(value.cursor).copied() else {
                    return;
                };
                let last_line = positions.last().map_or(0, |last| last.line);
                let up = event.key == Key::ArrowUp;
                if (up && here.line == 0) || (!up && here.line >= last_line) {
                    value.move_to(if up { 0 } else { value.length() }, shift);
                } else {
                    let target = if up { here.line - 1 } else { here.line + 1 };
                    let index = nearest_on_line(&positions, target, here.x);
                    value.move_to(index, shift);
                }
            }
            Key::Enter if multiline && !read_only => value.insert("\n", true),
            Key::Enter if !multiline => {
                let handler = self.submit.borrow().clone();
                match handler {
                    Some(handler) => submit = Some(handler),
                    None => handled = false,
                }
            }
            Key::Backspace | Key::Delete if read_only => handled = false,
            Key::Backspace => {
                if !value.delete_selection() && value.cursor > 0 {
                    value.move_to(value.cursor - 1, true);
                    value.delete_selection();
                }
            }
            Key::Delete => {
                if !value.delete_selection() && value.cursor < value.length() {
                    value.move_to(value.cursor + 1, true);
                    value.delete_selection();
                }
            }
            Key::Character(letter) if command => match letter.to_lowercase().as_str() {
                "a" => {
                    value.anchor = 0;
                    value.cursor = value.length();
                }
                "c" => {
                    if let (Some(text), Some(clipboard)) = (value.selected_text(), clipboard()) {
                        clipboard.set_text(text);
                    }
                }
                "x" if read_only => handled = false,
                "v" if read_only => handled = false,
                "x" => {
                    if let (Some(text), Some(clipboard)) = (value.selected_text(), clipboard()) {
                        clipboard.set_text(text);
                        value.delete_selection();
                    }
                }
                "v" => {
                    if let Some(text) = clipboard().and_then(|clipboard| clipboard.text()) {
                        value.insert(&text, multiline);
                    }
                }
                _ => handled = false,
            },
            _ if command || event.modifiers.alt || read_only => handled = false,
            _ => match &event.text {
                Some(text) if !text.chars().all(char::is_control) => value.insert(text, multiline),
                _ => handled = false,
            },
        });
        // Run after the edit, so the handler sees the field as it is now.
        if let Some(submit) = submit {
            submit();
        }
        handled
    }
}

/// Remembers the state of a text field at this call site, starting with `initial`.
#[track_caller]
pub fn remember_text_field_state(initial: &str) -> TextFieldState {
    group(|| {
        let end = initial.chars().count();
        TextFieldState {
            value: remember(|| TextFieldValue {
                text: initial.to_string(),
                cursor: end,
                anchor: end,
            }),
            submit: remember(Rc::default).get(),
        }
    })
}

/// Emits an editable single line of text with no decoration.
///
/// The field takes keyboard focus when clicked or tabbed to, and shows its
/// caret while it has it. Style the surroundings with `modifier`.
#[track_caller]
pub fn basic_text_field(modifier: Modifier, state: &TextFieldState) {
    group(|| {
        let focus = remember_focus();
        basic_text_field_in(
            modifier,
            Modifier::new(),
            state,
            &focus,
            TextFieldOptions::default(),
        );
    });
}

/// Emits an editable field of several lines: Enter starts a new one, and lines
/// do not wrap. Size it with `modifier`; it clips what does not fit.
#[track_caller]
pub fn basic_text_area(modifier: Modifier, state: &TextFieldState) {
    group(|| {
        let focus = remember_focus();
        let options = TextFieldOptions {
            read_only: false,
            multiline: true,
        };
        basic_text_field_in(modifier.clip(), Modifier::new(), state, &focus, options);
    });
}

/// Emits text the user can select and copy but not change.
#[track_caller]
pub fn selectable_text(modifier: Modifier, content: &str) {
    group(|| {
        let state = remember_text_field_state(content);
        if state.text() != content {
            state.set_text(content);
        }
        let focus = remember_focus();
        let options = TextFieldOptions {
            read_only: true,
            multiline: true,
        };
        basic_text_field_in(modifier, Modifier::new(), &state, &focus, options);
    });
}

/// Like [`basic_text_field`], with the modifier split in two: `outer` applies
/// around the focusable area, `inner` inside it, between the focusable area
/// and the text. Put padding in `inner` for the padding to take focus when
/// clicked, but note that only the text itself places the caret. `focus` is
/// the field's focus, so the surroundings can show whether it has it. A
/// `read_only` field can be selected and copied from but not edited.
#[track_caller]
pub fn basic_text_field_in(
    outer: Modifier,
    inner: Modifier,
    state: &TextFieldState,
    focus: &Focus,
    options: TextFieldOptions,
) {
    let TextFieldOptions {
        read_only,
        multiline,
    } = options;
    group(|| {
        let boundaries = remember(|| Rc::new(RefCell::new(Vec::<CharPosition>::new()))).get();
        let value = state.value();
        let style = LOCAL_TEXT_STYLE.current();

        // Text longer than the field scrolls to keep the caret in view. This runs
        // after layout, when the caret's position is known.
        let across = remember_scroll_state();
        let down = remember_scroll_state();
        let on_layout: Rc<dyn Fn()> = {
            let (state, focus) = (state.clone(), focus.clone());
            let (boundaries, across, down) = (Rc::clone(&boundaries), across.clone(), down.clone());
            let line_height = style.line_height;
            Rc::new(move || {
                if !focus.is_focused() {
                    return;
                }
                let cursor = state.value().cursor;
                if let Some(caret) = boundaries.borrow().get(cursor).copied() {
                    follow(&across, caret.x, 8.0);
                    if multiline {
                        follow(&down, caret.line as f32 * line_height, line_height);
                    }
                }
            })
        };

        // The caret blinks while the field has focus, and shows solid after any edit or move.
        let caret_on = remember(|| true);
        let blink = caret_on.clone();
        let focused = focus.is_focused();
        launched_effect(
            (focused, value.cursor, value.text.len()),
            move |scope| async move {
                if !blink.get() {
                    blink.set(true);
                }
                if focused {
                    loop {
                        scope.delay(Duration::from_millis(530)).await;
                        blink.update(|on| *on = !*on);
                    }
                }
            },
        );

        let keys = state.clone();
        let key_boundaries = Rc::clone(&boundaries);
        let line_height = style.line_height;
        let nearest = {
            let boundaries = Rc::clone(&boundaries);
            move |at: Offset| {
                let positions = boundaries.borrow();
                let last_line = positions.last().map_or(0, |last| last.line);
                let line = ((at.y / line_height).floor().max(0.0) as u32).min(last_line);
                nearest_on_line(&positions, line, at.x)
            }
        };
        let (presses, drags) = (state.clone(), state.clone());
        let (press_at, drag_to) = (nearest.clone(), nearest);
        let modifier = outer
            .semantics(
                Semantics::new(Role::TextInput)
                    .value(value.text.clone())
                    .focus(focus),
            )
            .focusable(focus, move |event| {
                keys.handle_key(event, options, &key_boundaries)
            })
            .chain(inner)
            .chain(scrolling(multiline, &across, &down))
            .on_press(move |press| {
                let index = press_at(press.position);
                presses.edit(|value| match press.click_count {
                    2 => value.select_word(index),
                    3 => {
                        value.anchor = 0;
                        value.cursor = value.length();
                    }
                    _ => value.move_to(index, false),
                });
            })
            .on_drag(move |at| {
                let index = drag_to(at);
                drags.edit(|value| value.move_to(index, true));
            });

        emit(Node {
            modifier,
            content: Content::Text {
                content: value.text.clone(),
                style,
                editing: Some(TextEditing {
                    caret: (focused && caret_on.get() && !read_only).then_some(value.cursor),
                    selection: value.selection(),
                    caret_color: style.color,
                    selection_color: Color::rgba(74, 96, 214, 140),
                    boundaries,
                    on_layout: Some(on_layout),
                }),
            },
        });
    });
}
