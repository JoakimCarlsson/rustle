//! Text fields: editing, caret, selection, the clipboard, undo, what shapes input and
//! output, and the decoration around the text, as Compose's `BasicTextField` has them.

mod state;
mod transformation;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use rustle_geometry::{Offset, Rect};
use rustle_graphics::Color;
use rustle_runtime::{group, launched_effect, remember};
use rustle_text::{Affinity, TextLayout, TextStyle, TextWrap};
use rustle_ui::{
    CharPosition, Content, Focus, Key, KeyEvent, Modifier, Node, Role, ScrollState, Semantics,
    TextEditing, clipboard, emit, remember_focus, remember_scroll_state,
};

use crate::LOCAL_TEXT_STYLE;
use crate::selection::{LOCAL_TEXT_SELECTION_COLORS, in_selection_container};

pub use state::{
    ChangeList, TextFieldBuffer, TextFieldState, TextFieldValue, TextRange, UndoState,
};
pub use transformation::{
    ByValue, DEFAULT_OBFUSCATION_CHARACTER, ImeAction, InputTransformation, KeyboardActionHandler,
    KeyboardOptions, KeyboardType, MaxLength, OutputTransformation, TextFieldLineLimits,
    TextObfuscationMode, Then,
};

use state::{
    UndoBehavior, byte_of, char_index, line_around, next_word, previous_word, typeable, word_around,
};
use transformation::Shown;

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

/// Draws what surrounds a field's text, Compose's `TextFieldDecorator`: it is given the
/// field's text as a composable to call once, where the text goes among the decoration,
/// such as a placeholder, a border or icons. Presses anywhere in the decoration focus the
/// field and place its caret at the nearest character.
pub type TextFieldDecorator = Rc<dyn Fn(&dyn Fn())>;

/// What [`basic_text_field_with`] takes besides the field's modifier and state: the
/// parameters of Compose's `BasicTextField` after `state` and `modifier`.
#[derive(Clone, Default)]
pub struct BasicTextFieldOptions {
    /// The field can neither be focused nor edited, Compose's `enabled = false`.
    pub disabled: bool,
    /// The text can be selected and copied but not changed.
    pub read_only: bool,
    /// Changes or refuses what the user types before it reaches the state.
    pub input_transformation: Option<Rc<dyn InputTransformation>>,
    /// Changes how the text is shown without changing it.
    pub output_transformation: Option<Rc<dyn OutputTransformation>>,
    /// The style the text is set in; the local text style when not given.
    pub text_style: Option<TextStyle>,
    /// How the keyboard behaves.
    pub keyboard_options: KeyboardOptions,
    /// What the field's action does, Enter in a single line.
    pub on_keyboard_action: Option<KeyboardActionHandler>,
    /// How many lines the field shows.
    pub line_limits: TextFieldLineLimits,
    /// Called with the shaped text each time it is laid out, Compose's `onTextLayout`.
    /// Offsets in it are bytes of the text shown.
    pub on_text_layout: Option<Rc<dyn Fn(Rc<TextLayout>)>>,
    /// The color of the caret, Compose's `cursorBrush`; the text's color when not given.
    pub cursor_color: Option<Color>,
    /// What surrounds the text.
    pub decorator: Option<TextFieldDecorator>,
    /// The scroll state of the text, Compose's `scrollState`: down for several lines,
    /// across for one. Remembered at the call site when not given.
    pub scroll_state: Option<ScrollState>,
    /// The field's focus, to request or free it from outside, Compose's `FocusRequester`;
    /// remembered at the call site when not given.
    pub focus: Option<Focus>,
}

