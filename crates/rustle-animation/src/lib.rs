//! Animation: values that move toward a target over time.
//!
//! [`animate_as_state`] returns state that follows a target value. Whenever the
//! target changes, the state moves to it frame by frame, driven by the frame
//! clock, so frames are only requested while something is moving.

mod animatable;
mod easing;
mod infinite;
mod lerp;
mod spec;
mod state;

pub use animatable::*;
pub use easing::*;
pub use infinite::*;
pub use lerp::*;
pub use spec::*;
pub use state::*;
