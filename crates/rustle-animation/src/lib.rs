//! Animation: values that move toward a target over time.
//!
//! [`animate_as_state`] returns state that follows a target value. Whenever the
//! target changes, the state moves to it frame by frame, driven by the frame
//! clock, so frames are only requested while something is moving.
//!
//! [`update_transition`] animates several values off one target state,
//! [`animate_decay`] coasts a flung value to rest, and `Path`s morph through [`Lerp`].

mod animatable;
mod decay;
mod easing;
mod infinite;
mod lerp;
mod spec;
mod state;
mod transition;

pub use animatable::*;
pub use decay::*;
pub use easing::*;
pub use infinite::*;
pub use lerp::*;
pub use spec::*;
pub use state::*;
pub use transition::*;

#[cfg(test)]
mod tests;
