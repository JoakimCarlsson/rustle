//! Pinning what an area belongs to so it stays composed: Compose's `PinnableContainer`,
//! `PinnedHandle` and `LocalPinnableContainer`. A lazy list provides a container for each
//! item it composes, and a focus target pins the item it is in while it holds focus.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_runtime::{CompositionLocal, is_composing};

/// Something, such as an item of a lazy list, that can be pinned so it stays composed while
/// it would otherwise leave the composition: Compose's `PinnableContainer`.
#[derive(Clone)]
pub struct PinnableContainer {
    /// Tells containers apart: two containers with the same key are the same one.
    key: u64,
    /// Pins what the container holds.
    pin: Rc<dyn Fn() -> PinnedHandle>,
}

impl PinnableContainer {
    /// A container that `pin` pins, told apart from others by `key`, which the maker keeps
    /// the same for as long as the container stands for the same thing.
    pub fn new(key: u64, pin: impl Fn() -> PinnedHandle + 'static) -> Self {
        Self {
            key,
            pin: Rc::new(pin),
        }
    }

    /// Pins what the container holds until every handle pinning it is released: Compose's
    /// `pin()`.
    pub fn pin(&self) -> PinnedHandle {
        (self.pin)()
    }
}

impl PartialEq for PinnableContainer {
    /// Whether both stand for the same thing, by their keys.
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl std::fmt::Debug for PinnableContainer {
    /// The key of the container.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PinnableContainer")
            .field("key", &self.key)
            .finish()
    }
}

/// A pin from [`PinnableContainer::pin`]: Compose's `PinnedHandle`. Released by
/// [`release`](Self::release) or when dropped.
pub struct PinnedHandle {
    /// What releasing the pin does, until it is released.
    release: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl PinnedHandle {
    /// A pin that `release` lets go of, once.
    pub fn new(release: impl FnOnce() + 'static) -> Self {
        Self {
            release: RefCell::new(Some(Box::new(release))),
        }
    }

    /// Releases the pin; releasing it twice does nothing more. Compose's `release()`.
    pub fn release(&self) {
        let release = self.release.borrow_mut().take();
        if let Some(release) = release {
            release();
        }
    }
}

impl Drop for PinnedHandle {
    /// Releases the pin if it was not released.
    fn drop(&mut self) {
        self.release();
    }
}

impl std::fmt::Debug for PinnedHandle {
    /// Whether the pin is still held.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PinnedHandle")
            .field("held", &self.release.borrow().is_some())
            .finish()
    }
}

/// The [`PinnableContainer`] of what is being composed, such as the item of a lazy list, or
/// `None` outside one: Compose's `LocalPinnableContainer`.
pub static LOCAL_PINNABLE_CONTAINER: CompositionLocal<Option<PinnableContainer>> =
    CompositionLocal::new(|| None);

/// The container of what is being composed, when composing, for a focus target to pin.
pub(crate) fn current_pinnable_container() -> Option<PinnableContainer> {
    if is_composing() {
        LOCAL_PINNABLE_CONTAINER.current()
    } else {
        None
    }
}
