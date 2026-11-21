//! Edits text fields of an [`Embedded`] interface through keys, the pointer and the
//! program: a state made outside composition, undo, input and output transformations,
//! a secure field, a decorator, selection by words and lines, and focus freed on request.

use std::cell::RefCell;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    BasicTextFieldOptions, Clipboard, Embedded, Focus, InputTransformation, Key, KeyEvent,
    MaxLength, Modifiers, Offset, PointerButton, ScrollState, TextFieldDecorator,
    TextFieldLineLimits, TextLayout, TextObfuscationMode, TextRange, basic_secure_text_field,
    basic_text_field_with, set_clipboard,
};

/// A clipboard kept in memory.
#[derive(Default)]
struct Memory(RefCell<Option<String>>);

impl Clipboard for Memory {
    /// The text kept.
    fn text(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    /// Keeps `text`.
    fn set_text(&self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_string());
    }
}

/// An interface 400 by 200 logical pixels of `root`, composed once.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((400, 200), 1.0);
    embedded.update();
    embedded
}

/// Presses and lets go of `key` with `modifiers`, then composes.
fn press(embedded: &mut Embedded, key: Key, modifiers: Modifiers) {
    embedded.key_event(KeyEvent::press(key, modifiers));
    embedded.update();
}

/// Types `text` a character at a time.
fn type_text(embedded: &mut Embedded, text: &str) {
    for character in text.chars() {
        press(
            embedded,
            Key::Character(character.to_string()),
            Modifiers::NONE,
        );
    }
}

/// Presses and lets go of the primary button at `at`, then composes.
fn click(embedded: &mut Embedded, at: Offset) {
    embedded.pointer_moved(at);
    embedded.pointer_button(PointerButton::Primary, true);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
}

/// The middle of the first line of `layout` at character `index`, offset by `origin`.
fn point(layout: &TextLayout, index: usize, origin: Offset) -> Offset {
    let byte = layout
        .text()
        .char_indices()
        .nth(index)
        .map_or(layout.text().len(), |(byte, _)| byte);
    let caret = layout.caret(byte, rustle::Affinity::Downstream);
    Offset::new(
        origin.x + caret.x + 0.5,
        origin.y + caret.y + caret.height / 2.0,
    )
}

/// A field holding a state its owner made outside composition, with its focus held
/// where the test can request and free it, and its layout recorded.
struct Owned {
    state: TextFieldState,
    focus: Rc<RefCell<Option<Focus>>>,
    layout: Rc<RefCell<Option<Rc<TextLayout>>>>,
}

impl Owned {
    /// A field holding `text`.
    fn new(text: &str) -> Self {
        Self {
            state: TextFieldState::new(text),
            focus: Rc::default(),
            layout: Rc::default(),
        }
    }

    /// A root showing the field at the top left with `options`, its focus and layout kept.
    fn root(&self, options: BasicTextFieldOptions) -> impl FnMut() + 'static {
        let (state, focus, layout) = (
            self.state.clone(),
            Rc::clone(&self.focus),
            Rc::clone(&self.layout),
        );
        move || {
            let held = remember_focus();
            *focus.borrow_mut() = Some(held.clone());
            let layout = Rc::clone(&layout);
            basic_text_field_with(
                Modifier::new().width(300.0),
                &state,
                BasicTextFieldOptions {
                    focus: Some(held),
                    on_text_layout: Some(Rc::new(move |told| *layout.borrow_mut() = Some(told))),
                    ..options.clone()
                },
            );
        }
    }

    /// Asks for focus on the field and composes.
    fn focus(&self, embedded: &mut Embedded) {
        if let Some(focus) = self.focus.borrow().as_ref() {
            focus.request_focus();
        }
        embedded.update();
    }
}

