//! Composition locals: values passed down the composition without parameters.

use std::rc::Rc;

use crate::composition::with_runtime;
use crate::state::record_check;

/// A value that composables further down can read without it being passed to them.
///
/// Declare one as a `static` with a default, provide a different value for a
/// part of the tree with [`CompositionLocal::provide`], and read the nearest
/// provided value with [`CompositionLocal::current`]:
///
/// ```ignore
/// static LOCAL_PADDING: CompositionLocal<f32> = CompositionLocal::new(|| 8.0);
/// ```
pub struct CompositionLocal<T> {
    default: fn() -> T,
}

impl<T: Clone + PartialEq + 'static> CompositionLocal<T> {
    pub const fn new(default: fn() -> T) -> Self {
        Self { default }
    }

    /// The value from the nearest enclosing [`CompositionLocal::provide`], or the default.
    pub fn current(&'static self) -> T {
        let value = self.current_untracked();
        // A skippable block that read this must not be skipped once it changed.
        let observed = value.clone();
        record_check(Rc::new(move || self.current_untracked() == observed));
        value
    }

    fn current_untracked(&'static self) -> T {
        let id = self.id();
        let provided = with_runtime(|runtime| {
            runtime
                .locals
                .iter()
                .rev()
                .find(|(local, _)| *local == id)
                .and_then(|(_, value)| value.downcast_ref::<T>().cloned())
        });
        provided.unwrap_or_else(self.default)
    }

    /// Runs `content` with `value` as the current value of this local.
    /// An inner `provide` overrides an outer one within its own `content`.
    pub fn provide<R>(&'static self, value: T, content: impl FnOnce() -> R) -> R {
        with_runtime(|runtime| runtime.locals.push((self.id(), Rc::new(value))));
        let _pop = Pop;
        content()
    }

    /// Each local is a distinct `static`, so its address identifies it.
    fn id(&'static self) -> usize {
        std::ptr::from_ref(self) as usize
    }
}

/// Removes the innermost provided value, also when `content` panics.
struct Pop;

impl Drop for Pop {
    fn drop(&mut self) {
        with_runtime(|runtime| {
            runtime.locals.pop();
        });
    }
}
