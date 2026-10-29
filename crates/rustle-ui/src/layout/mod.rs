//! Layout: turns a node tree into drawing commands in two steps.
//!
//! Measuring passes [`Constraints`] down and sizes up, applying each node's
//! modifiers and its [`Layout`] policy. Painting then walks the measured tree
//! and emits [`DrawCommand`]s, clickable regions and scrolling regions.

mod alignment;
mod children;
mod measure;
mod measured;
mod paint;
mod regions;

use rustle_geometry::{Offset, Size};
use rustle_unit::Constraints;

pub use alignment::{BoxAlignment, ContentScale, Overlay};
pub use children::Children;
pub use paint::display_list;
pub use regions::{
    DisplayList, DragRegion, DragSourceRegion, DropTargetRegion, FileDropRegion, FocusRegion,
    HitRegion, HoverRegion, PressRegion, ScrollRegion, SemanticsNode,
};

/// Decides how a node sizes and places its children.
pub trait Layout {
    /// Measures the children through `children` and returns the node's size
    /// and where each child goes, relative to the node's top-left corner.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult;
}

/// The outcome of a [`Layout`]: the node's size and one position per child.
pub struct LayoutResult {
    pub size: Size,
    pub positions: Vec<Offset>,
}
