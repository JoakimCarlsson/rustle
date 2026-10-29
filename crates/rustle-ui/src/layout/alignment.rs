use rustle_geometry::{Offset, Size};
use rustle_unit::Constraints;

use super::{Children, Layout, LayoutResult};

/// How a picture fills the area it is given when the shapes differ.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ContentScale {
    /// All of the picture, as large as fits, centered.
    #[default]
    Fit,
    /// The picture fills the area, centered, and what overflows is cut off.
    Crop,
    /// The picture is stretched to the area, distorting it.
    Stretch,
}

/// Where a child sits within a stack.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum BoxAlignment {
    #[default]
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl BoxAlignment {
    /// How far along each axis the child sits, from 0 (start) to 1 (end).
    fn factors(self) -> (f32, f32) {
        match self {
            Self::TopStart => (0.0, 0.0),
            Self::TopCenter => (0.5, 0.0),
            Self::TopEnd => (1.0, 0.0),
            Self::CenterStart => (0.0, 0.5),
            Self::Center => (0.5, 0.5),
            Self::CenterEnd => (1.0, 0.5),
            Self::BottomStart => (0.0, 1.0),
            Self::BottomCenter => (0.5, 1.0),
            Self::BottomEnd => (1.0, 1.0),
        }
    }
}

/// Places every child on top of each other, each at its own alignment or at `alignment`.
///
/// Also the layout of a root that emits several elements.
#[derive(Default)]
pub struct Overlay {
    pub alignment: BoxAlignment,
    /// Mirrors the horizontal alignments.
    pub rtl: bool,
}

impl Layout for Overlay {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            ..constraints
        };
        let mut sizes = Vec::with_capacity(children.len());
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, child_constraints);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
            sizes.push(child);
        }
        // Align against the final size, which may be larger than the content.
        let size = constraints.constrain(size);
        let positions = sizes
            .iter()
            .enumerate()
            .map(|(index, child)| {
                let (mut x, y) = children
                    .alignment(index)
                    .unwrap_or(self.alignment)
                    .factors();
                if self.rtl {
                    x = 1.0 - x;
                }
                Offset::new(
                    (size.width - child.width) * x,
                    (size.height - child.height) * y,
                )
            })
            .collect();
        LayoutResult { size, positions }
    }
}