/// Keys type into a state made outside composition; Control with Backspace erases a word,
/// undo takes typing back as one step and redo puts it again.
#[test]
fn keys_edit_a_state_its_owner_made_and_undo_walks_back() {
    let owned = Owned::new("");
    let mut embedded = interface(owned.root(BasicTextFieldOptions::default()));
    owned.focus(&mut embedded);
    type_text(&mut embedded, "one two");
    assert_eq!(owned.state.text(), "one two");
    press(&mut embedded, Key::Backspace, Modifiers::CTRL);
    assert_eq!(owned.state.text(), "one ");
    press(&mut embedded, Key::Character("z".into()), Modifiers::CTRL);
    assert_eq!(owned.state.text(), "one two");
    press(&mut embedded, Key::Character("z".into()), Modifiers::CTRL);
    assert_eq!(owned.state.text(), "");
    press(&mut embedded, Key::Character("y".into()), Modifiers::CTRL);
    assert_eq!(owned.state.text(), "one two");
    press(&mut embedded, Key::ArrowLeft, Modifiers::CTRL);
    type_text(&mut embedded, "x");
    assert_eq!(owned.state.text(), "one xtwo");
    owned.state.edit(|buffer| {
        buffer.replace(0, 3, "ONE");
        buffer.place_cursor_at_end();
    });
    assert_eq!(owned.state.text(), "ONE xtwo");
    assert!(owned.state.undo_state().can_undo());
}

/// An input transformation refuses typing past its length, and the program's own edits
/// pass it by.
#[test]
fn an_input_transformation_shapes_typing_only() {
    let owned = Owned::new("");
    let limit: Rc<dyn InputTransformation> = Rc::new(MaxLength(3));
    let mut embedded = interface(owned.root(BasicTextFieldOptions {
        input_transformation: Some(limit),
        ..BasicTextFieldOptions::default()
    }));
    owned.focus(&mut embedded);
    type_text(&mut embedded, "abcdef");
    assert_eq!(owned.state.text(), "abc");
    owned.state.set_text("longer");
    assert_eq!(owned.state.text(), "longer");
}

/// A secure field shows a dot for each character and neither copies nor cuts.
#[test]
fn a_secure_field_hides_its_text_and_keeps_it_off_the_clipboard() {
    let memory = Rc::new(Memory::default());
    let state = TextFieldState::new("secret");
    let layout: Rc<RefCell<Option<Rc<TextLayout>>>> = Rc::default();
    let focus: Rc<RefCell<Option<Focus>>> = Rc::default();
    let root = {
        let (state, layout, focus) = (state.clone(), Rc::clone(&layout), Rc::clone(&focus));
        move || {
            let held = remember_focus();
            *focus.borrow_mut() = Some(held.clone());
            let layout = Rc::clone(&layout);
            basic_secure_text_field(
                Modifier::new().width(300.0),
                &state,
                TextObfuscationMode::Hidden,
                '*',
                BasicTextFieldOptions {
                    focus: Some(held),
                    on_text_layout: Some(Rc::new(move |told| *layout.borrow_mut() = Some(told))),
                    ..BasicTextFieldOptions::default()
                },
            );
        }
    };
    let mut embedded = interface(root);
    set_clipboard(Rc::clone(&memory) as Rc<dyn Clipboard>);
    focus.borrow().as_ref().expect("composed").request_focus();
    embedded.update();
    let shown = layout.borrow().clone().expect("laid out");
    assert_eq!(shown.text(), "******");
    press(&mut embedded, Key::Character("a".into()), Modifiers::CTRL);
    press(&mut embedded, Key::Character("c".into()), Modifiers::CTRL);
    press(&mut embedded, Key::Character("x".into()), Modifiers::CTRL);
    assert_eq!(memory.text(), None);
    assert_eq!(state.text(), "secret");
}

/// A press in the decoration focuses the field and puts the caret at the nearest
/// character; the decorator places the text where it calls for it.
#[test]
fn a_press_on_the_decoration_focuses_the_field() {
    let owned = Owned::new("hello");
    let decorator: TextFieldDecorator = Rc::new(|inner: &dyn Fn()| {
        column_with(Modifier::new().padding(20.0), || {
            text("label");
            inner();
        });
    });
    let mut embedded = interface(owned.root(BasicTextFieldOptions {
        decorator: Some(decorator),
        ..BasicTextFieldOptions::default()
    }));
    click(&mut embedded, Offset::new(5.0, 5.0));
    assert!(embedded.has_focus());
    assert_eq!(owned.state.selection(), TextRange::collapsed_at(0));
    let layout = owned.layout.borrow().clone().expect("laid out");
    assert_eq!(layout.text(), "hello");
    type_text(&mut embedded, "x");
    assert_eq!(owned.state.text(), "xhello");
}

