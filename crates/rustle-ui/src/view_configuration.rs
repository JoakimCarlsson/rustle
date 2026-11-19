//! How gestures are recognized: Compose's `ViewConfiguration` and `LocalViewConfiguration`.

use rustle_runtime::{CompositionLocal, is_composing};

/// The distances and times gestures are recognized by: Compose's `ViewConfiguration`. Read
/// it through [`LOCAL_VIEW_CONFIGURATION`]; every gesture takes the one in effect where its
/// modifier is made, and presses where no area counts them take the default.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewConfiguration {
    /// How long the primary button is held on an area before it counts as a long press, in
    /// milliseconds: Compose's `longPressTimeoutMillis`.
    pub long_press_timeout_millis: u64,
    /// How long after a press another may come and count as the next in a double or triple
    /// click, in milliseconds: Compose's `doubleTapTimeoutMillis`. The second press must also
    /// land within `touch_slop` of the first.
    pub double_tap_timeout_millis: u64,
    /// How far, in logical pixels, the pointer moves with the primary button held before the
    /// movement counts as a drag, scroll, drag and drop or pick-up, and no longer as a long
    /// press: Compose's `touchSlop`.
    pub touch_slop: f32,
}

impl Default for ViewConfiguration {
    /// Compose's defaults: half a second to a long press, as on the desktop, 300 milliseconds
    /// between the presses of a double click, and 8 logical pixels of slop, Compose's touch
    /// slop on Android, since the desktop's 18 is meant for touch screens.
    fn default() -> Self {
        Self {
            long_press_timeout_millis: 500,
            double_tap_timeout_millis: 300,
            touch_slop: 8.0,
        }
    }
}

/// The [`ViewConfiguration`] the gestures below it are recognized by: Compose's
/// `LocalViewConfiguration`. Provide another to change them for part of the tree.
pub static LOCAL_VIEW_CONFIGURATION: CompositionLocal<ViewConfiguration> =
    CompositionLocal::new(ViewConfiguration::default);

/// The view configuration in effect where a modifier is made: the one provided around it
/// while composing, and the default outside composition.
pub(crate) fn current_view_configuration() -> ViewConfiguration {
    if is_composing() {
        LOCAL_VIEW_CONFIGURATION.current()
    } else {
        ViewConfiguration::default()
    }
}
