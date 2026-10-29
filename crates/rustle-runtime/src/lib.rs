//! Rustle's runtime: observable state, `remember`, and full-root recomposition.
//!
//! Knows nothing about UI trees or rendering.

mod composition;
mod state;

pub use composition::{Composition, remember};
pub use state::State;
