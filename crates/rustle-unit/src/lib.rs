//! Units and layout constraints.

use rustle_geometry::Size;

/// A length in density-independent pixels, which the renderer scales to the display.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd)]
pub struct Dp(pub f32);

/// Shorthand for [`Dp`].
pub const fn dp(value: f32) -> Dp {
    Dp(value)
}

impl From<f32> for Dp {
    fn from(value: f32) -> Self {
        Dp(value)
    }
}

/// The range of sizes a layout may choose, passed from parent to child.
///
/// A bound of `f32::INFINITY` means unbounded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Constraints {
    /// Any size up to `max`.
    pub const fn loose(max: Size) -> Self {
        Self {
            min_width: 0.0,
            max_width: max.width,
            min_height: 0.0,
            max_height: max.height,
        }
    }

    /// Clamps `size` into the constraints.
    pub fn constrain(&self, size: Size) -> Size {
        Size::new(
            size.width.clamp(self.min_width, self.max_width),
            size.height.clamp(self.min_height, self.max_height),
        )
    }

    /// Shrinks every bound by the space taken by padding, never below zero.
    pub fn deflate(&self, horizontal: f32, vertical: f32) -> Self {
        Self {
            min_width: (self.min_width - horizontal).max(0.0),
            max_width: (self.max_width - horizontal).max(0.0),
            min_height: (self.min_height - vertical).max(0.0),
            max_height: (self.max_height - vertical).max(0.0),
        }
    }
}