/// Everything a field's text, keys and pointer go by, gathered from either API.
#[derive(Clone)]
struct Settings {
    /// The text can be selected and copied but not changed.
    read_only: bool,
    /// The field takes neither focus nor presses.
    disabled: bool,
    /// Enter breaks lines and Up and Down move between them.
    multiline: bool,
    /// Long lines stay whole and scroll sideways.
    no_wrap: bool,
    /// The fewest and most lines tall a multiline field is, when bounded.
    lines: Option<(usize, usize)>,
    /// What shapes typed input.
    input: Option<Rc<dyn InputTransformation>>,
    /// What shapes the text shown.
    output: Option<Rc<dyn OutputTransformation>>,
    /// The character a secure field shows for each one hidden.
    obfuscation: Option<char>,
    /// Whether the text may not be copied or cut, as a secure field's may not.
    secure: bool,
    /// The style the text is set in, when given.
    text_style: Option<TextStyle>,
    /// The color of the caret, when given.
    cursor_color: Option<Color>,
    /// How the keyboard behaves.
    keyboard: KeyboardOptions,
    /// What Enter does in a single line.
    on_action: Option<KeyboardActionHandler>,
    /// Told the shaped text after each layout.
    on_text_layout: Option<Rc<dyn Fn(Rc<TextLayout>)>>,
    /// What surrounds the text.
    decorator: Option<TextFieldDecorator>,
    /// The scroll state given from outside.
    scroll: Option<ScrollState>,
}

impl Settings {
    /// The settings of the older API's `options`.
    fn of_options(options: TextFieldOptions) -> Self {
        Self {
            read_only: options.read_only,
            disabled: false,
            multiline: options.multiline,
            no_wrap: options.no_wrap,
            lines: None,
            input: None,
            output: None,
            obfuscation: None,
            secure: false,
            text_style: None,
            cursor_color: None,
            keyboard: KeyboardOptions::default(),
            on_action: None,
            on_text_layout: None,
            decorator: None,
            scroll: None,
        }
    }

    /// The settings `options` give.
    fn of_basic(options: BasicTextFieldOptions) -> Self {
        let lines = match options.line_limits {
            TextFieldLineLimits::SingleLine => None,
            TextFieldLineLimits::MultiLine {
                min_height_in_lines,
                max_height_in_lines,
            } => Some((
                min_height_in_lines.max(1),
                max_height_in_lines.max(min_height_in_lines.max(1)),
            )),
        };
        Self {
            read_only: options.read_only || options.disabled,
            disabled: options.disabled,
            multiline: lines.is_some(),
            no_wrap: false,
            lines,
            input: options.input_transformation,
            output: options.output_transformation,
            obfuscation: None,
            secure: false,
            text_style: options.text_style,
            cursor_color: options.cursor_color,
            keyboard: options.keyboard_options,
            on_action: options.on_keyboard_action,
            on_text_layout: options.on_text_layout,
            decorator: options.decorator,
            scroll: options.scroll_state,
        }
    }
}

/// The shaped text of the field, filled in by layout.
type LayoutCell = Rc<RefCell<Option<Rc<TextLayout>>>>;

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
/// single line, also scroll across. Read-only text is as big as its content. A field
/// bounded to some lines is as tall as its lines within those bounds.
fn scrolling(
    settings: &Settings,
    line_height: f32,
    across: &ScrollState,
    down: &ScrollState,
) -> Modifier {
    if settings.read_only && settings.multiline && !settings.disabled {
        Modifier::new()
    } else if let Some((least, most)) = settings.lines {
        let most = if most == usize::MAX {
            f32::INFINITY
        } else {
            most as f32 * line_height
        };
        Modifier::new()
            .fill_max_width()
            .height_in(least as f32 * line_height, most)
            .vertical_scroll(down)
    } else if settings.multiline && !settings.no_wrap {
        Modifier::new().fill_max_size().vertical_scroll(down)
    } else if settings.multiline {
        Modifier::new()
            .fill_max_size()
            .vertical_scroll(down)
            .horizontal_scroll(across)
    } else {
        Modifier::new().fill_max_width().horizontal_scroll(across)
    }
}

