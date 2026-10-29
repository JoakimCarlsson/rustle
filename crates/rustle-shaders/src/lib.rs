//! The WGSL the renderer draws with.
//!
//! Each shader is one source string, kept here so the pipelines that consume
//! them and the shaders themselves are written and reviewed apart.

/// Rounded, bordered, optionally blurred rectangles, one instanced quad each.
pub const QUAD: &str = include_str!("quad.wgsl");

/// Glyphs, one instanced quad each, sampled from a coverage atlas.
pub const GLYPH: &str = include_str!("glyph.wgsl");

/// Pictures, one instanced quad each, sampled from a full-colour texture.
pub const IMAGE: &str = include_str!("image.wgsl");

/// A rendered layer composited onto its parent, turned, scaled and faded.
pub const COMPOSITE: &str = include_str!("composite.wgsl");

/// Filled vector outlines, drawn as triangles.
pub const PATH: &str = include_str!("path.wgsl");
