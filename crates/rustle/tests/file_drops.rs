//! Hands files a host's own drag and drop let go of to the file drop area of an [`Embedded`]
//! interface.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{DragAndDropEvent, DragAndDropTarget, DragData, Embedded, Offset};

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

/// What a drop target heard of files dragged in from another application, as words.
struct Heard(Rc<RefCell<Vec<String>>>);

impl DragAndDropTarget for Heard {
    /// Notes the files let go, and takes them.
    fn on_drop(&self, event: &DragAndDropEvent) -> bool {
        let Some(DragData::FilesList(files)) = event.drag_data() else {
            return false;
        };
        self.0
            .borrow_mut()
            .push(format!("drop {} at {}", files.len(), event.position.x));
        true
    }

    /// Notes that the drag started.
    fn on_started(&self, _event: &DragAndDropEvent) {
        self.0.borrow_mut().push("started".to_owned());
    }

    /// Notes that the pointer came over the target.
    fn on_entered(&self, _event: &DragAndDropEvent) {
        self.0.borrow_mut().push("entered".to_owned());
    }

    /// Notes where the pointer moved over the target.
    fn on_moved(&self, event: &DragAndDropEvent) {
        self.0
            .borrow_mut()
            .push(format!("moved {}", event.position.x));
    }

    /// Notes that the pointer left the target.
    fn on_exited(&self, _event: &DragAndDropEvent) {
        self.0.borrow_mut().push("exited".to_owned());
    }

    /// Notes that the drag ended.
    fn on_ended(&self, _event: &DragAndDropEvent) {
        self.0.borrow_mut().push("ended".to_owned());
    }
}

/// A root with one 100 by 40 drop target at (10, 10) that accepts files from other
/// applications and notes what it hears.
fn target_root(heard: Rc<RefCell<Vec<String>>>) -> impl FnMut() {
    move || {
        let target = Heard(Rc::clone(&heard));
        let modifier = Modifier::new()
            .padding(10.0)
            .size(100.0, 40.0)
            .drag_and_drop_target(
                |event| matches!(event.drag_data(), Some(DragData::FilesList(_))),
                target,
            );
        canvas(modifier, |_| {});
    }
}

/// Files dragged in from another application are followed by the drop target as a drag
/// inside the window is: it hears the drag start, the pointer enter, move and leave, and gets
/// every file in one drop at the point they were let go.
#[test]
fn drop_targets_follow_files_from_other_applications() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut ui = Embedded::without_renderer(target_root(Rc::clone(&heard)), || {});
    ui.resize((200, 100), 1.0);
    ui.update();
    assert!(!ui.files_dragged_over(Offset::new(150.0, 80.0)));
    assert!(ui.files_dragged_over(Offset::new(40.0, 30.0)));
    assert!(ui.files_dragged_over(Offset::new(60.0, 30.0)));
    assert!(!ui.files_dragged_over(Offset::new(150.0, 30.0)));
    assert!(ui.takes_files_at(Offset::new(50.0, 30.0)));
    assert!(ui.files_dropped(
        Offset::new(50.0, 30.0),
        vec![PathBuf::from("/b"), PathBuf::from("/c")]
    ));
    assert_eq!(
        *heard.borrow(),
        [
            "started",
            "entered",
            "moved 40",
            "moved 60",
            "exited",
            "entered",
            "moved 50",
            "drop 2 at 50",
            "ended",
        ]
    );
}

/// A drag of files that leaves without a drop tells the target it was over that the pointer
/// left and the drag ended.
#[test]
fn files_that_leave_end_the_drag() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut ui = Embedded::without_renderer(target_root(Rc::clone(&heard)), || {});
    ui.resize((200, 100), 1.0);
    ui.update();
    assert!(ui.files_dragged_over(Offset::new(40.0, 30.0)));
    ui.files_drag_left();
    assert_eq!(
        *heard.borrow(),
        ["started", "entered", "moved 40", "exited", "ended"]
    );
}
