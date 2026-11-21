//! Annotated text in an [`Embedded`] interface as Compose's `BasicText(AnnotatedString)`
//! sets it: links pressed with the pointer and the keyboard and restyled while hovered,
//! addresses opened through the uri handler, inline content set in the room its placeholder
//! keeps and copied as its alternate text, and paragraphs with line heights of their own.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    AnnotatedString, AppendInlineContent, Color, DrawCommand, Embedded, InlineTextContent, Key,
    KeyEvent, LOCAL_URI_HANDLER, LinkAnnotation, Modifiers, Offset, ParagraphStyle, Placeholder,
    PlaceholderVerticalAlign, PointerButton, Rect, SelectionState, SpanStyle, TextLinkStyles,
    TextStyle, TextUnit, UriHandler, basic_text_annotated, build_annotated_string,
    remember_selection_state, selection_container_with,
};

/// The color links are drawn in at rest.
const BLUE: Color = Color::rgb(0, 0, 255);

/// The color links are drawn in while hovered.
const RED: Color = Color::rgb(255, 0, 0);

/// The style every text of these tests is set in.
fn style() -> TextStyle {
    TextStyle::default()
}

/// An interface 400 by 200 logical pixels of `root`, composed once.
fn interface(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((400, 200), 1.0);
    embedded.update();
    embedded
}

/// Clicks the primary button at `at`, then composes.
fn click(embedded: &mut Embedded, at: Offset) {
    embedded.pointer_moved(at);
    embedded.pointer_button(PointerButton::Primary, true);
    embedded.pointer_button(PointerButton::Primary, false);
    embedded.update();
}

/// Sends `pressed` going down and coming up, then composes.
fn stroke(embedded: &mut Embedded, pressed: Key) {
    for down in [true, false] {
        embedded.key_event(KeyEvent {
            pressed: down,
            ..KeyEvent::press(pressed.clone(), Modifiers::NONE)
        });
        embedded.update();
    }
}

/// Where the middle of the byte `offset` of `text`, set on one line in [`style`], is drawn
/// when the text starts at the top-left corner.
fn point_in(text: &str, offset: usize) -> Offset {
    let layout = rustle::layout_text(text, &style(), None);
    let x = layout.caret(offset, rustle::Affinity::Downstream).x;
    Offset::new(x + 2.0, style().line_height / 2.0)
}

/// "see docs here", "docs" a link that records its tag into `pressed`, blue at rest and red
/// while hovered.
fn linked(pressed: &Rc<RefCell<Vec<String>>>) -> AnnotatedString {
    let pressed = Rc::clone(pressed);
    let link = LinkAnnotation::clickable("docs", move |link| {
        if let LinkAnnotation::Clickable { tag, .. } = link {
            pressed.borrow_mut().push(tag.clone());
        }
    })
    .with_styles(
        TextLinkStyles::new(SpanStyle::new().color(BLUE))
            .hovered_style(SpanStyle::new().color(RED)),
    );
    build_annotated_string(|builder| {
        builder.append("see ");
        builder.with_link(link, |builder| builder.append("docs"));
        builder.append(" here");
    })
}

/// The colors the runs saying `words` were drawn in.
fn colors_of(embedded: &Embedded, words: &str) -> Vec<Color> {
    embedded
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::RichText { spans, .. } => Some(spans.clone()),
            _ => None,
        })
        .flatten()
        .filter(|span| span.text == words)
        .map(|span| span.color)
        .collect()
}

/// A press on a link tells its listener, a press beside it does not, and the link is drawn
/// in its hovered style while the pointer is over it.
#[test]
fn a_link_is_pressed_and_hovered() {
    let pressed: Rc<RefCell<Vec<String>>> = Rc::default();
    let text = linked(&pressed);
    let mut ui = interface(move || {
        basic_text_annotated(Modifier::new(), &text, style(), &HashMap::new());
    });
    assert_eq!(colors_of(&ui, "docs"), [BLUE]);
    click(&mut ui, point_in("see docs here", 1));
    assert!(pressed.borrow().is_empty(), "the press missed the link");
    ui.pointer_moved(point_in("see docs here", 5));
    ui.update();
    assert_eq!(colors_of(&ui, "docs"), [RED]);
    click(&mut ui, point_in("see docs here", 5));
    assert_eq!(*pressed.borrow(), ["docs"]);
    ui.pointer_moved(point_in("see docs here", 11));
    ui.update();
    assert_eq!(colors_of(&ui, "docs"), [BLUE]);
}

