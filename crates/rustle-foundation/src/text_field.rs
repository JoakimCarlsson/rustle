//! A single-line text field: editing, caret, selection and clipboard.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use rustle_geometry::Offset;
use rustle_runtime::{State, group, launched_effect, remember};
use rustle_text::{Affinity, TextLayout, TextWrap};
use rustle_ui::{
    CharPosition, Content, Focus, Key, KeyEvent, Modifier, Node, Role, ScrollState, Semantics,
    TextEditing, clipboard, emit, remember_focus, remember_scroll_state,
};

use crate::LOCAL_TEXT_STYLE;
use crate::selection::{LOCAL_SELECTION, SELECTION_COLOR, emit_block};

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
    /// A multiline field keeps long lines whole and scrolls sideways instead of
    /// wrapping them at the field's width. Home and End then go to the ends of the
    /// line in the text; wrapped, they go to the ends of the visual line.
    pub no_wrap: bool,
}

type LayoutCell = Rc<RefCell<Option<Rc<TextLayout>>>>;

/// The character index of byte `byte` of `text`.
fn char_index(text: &str, byte: usize) -> usize {
    text.get(..byte)
        .map_or_else(|| text.chars().count(), |s| s.chars().count())
}

/// The byte of character `index` of `text`.
fn value_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
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

