//! Rustle's runtime: observable state, groups, `remember`, effects, composition
//! locals, and full-root recomposition.
//!
//! Knows nothing about UI trees or rendering.

mod collections;
mod composition;
mod effects;
mod frame;
mod local;
mod state;
mod task;

pub use collections::{StateList, StateMap, remember_state_list, remember_state_map};
pub use composition::{Composition, frame_clock, group, key, memo, remember};
pub use effects::{
    Saver, derived_state_of, disposable_effect, launched_effect, produce_state, remember_saveable,
    remember_updated_state, set_saver, side_effect,
};
pub use frame::FrameClock;
pub use local::CompositionLocal;
pub use state::State;
pub use task::{Delay, NextFrame, TaskScope};