/// How far the pointer selects: by character, word or line, from the press that began it.
#[derive(Debug, Clone, Copy, Default)]
struct PressUnit {
    /// The characters the press took hold of.
    held: TextRange,
    /// How many presses in a row it was: 1 by character, 2 by word, 3 by line.
    count: u32,
}

impl TextFieldState {
    /// Makes a change the user asked for: `change` edits a buffer of the value, an input
    /// transformation sees it when it changed the text, and what is left becomes the value,
    /// recorded in the history as `behavior` says.
    fn user_edit(
        &self,
        settings: &Settings,
        behavior: UndoBehavior,
        change: impl FnOnce(&mut TextFieldBuffer),
    ) {
        let mut buffer = TextFieldBuffer::new(self.value());
        change(&mut buffer);
        if buffer.as_str() != buffer.original_text()
            && let Some(transformation) = &settings.input
        {
            transformation.transform_input(&mut buffer);
        }
        self.commit(buffer, behavior);
    }

    /// Moves the caret to `to`, dragging the selection along if `extend` is set.
    fn move_caret(&self, to: usize, extend: bool) {
        let mut value = self.value();
        value.move_to(to, extend);
        if value != self.value() {
            self.value.set(value);
        }
    }

    /// Selects from `anchor` to `cursor`, kept within the text.
    fn select(&self, anchor: usize, cursor: usize) {
        let mut value = self.value();
        let length = value.length();
        value.anchor = anchor.min(length);
        value.cursor = cursor.min(length);
        if value != self.value() {
            self.value.set(value);
        }
    }