/// Tab reaches a link and Enter presses it, as it does a clickable.
#[test]
fn a_link_is_reached_with_tab_and_pressed_with_enter() {
    let pressed: Rc<RefCell<Vec<String>>> = Rc::default();
    let text = linked(&pressed);
    let mut ui = interface(move || {
        basic_text_annotated(Modifier::new(), &text, style(), &HashMap::new());
    });
    stroke(&mut ui, Key::Tab);
    assert!(ui.has_focus());
    stroke(&mut ui, Key::Enter);
    assert_eq!(*pressed.borrow(), ["docs"]);
}

/// A link to an address without a listener opens it through the uri handler in effect.
#[test]
fn a_url_link_opens_through_the_uri_handler() {
    let opened: Rc<RefCell<Vec<String>>> = Rc::default();
    let handler = {
        let opened = Rc::clone(&opened);
        UriHandler::new(move |uri| opened.borrow_mut().push(uri.to_owned()))
    };
    let text = build_annotated_string(|builder| {
        builder.with_link(LinkAnnotation::url("https://example.com"), |builder| {
            builder.append("example")
        });
    });
    let mut ui = interface(move || {
        let text = text.clone();
        LOCAL_URI_HANDLER.provide(handler.clone(), move || {
            basic_text_annotated(Modifier::new(), &text, style(), &HashMap::new());
        });
    });
    click(&mut ui, point_in("example", 2));
    assert_eq!(*opened.borrow(), ["https://example.com"]);
}

/// A link that runs over two lines is pressed on either.
#[test]
fn a_link_over_two_lines_is_pressed_on_both() {
    let pressed: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = Rc::clone(&pressed);
    let words = "one two three four five six seven eight nine ten eleven twelve";
    let text = build_annotated_string(|builder| {
        let link =
            LinkAnnotation::clickable("long", move |_| seen.borrow_mut().push("long".into()));
        builder.with_link(link, |builder| builder.append(words));
    });
    let width = rustle::layout_text(words, &style(), None).size().width * 0.6;
    let mut ui = interface(move || {
        basic_text_annotated(
            Modifier::new().width(width),
            &text,
            style(),
            &HashMap::new(),
        );
    });
    click(&mut ui, Offset::new(4.0, style().line_height / 2.0));
    click(&mut ui, Offset::new(4.0, style().line_height * 1.5));
    assert_eq!(*pressed.borrow(), ["long", "long"]);
    click(&mut ui, Offset::new(width - 1.0, style().line_height * 1.5));
    assert_eq!(
        pressed.borrow().len(),
        2,
        "past the end of the second line is not the link"
    );
}

/// What inline content was told and where it was placed.
#[derive(Default)]
struct Shown {
    alternate: RefCell<Vec<String>>,
    rect: Cell<Option<Rect>>,
}

/// "ab " then inline content standing for "code" in a placeholder 30 by 10, centered on
/// the line, then " cd".
fn with_content(shown: &Rc<Shown>) -> (AnnotatedString, HashMap<String, InlineTextContent>) {
    let text = build_annotated_string(|builder| {
        builder.append("ab ");
        builder.append_inline_content("chip", "code");
        builder.append(" cd");
    });
    let shown = Rc::clone(shown);
    let content = InlineTextContent::new(
        Placeholder::new(
            TextUnit::Sp(30.0),
            TextUnit::Sp(10.0),
            PlaceholderVerticalAlign::Center,
        ),
        move |alternate| {
            shown.alternate.borrow_mut().push(alternate.to_owned());
            let placed = Rc::clone(&shown);
            stack_with(
                Modifier::new()
                    .fill_max_size()
                    .on_globally_positioned(move |rect| placed.rect.set(Some(rect))),
                || {},
            );
        },
    );
    (text, HashMap::from([("chip".to_owned(), content)]))
}

