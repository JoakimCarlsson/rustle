//! A clipboard a program installs before its interface opens is the one the interface uses.

use std::cell::RefCell;
use std::rc::Rc;

use rustle::{Clipboard, Embedded};

/// A clipboard kept in memory.
#[derive(Default)]
struct Kept(RefCell<Option<String>>);

impl Clipboard for Kept {
    /// The text kept.
    fn text(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    /// Keeps `text`.
    fn set_text(&self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_owned());
    }
}

/// Opening an interface keeps a clipboard installed before it, rather than putting the
/// system's in its place.
#[test]
fn an_installed_clipboard_outlives_opening_an_interface() {
    let kept = Rc::new(Kept::default());
    rustle::set_clipboard(Rc::clone(&kept) as Rc<dyn Clipboard>);
    let _ui = Embedded::without_renderer(|| {}, || {});
    let _other = Embedded::without_renderer(|| {}, || {});
    rustle::clipboard()
        .expect("a clipboard is installed")
        .set_text("kept");
    assert_eq!(kept.text().as_deref(), Some("kept"));
}
