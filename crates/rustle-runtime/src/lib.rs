//! Rustle's runtime: observable state, groups, `remember`, effects, composition
//! locals, snapshot state, and recomposition of the root or, for state read inside
//! [`restartable`] blocks, of those blocks alone, with skipping of unchanged blocks.
//!
//! Knows nothing about UI trees or rendering.

mod collections;
mod composition;
mod effects;
mod frame;
mod local;
mod scope;
mod snapshot;
mod state;
mod task;

#[cfg(test)]
mod recomposition_tests;

pub use collections::{StateList, StateMap, remember_state_list, remember_state_map};
pub use composition::{
    Composition, Subcomposition, frame_clock, group, key, memo, mutable_state_of, remember,
    remember_live, restartable,
};
pub use effects::{
    Saver, derived_state_of, disposable_effect, launched_effect, produce_state, remember_saveable,
    remember_updated_state, set_saver, side_effect,
};
pub use frame::FrameClock;
pub use local::CompositionLocal;
pub use scope::{Reads, Scope, mark_unskippable, track_reads};
pub use snapshot::{
    AppliedChanges, ApplyResult, MutableSnapshot, ObserverHandle, Snapshot, SnapshotFlow,
    snapshot_flow,
};
pub use state::State;
pub use task::{Delay, NextFrame, TaskScope};