/// Two presses select a word and three the line; a press with Shift moves the caret
/// keeping the anchor.
#[test]
fn presses_select_words_and_lines_and_shift_extends() {
    let owned = Owned::new("first second third");
    let mut embedded = interface(owned.root(BasicTextFieldOptions::default()));
    embedded.update();
    let layout = owned.layout.borrow().clone().expect("laid out");
    let inside = point(&layout, 8, Offset::new(0.0, 0.0));
    click(&mut embedded, inside);
    click(&mut embedded, inside);
    assert_eq!(owned.state.selected_text(), "second");
    click(&mut embedded, inside);
    assert_eq!(owned.state.selected_text(), "first second third");
    click(&mut embedded, point(&layout, 2, Offset::new(0.0, 0.0)));
    embedded.set_modifiers(Modifiers::SHIFT);
    click(&mut embedded, point(&layout, 12, Offset::new(0.0, 0.0)));
    embedded.set_modifiers(Modifiers::NONE);
    assert_eq!(owned.state.selection(), TextRange::new(2, 12));
}

/// Freeing the field's focus takes focus away from it.
#[test]
fn freeing_focus_lets_go_of_the_field() {
    let owned = Owned::new("");
    let mut embedded = interface(owned.root(BasicTextFieldOptions::default()));
    owned.focus(&mut embedded);
    assert!(embedded.has_focus());
    owned
        .focus
        .borrow()
        .as_ref()
        .expect("composed")
        .free_focus();
    embedded.update();
    assert!(!embedded.has_focus());
}

/// A field of several lines grows to its most lines and then scrolls; Enter breaks lines.
#[test]
fn line_limits_bound_how_tall_a_field_grows() {
    let owned = Owned::new("");
    let heights: Rc<RefCell<Vec<f32>>> = Rc::default();
    let root = {
        let (state, focus, heights) = (
            owned.state.clone(),
            Rc::clone(&owned.focus),
            Rc::clone(&heights),
        );
        move || {
            let held = remember_focus();
            *focus.borrow_mut() = Some(held.clone());
            let heights = Rc::clone(&heights);
            basic_text_field_with(
                Modifier::new()
                    .width(300.0)
                    .on_size_changed(move |size| heights.borrow_mut().push(size.height)),
                &state,
                BasicTextFieldOptions {
                    focus: Some(held),
                    line_limits: TextFieldLineLimits::MultiLine {
                        min_height_in_lines: 1,
                        max_height_in_lines: 2,
                    },
                    ..BasicTextFieldOptions::default()
                },
            );
        }
    };
    let mut embedded = interface(root);
    owned.focus(&mut embedded);
    let one = *heights.borrow().last().expect("measured");
    type_text(&mut embedded, "a");
    press(&mut embedded, Key::Enter, Modifiers::NONE);
    type_text(&mut embedded, "b");
    let two = *heights.borrow().last().expect("measured");
    press(&mut embedded, Key::Enter, Modifiers::NONE);
    type_text(&mut embedded, "c");
    let three = *heights.borrow().last().expect("measured");
    assert_eq!(owned.state.text(), "a\nb\nc");
    assert!(two > one, "{one} then {two}");
    assert_eq!(three, two);
}

/// A field scrolled by the wheel stays where the reader scrolled it until the caret
/// moves or the text changes, which bring the caret back into view.
#[test]
fn the_wheel_scroll_holds_until_an_edit() {
    let owned = Owned::new(&"line\n".repeat(40));
    let scroll = ScrollState::default();
    let mut embedded = interface(owned.root(BasicTextFieldOptions {
        line_limits: TextFieldLineLimits::MultiLine {
            min_height_in_lines: 1,
            max_height_in_lines: 4,
        },
        scroll_state: Some(scroll.clone()),
        ..BasicTextFieldOptions::default()
    }));
    owned.focus(&mut embedded);
    embedded.update();
    let followed = scroll.offset();
    assert!(followed > 0.0);
    embedded.pointer_moved(Offset::new(10.0, 10.0));
    embedded.wheel(Offset::new(0.0, 60.0));
    embedded.update();
    embedded.update();
    let scrolled = scroll.offset();
    assert!(scrolled < followed, "{followed} then {scrolled}");
    type_text(&mut embedded, "x");
    embedded.update();
    assert_eq!(scroll.offset(), followed);
}