    /// Handles a key event aimed at the field. Returns whether it was used.
    /// A read-only field moves, selects and copies but does not change.
    fn handle_key(
        &self,
        event: &KeyEvent,
        settings: &Settings,
        layout: &LayoutCell,
        shown: &Rc<RefCell<Shown>>,
    ) -> bool {
        if !event.pressed {
            return false;
        }
        let read_only = settings.read_only;
        let multiline = settings.multiline;
        let shift = event.modifiers.shift;
        let command = event.modifiers.command();
        let mac = cfg!(target_os = "macos");
        let option = mac
            && event.modifiers.alt
            && matches!(
                event.key,
                Key::Backspace | Key::Delete | Key::ArrowLeft | Key::ArrowRight
            );
        if event.modifiers.alt && !option && !matches!(event.key, Key::Other) {
            return false;
        }
        let word = command || option;
        let value = self.value();
        let length = value.length();
        let mut affinity = Affinity::Downstream;
        let mut goal_x = None;
        let along_layout = |step: &dyn Fn(&TextLayout, usize, Affinity) -> (usize, Affinity)| {
            let layout = layout.borrow().clone()?;
            let shown = shown.borrow();
            let byte = byte_of(layout.text(), shown.to_shown(value.cursor));
            let (to, side) = step(&layout, byte, self.affinity.get());
            Some((shown.to_value(char_index(layout.text(), to)), side))
        };
        let handled = match &event.key {
            Key::ArrowLeft => {
                match value.selection() {
                    Some((start, _)) if !shift && !word => self.move_caret(start, false),
                    _ if word => self.move_caret(previous_word(&value.text, value.cursor), shift),
                    _ => self.move_caret(value.cursor.saturating_sub(1), shift),
                }
                true
            }
            Key::ArrowRight => {
                match value.selection() {
                    Some((_, end)) if !shift && !word => self.move_caret(end, false),
                    _ if word => self.move_caret(next_word(&value.text, value.cursor), shift),
                    _ => self.move_caret(value.cursor + 1, shift),
                }
                true
            }
            Key::Home | Key::End if multiline && !command => {
                let home = event.key == Key::Home;
                let edge = along_layout(&|layout, byte, at| {
                    if home {
                        layout.line_start(byte, at)
                    } else {
                        layout.line_end(byte, at)
                    }
                });
                match edge {
                    Some((to, side)) => {
                        self.move_caret(to, shift);
                        affinity = side;
                    }
                    None if home => self.move_caret(value.line_start(), shift),
                    None => self.move_caret(value.line_end(), shift),
                }
                true
            }
            Key::Home => {
                self.move_caret(0, shift);
                true
            }
            Key::End => {
                self.move_caret(length, shift);
                true
            }
            Key::ArrowUp | Key::ArrowDown if multiline => {
                let up = event.key == Key::ArrowUp;
                let moved = (|| {
                    let layout = layout.borrow().clone()?;
                    let shown = shown.borrow();
                    let byte = byte_of(layout.text(), shown.to_shown(value.cursor));
                    let at = self.affinity.get();
                    let x = self
                        .goal_x
                        .get()
                        .unwrap_or_else(|| layout.caret(byte, at).x);
                    goal_x = Some(x);
                    Some(
                        layout
                            .move_vertically(byte, at, if up { -1 } else { 1 }, Some(x))
                            .map(|(to, side)| {
                                (shown.to_value(char_index(layout.text(), to)), side)
                            }),
                    )
                })();
                match moved {
                    Some(Some((to, side))) => {
                        self.move_caret(to, shift);
                        affinity = side;
                    }
                    Some(None) => self.move_caret(if up { 0 } else { length }, shift),
                    None => {}
                }
                true
            }
            Key::Enter if multiline && !read_only => {
                self.user_edit(settings, UndoBehavior::NeverMerge, |buffer| {
                    type_into(buffer, "\n", true)
                });
                true
            }
            Key::Enter if !multiline => self.keyboard_action(settings),
            Key::Backspace | Key::Delete if read_only => false,
            Key::Backspace => {
                self.user_edit(settings, UndoBehavior::NeverMerge, |buffer| {
                    let selection = buffer.selection();
                    if !selection.collapsed() {
                        buffer.delete(selection.min(), selection.max());
                    } else if selection.end > 0 {
                        let from = if word {
                            previous_word(buffer.as_str(), selection.end)
                        } else {
                            selection.end - 1
                        };
                        buffer.delete(from, selection.end);
                    }
                });
                true
            }
            Key::Delete => {
                self.user_edit(settings, UndoBehavior::NeverMerge, |buffer| {
                    let selection = buffer.selection();
                    if !selection.collapsed() {
                        buffer.delete(selection.min(), selection.max());
                    } else if selection.end < buffer.length() {
                        let to = if word {
                            next_word(buffer.as_str(), selection.end)
                        } else {
                            selection.end + 1
                        };
                        buffer.delete(selection.end, to);
                    }
                });
                true
            }
            Key::Character(letter) if command => {
                self.shortcut(&letter.to_lowercase(), shift, settings)
            }
            _ if command || read_only => false,
            _ => match &event.text {
                Some(text) if !text.chars().all(char::is_control) => {
                    self.user_edit(settings, UndoBehavior::MergeTyping, |buffer| {
                        type_into(buffer, text, multiline)
                    });
                    true
                }
                _ => false,
            },
        };
        if handled {
            if self.affinity.get() != affinity {
                self.affinity.set(affinity);
            }
            self.goal_x.set(goal_x);
        }
        handled
    }

    /// Runs the field's keyboard action, Enter in a single line: the handler given, with
    /// the default of running the submit handler, or that default alone. Returns whether
    /// anything handled it.
    fn keyboard_action(&self, settings: &Settings) -> bool {
        if settings.keyboard.ime_action == ImeAction::None {
            return false;
        }
        let submit = self.submit.borrow().clone();
        let default = move || {
            if let Some(submit) = &submit {
                submit();
            }
        };
        match &settings.on_action {
            Some(action) => {
                action(&default);
                true
            }
            None if self.submit.borrow().is_some() => {
                default();
                true
            }
            None => false,
        }
    }

