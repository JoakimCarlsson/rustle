//! Lays out text and rich text with `on_text_layout` in an [`Embedded`] interface and checks
//! that each is told its shaped text, which hit tests to byte offsets.

use std::cell::RefCell;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{Embedded, Offset, TextLayout, TextSpan};

/// The layouts each text was told.
#[derive(Default)]
struct Told {
    plain: RefCell<Option<Rc<TextLayout>>>,
    rich: RefCell<Option<Rc<TextLayout>>>,
}

/// A root with a plain text and a rich text, each recording the layout it is told.
fn root(told: Rc<Told>) -> impl FnMut() {
    move || {
        let plain = Rc::clone(&told);
        let rich = Rc::clone(&told);
        column(move || {
            let plain = Rc::clone(&plain);
            let rich = Rc::clone(&rich);
            text_with(
                Modifier::new()
                    .on_text_layout(move |layout| *plain.plain.borrow_mut() = Some(layout)),
                "hello world",
            );
            rich_text_with(
                Modifier::new()
                    .on_text_layout(move |layout| *rich.rich.borrow_mut() = Some(layout)),
                vec![TextSpan::new("left "), TextSpan::new("right")],
            );
        });
    }
}

/// Both texts are told their layout, which holds their text and turns points before and
/// past the line into the byte offsets of its ends.
#[test]
fn text_is_told_its_layout() {
    let told = Rc::new(Told::default());
    let mut embedded = Embedded::without_renderer(root(Rc::clone(&told)), || {});
    embedded.resize((400, 200), 1.0);
    embedded.update();
    let plain = told.plain.borrow().clone().expect("plain text was told");
    assert_eq!(plain.text(), "hello world");
    assert_eq!(
        plain.hit_test(Offset::new(1000.0, 1.0)),
        "hello world".len()
    );
    assert_eq!(plain.hit_test(Offset::new(-5.0, 1.0)), 0);
    let rich = told.rich.borrow().clone().expect("rich text was told");
    assert_eq!(rich.text(), "left right");
    assert_eq!(rich.line_count(), 1);
}