/// A selection is highlighted only while its field has focus.
#[test]
fn the_selection_shows_while_the_field_has_focus() {
    let owned = Owned::new("selected");
    owned.state.select_all();
    let mut embedded = interface(owned.root(BasicTextFieldOptions::default()));
    let highlighted = |embedded: &Embedded| {
        embedded
            .commands()
            .iter()
            .filter(|command| matches!(command, rustle::DrawCommand::Rect { .. }))
            .count()
    };
    let unfocused = highlighted(&embedded);
    assert_eq!(unfocused, 0);
    owned.focus(&mut embedded);
    embedded.update();
    assert!(embedded.has_focus());
    let focused = highlighted(&embedded);
    assert!(focused > unfocused, "{unfocused} then {focused}");
}

/// The text context menu provided around a field is given what the field can do on its
/// text: copy and cut its selection to the clipboard, paste over it as typing does, and
/// select it all, each left out while it would not act.
#[test]
fn a_text_context_menu_is_given_the_operations_of_its_field() {
    let memory = Rc::new(Memory::default());
    set_clipboard(Rc::clone(&memory) as Rc<dyn Clipboard>);
    let state = rustle::TextFieldState::with_selection("hello world", TextRange::new(0, 5));
    let given: Rc<RefCell<Option<rustle::TextManager>>> = Rc::default();
    let (field, kept) = (state.clone(), Rc::clone(&given));
    let mut ui = interface(move || {
        let kept = Rc::clone(&kept);
        let menu = rustle::TextContextMenu::new(move |manager, content| {
            *kept.borrow_mut() = Some(manager);
            content();
        });
        rustle::LOCAL_TEXT_CONTEXT_MENU.provide(menu, || {
            basic_text_field_with(
                Modifier::new().size(300.0, 30.0),
                &field,
                BasicTextFieldOptions::default(),
            );
        });
    });
    let manager = given
        .borrow()
        .clone()
        .expect("the menu was given the field");
    assert_eq!(manager.selected_text, "hello");
    (manager.copy.expect("a selection copies"))();
    assert_eq!(memory.text().as_deref(), Some("hello"));
    (manager.cut.expect("a selection cuts"))();
    ui.update();
    assert_eq!(state.text(), " world");
    memory.set_text("a\nb");
    let manager = given.borrow().clone().expect("composed again");
    assert!(
        manager.copy.is_none() && manager.cut.is_none(),
        "nothing selected"
    );
    (manager.paste.expect("an editable field pastes"))();
    ui.update();
    assert_eq!(
        state.text(),
        "ab world",
        "a single line drops the line break"
    );
    let manager = given.borrow().clone().expect("composed again");
    (manager.select_all.expect("not all selected"))();
    ui.update();
    assert_eq!(state.selected_text(), "ab world");
    let manager = given.borrow().clone().expect("composed again");
    assert!(manager.select_all.is_none(), "all of it is selected");
}

/// A secure field offers neither copy nor cut, nor its selected text.
#[test]
fn a_secure_field_keeps_its_text_from_the_context_menu() {
    let state = rustle::TextFieldState::with_selection("secret", TextRange::new(0, 6));
    let given: Rc<RefCell<Option<rustle::TextManager>>> = Rc::default();
    let (field, kept) = (state.clone(), Rc::clone(&given));
    let _ui = interface(move || {
        let kept = Rc::clone(&kept);
        let menu = rustle::TextContextMenu::new(move |manager, content| {
            *kept.borrow_mut() = Some(manager);
            content();
        });
        rustle::LOCAL_TEXT_CONTEXT_MENU.provide(menu, || {
            basic_secure_text_field(
                Modifier::new().size(300.0, 30.0),
                &field,
                TextObfuscationMode::Hidden,
                rustle::DEFAULT_OBFUSCATION_CHARACTER,
                BasicTextFieldOptions::default(),
            );
        });
    });
    let manager = given
        .borrow()
        .clone()
        .expect("the menu was given the field");
    assert!(manager.copy.is_none() && manager.cut.is_none());
    assert!(manager.selected_text.is_empty());
    assert!(manager.paste.is_some());
}
