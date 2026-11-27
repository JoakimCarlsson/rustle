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

use glyphon::{Cache, FontSystem, SwashCache, TextAtlas, TextRenderer, Viewport};

mod block;
mod blur;
mod composite;
mod device;
mod engine;
mod frame;
mod image;
mod instances;
mod layers;
mod paragraphs;
mod path;
mod quad;
mod shaders;
#[cfg(test)]
mod tests;
mod text;
mod uniforms;

pub use text::set_legible_text;

use blur::Blur;
use composite::Composite;
use image::Images;
use path::Paths;
use quad::Quads;
use shaders::Shaders;

pub use text::TextMetrics;

type SurfaceFactory = Box<
    dyn Fn(
        &wgpu::Instance,
    ) -> std::result::Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError>,
>;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// How many frames a window surface made from now on may queue; see [`set_frame_latency`].
static FRAME_LATENCY: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2);

/// Sets how many frames window surfaces configured from now on may hold queued for display,
/// at least one. Two, the default, favors throughput; one shortens the time from drawing a
/// frame to showing it on hardware that keeps up. Set it before the first renderer is made.
///
/// Two stays the default on macOS too, where frames are paced by a display link: animating a
/// few thousand nodes at 120 Hz on Apple silicon missed about 1.1% of refreshes with one frame
/// queued against 0.1 to 0.8% with two, so one would trade dropped frames for latency.
pub fn set_frame_latency(frames: u32) {
    FRAME_LATENCY.store(frames.max(1), std::sync::atomic::Ordering::Relaxed);
}

/// How many frames a window surface configured now may queue.
pub(crate) fn frame_latency() -> u32 {
    FRAME_LATENCY.load(std::sync::atomic::Ordering::Relaxed)
}

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
    /// Shaped text kept between frames, keyed by a hash of what decides its shape.
    shaped: HashMap<u64, text::Kept>,
    /// Sizes text measured for layout, keyed by a hash of what was measured,
    /// with the frame they were last used in.
    measured: HashMap<u64, (rustle_geometry::Size, u64)>,
    frame: u64,
    swash_cache: SwashCache,
    text_cache: Cache,
    /// What glyphon positions text against, one per size of texture drawn into.
    text_viewports: HashMap<(u32, u32), Viewport>,
    atlas: TextAtlas,
    /// One per layer that has text, grown as needed and reused across frames.
    text_renderers: Vec<TextRenderer>,
    quads: Quads,
    images: Images,
    paths: Paths,
    composite: Composite,
    blur: Blur,
    /// Programs' fragment shaders, compiled once per source.
    shaders: Shaders,
    layer_pool: std::cell::RefCell<frame::LayerPool>,
    /// The background as the surface takes it: linear on an sRGB surface.
    clear: wgpu::Color,

    // Declared before `create_surface`, which may own the window the surface
    // draws into, so the surface is dropped first.
    surface: Option<wgpu::Surface<'static>>,
    create_surface: Option<SurfaceFactory>,
}