    /// Carries out the shortcut of Command or Control with `letter`, Shift held when
    /// `shift`: select all, copy, cut, paste, undo and redo. Returns whether it was one.
    fn shortcut(&self, letter: &str, shift: bool, settings: &Settings) -> bool {
        let read_only = settings.read_only;
        match letter {
            "a" => {
                let length = self.value().length();
                self.select(0, length);
                true
            }
            "c" | "x" if settings.secure => true,
            "c" => {
                if let (Some(text), Some(clipboard)) = (self.value().selected_text(), clipboard()) {
                    clipboard.set_text(text);
                }
                true
            }
            "x" | "v" | "z" | "y" if read_only => false,
            "x" => {
                let value = self.value();
                if let (Some(text), Some(clipboard)) = (value.selected_text(), clipboard()) {
                    clipboard.set_text(text);
                    self.user_edit(settings, UndoBehavior::NeverMerge, |buffer| {
                        let selection = buffer.selection();
                        buffer.delete(selection.min(), selection.max());
                    });
                }
                true
            }
            "v" => {
                if let Some(text) = clipboard().and_then(|clipboard| clipboard.text()) {
                    let multiline = settings.multiline;
                    self.user_edit(settings, UndoBehavior::NeverMerge, |buffer| {
                        type_into(buffer, &text, multiline)
                    });
                }
                true
            }
            "z" if shift => {
                self.undo_state().redo();
                true
            }
            "z" => {
                self.undo_state().undo();
                true
            }
            "y" => {
                self.undo_state().redo();
                true
            }
            _ => false,
        }
    }

    /// Takes a primary press `count` times in a row at character `index`: places the caret,
    /// or with `extend` moves it there keeping the anchor; a second press selects the word
    /// there and a third the line. Returns what the press took hold of, for a drag to extend.
    fn press_at(&self, index: usize, count: u32, extend: bool) -> PressUnit {
        let value = self.value();
        let held = match count {
            2 => word_around(&value.text, index),
            3 => line_around(&value.text, index),
            _ => TextRange::collapsed_at(index),
        };
        if extend && count == 1 {
            self.move_caret(index, true);
            return PressUnit {
                held: TextRange::collapsed_at(value.anchor),
                count,
            };
        }
        self.select(held.min(), held.max());
        PressUnit { held, count }
    }

    /// Extends the selection the press `unit` began to character `index`, by the press's
    /// unit: characters, words or lines.
    fn drag_to(&self, index: usize, unit: PressUnit) {
        if unit.count < 2 {
            self.move_caret(index, true);
            return;
        }
        let text = self.text();
        let around = if unit.count == 2 {
            word_around(&text, index)
        } else {
            line_around(&text, index)
        };
        if around.min() < unit.held.min() {
            self.select(unit.held.max(), around.min());
        } else {
            self.select(unit.held.min(), around.max());
        }
    }
}

