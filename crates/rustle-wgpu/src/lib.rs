//! wgpu rendering backend for Rustle.
//!
//! The renderer only consumes [`DrawCommand`]s; it knows nothing about
//! composition or the UI tree. It also measures text for layout.
//!
//! Commands are drawn in order. Consecutive rectangles are drawn together,
//! then the text that follows them, so later commands paint over earlier ones.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use glyphon::{Buffer, FontSystem, SwashCache, TextAtlas, TextRenderer, Viewport};

mod composite;
mod device;
mod engine;
mod frame;
mod image;
mod layers;
mod path;
mod quad;
mod text;

use composite::Composite;
use image::Images;
use path::Paths;
use quad::Quads;

pub use text::TextMetrics;

type SurfaceFactory = Box<
    dyn Fn(
        &wgpu::Instance,
    ) -> std::result::Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError>,
>;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Decodes an sRGB channel in 0..=1 to linear light.
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub struct Renderer {
    /// Recreates a lost surface; a renderer given its device by a host has none.
    instance: Option<wgpu::Instance>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    surface_config: wgpu::SurfaceConfiguration,

    /// Shared with the text engine installed in `rustle-text`, which registers fonts and lays text out.
    font_system: Rc<RefCell<FontSystem>>,
    /// Counts fonts registered at run time, so cached text is shaped again.
    font_generation: Rc<Cell<u64>>,
    /// Shaped text kept between frames, with the frame it was last used in.
    shaped: HashMap<String, (Rc<Buffer>, u64)>,
    /// Sizes text measured for layout, keyed by a hash of what was measured,
    /// with the frame they were last used in.
    measured: HashMap<u64, (rustle_geometry::Size, u64)>,
    frame: u64,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    /// One per layer that has text, grown as needed and reused across frames.
    text_renderers: Vec<TextRenderer>,
    quads: Quads,
    images: Images,
    paths: Paths,
    composite: Composite,
    layer_pool: std::cell::RefCell<frame::LayerPool>,
    /// The background as the surface takes it: linear on an sRGB surface.
    clear: wgpu::Color,

    // Declared before `create_surface`, which may own the window the surface
    // draws into, so the surface is dropped first.
    surface: Option<wgpu::Surface<'static>>,
    create_surface: Option<SurfaceFactory>,
}
