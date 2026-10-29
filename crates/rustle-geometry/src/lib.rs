//! Geometry primitives: offsets, sizes and rectangles, in logical pixels.

/// A position or displacement.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// A width and a height.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A rectangle given by its top-left corner and its size.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rect {
    pub origin: Offset,
    pub size: Size,
}

impl Rect {
    pub const fn new(origin: Offset, size: Size) -> Self {
        Self { origin, size }
    }
}
