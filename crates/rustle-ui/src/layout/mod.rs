//! Layout: turns a node tree into drawing commands in two steps.
//!
//! Measuring passes [`Constraints`] down and sizes up, applying each node's
//! modifiers and its [`Layout`] policy. Painting then walks the measured tree
//! and emits [`DrawCommand`]s, clickable regions and scrolling regions.

mod alignment;
mod children;
mod custom;
mod intrinsic;
mod measure;
mod measured;
mod paint;
pub(crate) mod regions;

#[cfg(test)]
mod layout_tests;

use rustle_geometry::{Offset, Size};
use rustle_unit::Constraints;

pub use alignment::{BoxAlignment, ContentScale, Overlay};
pub use children::Children;
pub use custom::{
    IntrinsicSize, Measurable, Placement, SubcomposeResult, SubcomposeScope, subcompose_layout,
};
pub use paint::{DisplayOptions, LaidOut, PixelRounding, display_list, display_list_with};
pub use regions::{
    ButtonRegion, CursorRegion, DisplayList, DragRegion, DragSourceRegion, DropTargetRegion,
    FileDropRegion, FocusRegion, HitRegion, HoverRegion, ImeCaretRegion, InteractionRegion,
    KeyRegion, PointerInputRegion, PopupRegion, PressRegion, ScrollRegion, SemanticsNode,
    TaggedNode,
};

/// Decides how a node sizes and places its children.
pub trait Layout {
    /// Measures the children through `children` and returns the node's size
    /// and where each child goes, relative to the node's top-left corner.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult;

    /// The smallest width at which the node's content is not cut off, when it
    /// is `height` tall. The default is the widest child's.
    fn min_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        (0..children.len()).fold(0.0, |widest, index| {
            widest.max(children.min_intrinsic_width(index, height))
        })
    }

    /// The width the node would take with all the room it wanted, when it is
    /// `height` tall. The default is the widest child's.
    fn max_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        (0..children.len()).fold(0.0, |widest, index| {
            widest.max(children.max_intrinsic_width(index, height))
        })
    }

    /// The smallest height at which the node's content is not cut off, when it
    /// is `width` wide. The default is the tallest child's.
    fn min_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        (0..children.len()).fold(0.0, |tallest, index| {
            tallest.max(children.min_intrinsic_height(index, width))
        })
    }

    /// The height the node would take with all the room it wanted, when it is
    /// `width` wide. The default is the tallest child's.
    fn max_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        (0..children.len()).fold(0.0, |tallest, index| {
            tallest.max(children.max_intrinsic_height(index, width))
        })
    }
}

/// The outcome of a [`Layout`]: the node's size and one position per child.
pub struct LayoutResult {
    pub size: Size,
    pub positions: Vec<Offset>,
}
