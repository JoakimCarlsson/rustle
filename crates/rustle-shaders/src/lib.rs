//! The WGSL the renderer draws with.
//!
//! Each shader is one source string, kept here so the pipelines that consume
//! them and the shaders themselves are written and reviewed apart.

/// Rounded, bordered, optionally blurred rectangles, one instanced quad each.
pub const QUAD: &str = include_str!("quad.wgsl");

/// Glyphs, one instanced quad each on whole physical pixels, sampled texel for texel from a
/// coverage atlas and tinted, or from a color atlas as they are.
pub const GLYPH: &str = include_str!("glyph.wgsl");

/// Pictures, one instanced quad each, sampled from a full-colour texture, and coverage masks
/// filled with a tint.
pub const IMAGE: &str = include_str!("image.wgsl");

/// A rendered layer composited onto its parent, turned, scaled and faded.
pub const COMPOSITE: &str = include_str!("composite.wgsl");

/// Filled vector outlines, drawn as quads sampling a coverage mask.
pub const PATH: &str = include_str!("path.wgsl");

/// A Gaussian blur of a texture along one direction.
pub const BLUR: &str = include_str!("blur.wgsl");

/// The frame a program's fragment shader is drawn in: bindings, a vertex shader and a
/// fragment entry point that calls `shade`. [`custom`] appends the program's source.
const SHADER: &str = include_str!("shader.wgsl");

/// The complete WGSL module for a program's fragment shader `source`, which must define
/// `fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32>`.
pub fn custom(source: &str) -> String {
    let mut module = String::with_capacity(SHADER.len() + source.len());
    module.push_str(SHADER);
    module.push_str(source);
    module
}