/// Types `text` over the selection of `buffer`, keeping line breaks when `multiline`,
/// and leaves the caret after it.
fn type_into(buffer: &mut TextFieldBuffer, text: &str, multiline: bool) {
    let text = typeable(text, multiline);
    let selection = buffer.selection();
    buffer.replace(selection.min(), selection.max(), &text);
    buffer.place_cursor_before_char_at(selection.min() + text.chars().count());
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
            history: remember(Rc::default).get(),
            history_version: remember(|| 0_u64),
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

/// Emits an editable field with every parameter of Compose's `BasicTextField`: a single
/// line or several within [`TextFieldLineLimits`], input shaped by an
/// [`InputTransformation`], output by an [`OutputTransformation`], a
/// [`TextFieldDecorator`] around the text, a hoisted scroll state and focus, and the
/// caret color, text style and keyboard action given. Undo and redo (Command or Control
/// with Z, Shift with it or Y) walk the state's [`UndoState`].
///
/// The selection is highlighted while the field has focus, as the caret is shown.
///
/// The text scrolls to bring the caret into view when the caret or the text changes: a
/// single line while it has focus, several lines whether or not they have it. Where the
/// reader or the owner scrolled it otherwise stays until then; a scroll state the owner
/// placed away from the start before the field was first laid out is kept.
#[track_caller]
pub fn basic_text_field_with(
    modifier: Modifier,
    state: &TextFieldState,
    options: BasicTextFieldOptions,
) {
    group(|| {
        let remembered = remember_focus();
        let focus = options.focus.clone().unwrap_or(remembered);
        let settings = Settings::of_basic(options);
        let modifier = if settings.multiline {
            modifier.clip()
        } else {
            modifier
        };
        field(modifier, Modifier::new(), state, &focus, settings);
    });
}

/// Emits a field whose text is hidden, Compose's `BasicSecureTextField`: a single line
/// shown as `character` for each of its characters while `mode` hides it, that can be
/// neither copied nor cut. `options` are those of [`basic_text_field_with`]; its line
/// limits and output transformation are not used.
#[track_caller]
pub fn basic_secure_text_field(
    modifier: Modifier,
    state: &TextFieldState,
    mode: TextObfuscationMode,
    character: char,
    options: BasicTextFieldOptions,
) {
    group(|| {
        let remembered = remember_focus();
        let focus = options.focus.clone().unwrap_or(remembered);
        let mut settings = Settings::of_basic(BasicTextFieldOptions {
            line_limits: TextFieldLineLimits::SingleLine,
            output_transformation: None,
            ..options
        });
        settings.secure = true;
        settings.obfuscation = (mode == TextObfuscationMode::Hidden).then_some(character);
        field(modifier, Modifier::new(), state, &focus, settings);
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
        if in_selection_container() {
            crate::text::text_with(modifier, content);
            return;
        }
        let state = remember_text_field_state(content);
        if state.text() != content {
            state.value.set(TextFieldValue {
                text: content.to_string(),
                cursor: content.chars().count(),
                anchor: content.chars().count(),
            });
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
    field(outer, inner, state, focus, Settings::of_options(options));
}

/// Where a field's text and the area around it were placed in the window, for turning
/// a press on its decoration into a place in its text.
#[derive(Debug, Default, Clone, Copy)]
struct Placed {
    /// The area that takes presses.
    area: Option<Rect>,
    /// The text's visible part, before it is scrolled.
    text: Option<Rect>,
}

/// Emits a field as `settings` say: one node holding the text with `outer` and `inner`
/// around it, or, with a decorator, an area with `outer` that takes focus and presses
/// around the decoration, the text placed where the decorator calls for it.
#[track_caller]
fn field(
    outer: Modifier,
    inner: Modifier,
    state: &TextFieldState,
    focus: &Focus,
    settings: Settings,
) {
    group(|| {
        let boundaries = remember(|| Rc::new(RefCell::new(Vec::<CharPosition>::new()))).get();
        let layout: LayoutCell = remember(|| Rc::new(RefCell::new(None))).get();
        let shown_cell = remember(|| Rc::new(RefCell::new(Shown::default()))).get();
        let placed = remember(|| Rc::new(Cell::new(Placed::default()))).get();
        let unit = remember(|| Rc::new(Cell::new(PressUnit::default()))).get();
        let value = state.value();
        let read_only = settings.read_only;
        let multiline = settings.multiline;
        let base = settings
            .text_style
            .unwrap_or_else(|| LOCAL_TEXT_STYLE.current());
        let style = if multiline && !settings.no_wrap {
            base
        } else {
            base.wrap(TextWrap::None)
        };
        let shown = Shown::of(&value, settings.output.as_ref(), settings.obfuscation);
        let caret = shown.to_shown(value.cursor);
        let selection = value
            .selection()
            .map(|(start, end)| (shown.to_shown(start), shown.to_shown(end)));

        let focused = focus.is_focused();
        let preedit = focus
            .preedit()
            .filter(|preedit| focused && !read_only && !preedit.text.is_empty());
        let (text, underline, shown_caret) = match &preedit {
            Some(preedit) => {
                let mut text = shown.text.clone();
                let composed = match settings.obfuscation {
                    Some(character) => {
                        std::iter::repeat_n(character, preedit.text.chars().count()).collect()
                    }
                    None => preedit.text.clone(),
                };
                text.insert_str(byte_of(&shown.text, caret), &composed);
                let length = composed.chars().count();
                let inside = preedit
                    .cursor
                    .map(|(_, end)| preedit.text[..end.min(preedit.text.len())].chars().count());
                (
                    text,
                    Some((caret, caret + length)),
                    Some(caret + inside.unwrap_or(length)),
                )
            }
            None => (shown.text.clone(), None, Some(caret)),
        };
        *shown_cell.borrow_mut() = shown;
        let composing = preedit.is_some();
        let caret_hidden = composing && preedit.as_ref().is_some_and(|p| p.cursor.is_none());
        let shown_caret = shown_caret.unwrap_or(caret);

        let across = remember_scroll_state();
        let remembered_down = remember_scroll_state();
        let down = settings.scroll.clone().unwrap_or(remembered_down);
        let across = if multiline {
            across
        } else {
            settings.scroll.clone().unwrap_or(across)
        };
        let followed: Rc<Cell<Option<(usize, usize)>>> =
            remember(|| Rc::new(Cell::new(None))).get();
        let on_layout: Rc<dyn Fn()> = {
            let focus = focus.clone();
            let (layout, across, down) = (Rc::clone(&layout), across.clone(), down.clone());
            let affinity = state.affinity.clone();
            let told = settings.on_text_layout.clone();
            let no_wrap = settings.no_wrap;
            let followed = Rc::clone(&followed);
            Rc::new(move || {
                let Some(layout) = layout.borrow().clone() else {
                    return;
                };
                if let Some(told) = &told {
                    told(Rc::clone(&layout));
                }
                if !focus.is_focused() && !multiline {
                    followed.set(None);
                    return;
                }
                let measured = if multiline {
                    down.viewport_length()
                } else {
                    across.viewport_length()
                };
                if measured <= 0.0 {
                    return;
                }
                let seen = (shown_caret, layout.text().len());
                let first = followed.get().is_none();
                if followed.replace(Some(seen)) == Some(seen) {
                    return;
                }
                let placed = if multiline && !no_wrap {
                    down.offset()
                } else {
                    across.offset()
                };
                if first && placed > 0.0 {
                    return;
                }
                let byte = byte_of(layout.text(), shown_caret);
                let caret = layout.caret(byte, affinity.get());
                if !multiline || no_wrap {
                    follow(&across, caret.x, 8.0);
                }
                if multiline {
                    follow(&down, caret.y, caret.height);
                }
            })
        };

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

        let nearest = {
            let layout = Rc::clone(&layout);
            let shown = Rc::clone(&shown_cell);
            move |at: Offset| {
                layout
                    .borrow()
                    .clone()
                    .map_or((0, Affinity::Downstream), |layout| {
                        let (byte, side) = layout.hit_test_with_affinity(at);
                        let index = char_index(layout.text(), byte);
                        (shown.borrow().to_value(index), side)
                    })
            }
        };
        let described = {
            let semantics = Semantics::new(Role::TextInput)
                .value(if settings.obfuscation.is_some() {
                    shown_cell.borrow().text.clone()
                } else {
                    value.text.clone()
                })
                .text_selection(value.anchor, value.cursor)
                .focus(focus);
            let selecting = state.clone();
            let semantics = semantics.on_set_text_selection(move |anchor, focus| {
                selecting.select(anchor, focus);
            });
            if read_only {
                semantics
            } else {
                let setting = state.clone();
                semantics.on_set_value(move |text| setting.set_text(text))
            }
        };
        let decorated = settings.decorator.is_some();
        let to_text: Rc<dyn Fn(Offset) -> Offset> = if decorated {
            let (placed, across, down) = (Rc::clone(&placed), across.clone(), down.clone());
            Rc::new(move |at: Offset| {
                let held = placed.get();
                match (held.area, held.text) {
                    (Some(area), Some(text)) => Offset::new(
                        at.x + area.origin.x - text.origin.x + across.offset(),
                        at.y + area.origin.y - text.origin.y + down.offset(),
                    ),
                    _ => at,
                }
            })
        } else {
            Rc::new(|at| at)
        };
        let pointer = {
            let (presses, drags) = (state.clone(), state.clone());
            let (press_at, drag_to) = (nearest.clone(), nearest);
            let (press_unit, drag_unit) = (Rc::clone(&unit), Rc::clone(&unit));
            let (press_text, drag_text) = (Rc::clone(&to_text), to_text);
            Modifier::new()
                .on_press(move |press| {
                    let (index, side) = press_at(press_text(press.position));
                    if presses.affinity.get() != side {
                        presses.affinity.set(side);
                    }
                    presses.goal_x.set(None);
                    let count = press.click_count.clamp(1, 3);
                    press_unit.set(presses.press_at(index, count, press.modifiers.shift));
                })
                .on_drag(move |at| {
                    let (index, side) = drag_to(drag_text(at));
                    if drags.affinity.get() != side {
                        drags.affinity.set(side);
                    }
                    drags.drag_to(index, drag_unit.get());
                })
        };
        let keys = {
            let keys = state.clone();
            let (key_layout, key_shown) = (Rc::clone(&layout), Rc::clone(&shown_cell));
            let key_settings = settings.clone();
            move |event: &KeyEvent| keys.handle_key(event, &key_settings, &key_layout, &key_shown)
        };
        let focusable = |modifier: Modifier| {
            if settings.disabled {
                modifier.semantics(described.clone())
            } else {
                modifier
                    .semantics(described.clone())
                    .focusable(focus, keys.clone())
            }
        };
        let editing = TextEditing {
            caret: (focused && caret_on.get() && !read_only && !caret_hidden)
                .then_some(shown_caret),
            selection: if composing || (!focused && !read_only) {
                None
            } else {
                selection
            },
            underline,
            ime_caret: (focused && !read_only).then_some(shown_caret),
            caret_color: settings.cursor_color.unwrap_or(style.color),
            selection_color: LOCAL_TEXT_SELECTION_COLORS.current().background_color,
            caret_affinity: state.affinity.get(),
            boundaries,
            layout,
            on_layout: Some(on_layout),
        };
        let scrolled = scrolling(&settings, style.line_height, &across, &down);
        let Some(decorator) = settings.decorator.clone() else {
            let mut modifier = focusable(outer).chain(inner).chain(scrolled);
            if !settings.disabled {
                modifier = modifier.chain(pointer);
            }
            emit(Node {
                modifier,
                content: Content::Text {
                    content: text,
                    style,
                    editing: Some(editing),
                },
            });
            return;
        };
        let area = {
            let placed = Rc::clone(&placed);
            Modifier::new().on_placed(move |rect, _| {
                placed.set(Placed {
                    area: Some(rect),
                    ..placed.get()
                });
            })
        };
        let mut modifier = focusable(outer).chain(area);
        if !settings.disabled {
            modifier = modifier.chain(pointer);
        }
        let viewport = {
            let placed = Rc::clone(&placed);
            Modifier::new().on_placed(move |rect, _| {
                placed.set(Placed {
                    text: Some(rect),
                    ..placed.get()
                });
            })
        };
        let content = RefCell::new(Some((text, editing)));
        let inner = inner.chain(viewport).chain(scrolled);
        let inner_text_field = move || {
            if let Some((text, editing)) = content.borrow_mut().take() {
                emit(Node {
                    modifier: inner.clone(),
                    content: Content::Text {
                        content: text,
                        style,
                        editing: Some(editing),
                    },
                });
            }
        };
        crate::stack_with(modifier, || decorator(&inner_text_field));
    });
}
