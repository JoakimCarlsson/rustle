//! Dropping what a frame replaced after the frame is shown, not while it is made.
//!
//! Laying out again replaces the measured tree, and painting again the display list; freeing
//! the old ones walks every node and returns every allocation, which is a good part of laying
//! out a window that changed size. They are retired here instead, and a host drops them once
//! it has handed the frame to the GPU, or before the next frame, so the time an input takes to
//! reach the screen does not include it.

use std::any::Any;
use std::cell::RefCell;

/// How many retired values are held at most; retiring more drops the oldest at once, so a host
/// that never drops them, such as a test laying out in a loop, holds only a few frames' worth.
const MOST_RETIRED: usize = 8;

thread_local! {
    /// What was retired on this thread and not dropped yet, oldest first.
    static RETIRED: RefCell<Vec<Box<dyn Any>>> = const { RefCell::new(Vec::new()) };
}

/// Keeps `value`, something a frame replaced, until [`drop_retired`] drops it.
pub fn retire(value: impl Any) {
    let oldest = RETIRED.with(|retired| {
        let mut retired = retired.borrow_mut();
        retired.push(Box::new(value));
        (retired.len() > MOST_RETIRED).then(|| retired.remove(0))
    });
    drop(oldest);
}

/// Drops everything retired on this thread, outside any borrow of the bin, so a value whose
/// drop retires another is not lost.
pub fn drop_retired() {
    let taken = RETIRED.with(|retired| std::mem::take(&mut *retired.borrow_mut()));
    drop(taken);
}