/// Inline content is composed with its alternate text, at its placeholder's size, where its
/// room is kept in the line, centered on it; the text after it moves past it.
#[test]
fn inline_content_is_set_in_its_room() {
    let shown = Rc::new(Shown::default());
    let (text, content) = with_content(&shown);
    let _ui = interface(move || {
        basic_text_annotated(Modifier::new(), &text, style(), &content);
    });
    assert_eq!(*shown.alternate.borrow(), ["code"]);
    let rect = shown.rect.get().expect("the content was placed");
    let before = rustle::layout_text("ab ", &style(), None).size().width;
    assert!(
        (rect.origin.x - before).abs() < 0.5,
        "{rect:?} after {before}"
    );
    assert!((rect.size.width - 30.0).abs() < 0.01, "{rect:?}");
    assert!((rect.size.height - 10.0).abs() < 0.01, "{rect:?}");
    let middle = rect.origin.y + rect.size.height / 2.0;
    assert!((middle - style().line_height / 2.0).abs() < 0.5, "{rect:?}");
}

/// A placeholder taller than the line makes the line taller.
#[test]
fn a_tall_placeholder_grows_its_line() {
    let height = Rc::new(Cell::new(0.0_f32));
    let told = Rc::clone(&height);
    let text = build_annotated_string(|builder| {
        builder.append("ab ");
        builder.append_inline_content("tall", "x");
    });
    let content = HashMap::from([(
        "tall".to_owned(),
        InlineTextContent::new(
            Placeholder::new(
                TextUnit::Em(1.0),
                TextUnit::Sp(50.0),
                PlaceholderVerticalAlign::Bottom,
            ),
            |_| {},
        ),
    )]);
    let _ui = interface(move || {
        let told = Rc::clone(&told);
        basic_text_annotated(
            Modifier::new().on_globally_positioned(move |rect| told.set(rect.size.height)),
            &text,
            style(),
            &content,
        );
    });
    assert!(height.get() >= 50.0 - 0.5, "{}", height.get());
}

/// Selecting across inline content copies the alternate text it stands for.
#[test]
fn copying_across_inline_content_copies_its_alternate_text() {
    let shown = Rc::new(Shown::default());
    let (text, content) = with_content(&shown);
    let state = Rc::new(RefCell::new(None::<SelectionState>));
    let kept = Rc::clone(&state);
    let mut ui = interface(move || {
        let selection = remember_selection_state();
        *kept.borrow_mut() = Some(selection.clone());
        let (text, content) = (text.clone(), content.clone());
        selection_container_with(Modifier::new(), &selection, move || {
            basic_text_annotated(Modifier::new(), &text, style(), &content);
        });
    });
    let selection = state.borrow().clone().expect("the container was composed");
    selection.select_all();
    ui.update();
    assert_eq!(selection.selected_text().as_deref(), Some("ab code cd"));
}

/// Each range with a paragraph style is a paragraph of its own under the one before, with
/// its own line height, and copying across them puts a line break between them.
#[test]
fn paragraphs_take_their_own_line_heights() {
    let height = Rc::new(Cell::new(0.0_f32));
    let told = Rc::clone(&height);
    let text = build_annotated_string(|builder| {
        builder.append("first");
        builder.with_style(
            ParagraphStyle::new().line_height(TextUnit::Sp(40.0)),
            |builder| builder.append("second"),
        );
    });
    let state = Rc::new(RefCell::new(None::<SelectionState>));
    let kept = Rc::clone(&state);
    let mut ui = interface(move || {
        let selection = remember_selection_state();
        *kept.borrow_mut() = Some(selection.clone());
        let (text, told) = (text.clone(), Rc::clone(&told));
        selection_container_with(Modifier::new(), &selection, move || {
            basic_text_annotated(
                Modifier::new().on_globally_positioned(move |rect| told.set(rect.size.height)),
                &text,
                style(),
                &HashMap::new(),
            );
        });
    });
    assert!(
        (height.get() - (style().line_height + 40.0)).abs() < 0.5,
        "{}",
        height.get()
    );
    let selection = state.borrow().clone().expect("the container was composed");
    selection.select_all();
    ui.update();
    assert_eq!(selection.selected_text().as_deref(), Some("first\nsecond"));
}