/// The modifiers that give a field's text room to be longer than the field: it
/// scrolls along the axes it can grow. A wrapping multiline field only scrolls
/// down, since its lines are as wide as the field; one that does not wrap, and a
/// single line, also scroll across. Read-only text is as big as its content.
fn scrolling(options: TextFieldOptions, across: &ScrollState, down: &ScrollState) -> Modifier {
    if options.read_only && options.multiline {
        Modifier::new()
    } else if options.multiline && !options.no_wrap {
        Modifier::new().fill_max_size().vertical_scroll(down)
    } else if options.multiline {
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
    /// Which side of a wrap the caret is on when it is at the end of a wrapped line.
    affinity: State<Affinity>,
    /// The x Up and Down keep to while they move the caret through shorter lines.
    goal_x: Rc<Cell<Option<f32>>>,
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
    fn handle_key(&self, event: &KeyEvent, options: TextFieldOptions, layout: &LayoutCell) -> bool {
        let TextFieldOptions {
            read_only,
            multiline,
            ..
        } = options;
        if !event.pressed {
            return false;
        }
        let shift = event.modifiers.shift;
        let command = event.modifiers.command();
        let mut handled = true;
        let mut submit: Option<Rc<dyn Fn()>> = None;
        // Where the caret ends up relative to a wrap, and the column Up and Down keep to.
        let mut affinity = Affinity::Downstream;
        let mut goal_x = None;
        self.edit(|value| match &event.key {
            Key::ArrowLeft => match value.selection() {
                Some((start, _)) if !shift => value.move_to(start, false),
                _ => value.move_to(value.cursor.saturating_sub(1), shift),
            },
            Key::ArrowRight => match value.selection() {
                Some((_, end)) if !shift => value.move_to(end, false),
                _ => value.move_to(value.cursor + 1, shift),
            },
            Key::Home | Key::End if multiline && !command => {
                let home = event.key == Key::Home;
                match layout.borrow().clone() {
                    Some(layout) => {
                        let (byte, at) = (value.byte(value.cursor), self.affinity.get());
                        let (to, side) = if home {
                            layout.line_start(byte, at)
                        } else {
                            layout.line_end(byte, at)
                        };
                        value.move_to(char_index(layout.text(), to), shift);
                        affinity = side;
                    }
                    None if home => value.move_to(value.line_start(), shift),
                    None => value.move_to(value.line_end(), shift),
                }
            }
            Key::Home => value.move_to(0, shift),
            Key::End => value.move_to(value.length(), shift),
            Key::ArrowUp | Key::ArrowDown if multiline => {
                let Some(layout) = layout.borrow().clone() else {
                    return;
                };
                let up = event.key == Key::ArrowUp;
                let (byte, at) = (value.byte(value.cursor), self.affinity.get());
                let x = self
                    .goal_x
                    .get()
                    .unwrap_or_else(|| layout.caret(byte, at).x);
                goal_x = Some(x);
                match layout.move_vertically(byte, at, if up { -1 } else { 1 }, Some(x)) {
                    Some((to, side)) => {
                        value.move_to(char_index(layout.text(), to), shift);
                        affinity = side;
                    }
                    None => value.move_to(if up { 0 } else { value.length() }, shift),
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
        if handled {
            if self.affinity.get() != affinity {
                self.affinity.set(affinity);
            }
            self.goal_x.set(goal_x);
        }
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
            affinity: remember(|| Affinity::Downstream),
            goal_x: remember(Rc::default).get(),
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
/// longer than the field wrap at its width. Up and Down move by visual line, Home
/// and End go to the ends of the visual line, and the field scrolls down to keep
/// the caret in view. Size it with `modifier`; it clips what does not fit. See
/// [`basic_text_area_no_wrap`] for lines that stay whole.
#[track_caller]
pub fn basic_text_area(modifier: Modifier, state: &TextFieldState) {
    group(|| {
        let focus = remember_focus();
        let options = TextFieldOptions {
            multiline: true,
            ..TextFieldOptions::default()
        };
        basic_text_field_in(modifier.clip(), Modifier::new(), state, &focus, options);
    });
}

/// Like [`basic_text_area`], but lines do not wrap: a long line makes the field
/// scroll sideways to keep the caret in view.
#[track_caller]
pub fn basic_text_area_no_wrap(modifier: Modifier, state: &TextFieldState) {
    group(|| {
        let focus = remember_focus();
        let options = TextFieldOptions {
            multiline: true,
            no_wrap: true,
            ..TextFieldOptions::default()
        };
        basic_text_field_in(modifier.clip(), Modifier::new(), state, &focus, options);
    });
}

/// Emits text the user can select and copy but not change. Long lines wrap at the
/// width available. Inside a [`selection_container`](crate::selection_container)
/// the text is one block of a selection that spans several blocks.
#[track_caller]
pub fn selectable_text(modifier: Modifier, content: &str) {
    group(|| {
        if let Some(scope) = LOCAL_SELECTION.current() {
            let style = LOCAL_TEXT_STYLE.current();
            let text = content.to_string();
            emit_block(&scope, modifier, text.clone(), |editing| Content::Text {
                content: text,
                style,
                editing: Some(editing),
            });
            return;
        }
        let state = remember_text_field_state(content);
        if state.text() != content {
            state.set_text(content);
        }
        let focus = remember_focus();
        let options = TextFieldOptions {
            read_only: true,
            multiline: true,
            ..TextFieldOptions::default()
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
        no_wrap,
    } = options;
    group(|| {
        let boundaries = remember(|| Rc::new(RefCell::new(Vec::<CharPosition>::new()))).get();
        let layout: LayoutCell = remember(|| Rc::new(RefCell::new(None))).get();
        let value = state.value();
        // Only a multiline field that wraps breaks lines at its width.
        let style = LOCAL_TEXT_STYLE.current();
        let style = if multiline && !no_wrap {
            style
        } else {
            style.wrap(TextWrap::None)
        };

        // An input method's composition is shown inline at the caret, underlined, and is not
        // part of the value until the input method commits it.
        let focused = focus.is_focused();
        let preedit = focus
            .preedit()
            .filter(|preedit| focused && !read_only && !preedit.text.is_empty());
        let (shown, underline, shown_caret) = match &preedit {
            Some(preedit) => {
                let mut shown = value.text.clone();
                shown.insert_str(value.byte(value.cursor), &preedit.text);
                let length = preedit.text.chars().count();
                let inside = preedit
                    .cursor
                    .map(|(_, end)| preedit.text[..end.min(preedit.text.len())].chars().count());
                (
                    shown,
                    Some((value.cursor, value.cursor + length)),
                    Some(value.cursor + inside.unwrap_or(length)),
                )
            }
            None => (value.text.clone(), None, Some(value.cursor)),
        };
        let composing = preedit.is_some();
        // A composition hides the caret when the input method gives no cursor in it.
        let caret_hidden = composing && preedit.as_ref().is_some_and(|p| p.cursor.is_none());
        let shown_caret = shown_caret.unwrap_or(value.cursor);

        // Text longer than the field scrolls to keep the caret in view. This runs
        // after layout, when the caret's position is known.
        let across = remember_scroll_state();
        let down = remember_scroll_state();
        let on_layout: Rc<dyn Fn()> = {
            let focus = focus.clone();
            let (layout, across, down) = (Rc::clone(&layout), across.clone(), down.clone());
            let affinity = state.affinity.clone();
            Rc::new(move || {
                if !focus.is_focused() {
                    return;
                }
                let Some(layout) = layout.borrow().clone() else {
                    return;
                };
                let byte = value_byte(layout.text(), shown_caret);
                let caret = layout.caret(byte, affinity.get());
                if !multiline || no_wrap {
                    follow(&across, caret.x, 8.0);
                }
                if multiline {
                    follow(&down, caret.y, caret.height);
                }
            })
        };

        // The caret blinks while the field has focus, and shows solid after any edit or move.
        let caret_on = remember(|| true);
        let blink = caret_on.clone();
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
        let key_layout = Rc::clone(&layout);
        // The character index and side of a wrap nearest to a point in the text.
        let nearest = {
            let layout = Rc::clone(&layout);
            move |at: Offset| {
                layout
                    .borrow()
                    .clone()
                    .map_or((0, Affinity::Downstream), |layout| {
                        let (byte, side) = layout.hit_test_with_affinity(at);
                        (char_index(layout.text(), byte), side)
                    })
            }
        };
        let (presses, drags) = (state.clone(), state.clone());
        let (press_at, drag_to) = (nearest.clone(), nearest);
        let described = {
            let semantics = Semantics::new(Role::TextInput)
                .value(value.text.clone())
                .text_selection(value.anchor, value.cursor)
                .focus(focus);
            let selecting = state.clone();
            let semantics = semantics.on_set_text_selection(move |anchor, focus| {
                selecting.edit(|value| {
                    let length = value.length();
                    value.anchor = anchor.min(length);
                    value.cursor = focus.min(length);
                });
            });
            if read_only {
                semantics
            } else {
                let setting = state.clone();
                semantics.on_set_value(move |text| setting.set_text(text))
            }
        };
        let modifier = outer
            .semantics(described)
            .focusable(focus, move |event| {
                keys.handle_key(event, options, &key_layout)
            })
            .chain(inner)
            .chain(scrolling(options, &across, &down))
            .on_press(move |press| {
                let (index, side) = press_at(press.position);
                if presses.affinity.get() != side {
                    presses.affinity.set(side);
                }
                presses.goal_x.set(None);
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
                let (index, side) = drag_to(at);
                if drags.affinity.get() != side {
                    drags.affinity.set(side);
                }
                drags.edit(|value| value.move_to(index, true));
            });

        emit(Node {
            modifier,
            content: Content::Text {
                content: shown,
                style,
                editing: Some(TextEditing {
                    caret: (focused && caret_on.get() && !read_only && !caret_hidden)
                        .then_some(shown_caret),
                    selection: if composing { None } else { value.selection() },
                    underline,
                    ime_caret: (focused && !read_only).then_some(shown_caret),
                    caret_color: style.color,
                    selection_color: SELECTION_COLOR,
                    caret_affinity: state.affinity.get(),
                    boundaries,
                    layout,
                    on_layout: Some(on_layout),
                }),
            },
        });
    });
}
