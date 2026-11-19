//! Hands files a host's own drag and drop let go of to the file drop area of an [`Embedded`]
//! interface.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{Embedded, Offset};

/// A root with one 100 by 40 file drop area at (10, 10) that records what it is given.
fn root(dropped: Rc<RefCell<Vec<PathBuf>>>) -> impl FnMut() {
    move || {
        let dropped = Rc::clone(&dropped);
        let modifier = Modifier::new()
            .padding(10.0)
            .size(100.0, 40.0)
            .on_file_drop(move |path| dropped.borrow_mut().push(path));
        canvas(modifier, |_| {});
    }
}

/// Files let go of over the area reach it; files let go of beside it are refused.
#[test]
fn files_reach_the_drop_area_under_them() {
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let mut ui = Embedded::without_renderer(root(Rc::clone(&dropped)), || {});
    ui.resize((200, 100), 1.0);
    ui.update();
    assert!(ui.takes_files_at(Offset::new(50.0, 30.0)));
    assert!(!ui.takes_files_at(Offset::new(150.0, 80.0)));
    assert!(!ui.files_dropped(Offset::new(150.0, 80.0), vec![PathBuf::from("/a")]));
    assert!(ui.files_dropped(
        Offset::new(50.0, 30.0),
        vec![PathBuf::from("/b"), PathBuf::from("/c")]
    ));
    assert_eq!(
        *dropped.borrow(),
        vec![PathBuf::from("/b"), PathBuf::from("/c")]
    );
}
