//! Steps an [`Embedded`] interface through frames on a clock the host sets, and checks that
//! task delays then depend on the frame times given, not on how long the test took.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use rustle::Embedded;
use rustle::prelude::*;

/// A root whose task waits `delay` after its first frame and then records that it finished.
fn delayed(done: Rc<Cell<bool>>, delay: Duration) -> impl FnMut() {
    move || {
        let done = Rc::clone(&done);
        launched_effect((), move |scope| async move {
            scope.delay(delay).await;
            done.set(true);
        });
    }
}

/// A headless interface of 200 by 200 logical pixels composing `root`.
fn rig(root: impl FnMut() + 'static) -> Embedded {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((200, 200), 1.0);
    embedded
}

/// A delay ends at the frame the host times past it, and not before.
#[test]
fn delays_follow_the_frames_the_host_times() {
    let done = Rc::new(Cell::new(false));
    let mut embedded = rig(delayed(Rc::clone(&done), Duration::from_millis(500)));
    embedded.advance_to(Duration::from_secs(10));
    assert_eq!(embedded.frame_time(), Duration::from_secs(10));
    embedded.advance_to(Duration::from_millis(10_499));
    assert!(!done.get(), "499 ms of frame time is not 500");
    assert!(embedded.wants_frame());
    embedded.advance_to(Duration::from_millis(10_500));
    assert!(
        done.get(),
        "the delay ended at the frame 500 ms after its first"
    );
}

/// Real time passing does not move a clock the host stopped.
#[test]
fn a_stopped_clock_never_ends_a_delay() {
    let done = Rc::new(Cell::new(false));
    let mut embedded = rig(delayed(Rc::clone(&done), Duration::from_millis(1)));
    embedded.set_clock(|| Duration::from_secs(3));
    for _ in 0..5 {
        embedded.update();
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !done.get(),
        "real time passing does not move a host's clock"
    );
    assert_eq!(embedded.frame_time(), Duration::from_secs(3));
}

/// `use_real_clock` drops the host's clock and times frames from now.
#[test]
fn the_real_clock_comes_back() {
    let mut embedded = rig(|| {});
    embedded.advance_to(Duration::from_secs(100));
    embedded.use_real_clock();
    assert!(embedded.frame_time() < Duration::from_secs(100));
}
