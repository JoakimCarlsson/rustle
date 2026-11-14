//! Creating the renderer: the device, the surface and what is built on them.

use glyphon::{Cache, FontSystem, SwashCache, TextAtlas};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::blur::Blur;
use crate::composite::Composite;
use crate::image::Images;
use crate::path::Paths;
use crate::quad::Quads;
use crate::shaders::Shaders;
use crate::{Renderer, Result, SurfaceFactory, srgb_to_linear};

/// The window background, as sRGB.
const BACKGROUND: [u8; 3] = [24, 24, 30];

/// The font system and the count of font registrations that renderers share.
type SharedFonts = (Rc<RefCell<FontSystem>>, Rc<Cell<u64>>);

thread_local! {
    /// What every renderer on this thread shares.
    static FONTS: std::cell::OnceCell<SharedFonts> = const { std::cell::OnceCell::new() };
}

/// The font system and the count of font registrations shared by all renderers of this thread,
/// made, and installed as the text engine, by the first.
///
/// The text functions of `rustle-text` reach one engine per thread. If each renderer had its
/// own fonts, the last one made would take the engine over, and a font registered afterwards
/// would exist for it alone. One font system also means the installed fonts are loaded once
/// however many interfaces draw on the thread.
pub(crate) fn shared_fonts() -> SharedFonts {
    FONTS.with(|fonts| {
        let (font_system, generation) = fonts.get_or_init(|| {
            let font_system = Rc::new(RefCell::new(FontSystem::new()));
            let generation = Rc::new(Cell::new(0));
            let engine = Rc::new(crate::engine::Engine {
                font_system: Rc::clone(&font_system),
                generation: Rc::clone(&generation),
            });
            rustle_text::install_text_engine(engine.clone());
            for data in rustle_text::take_pending_fonts() {
                rustle_text::TextEngine::register_font(&*engine, data);
            }
            (font_system, generation)
        });
        (Rc::clone(font_system), Rc::clone(generation))
    })
}

impl Renderer {
    /// Creates a renderer drawing into `target` (e.g. an `Arc<winit::window::Window>`).
    ///
    /// `size` is the target's size in physical pixels. `target` is cloned if the
    /// surface has to be recreated after being lost.
    pub fn new<T>(instance: wgpu::Instance, target: T, size: (u32, u32)) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>> + Clone + 'static,
    {
        Self::with_transparency(instance, target, size, false)
    }

    /// Like [`Renderer::new`], and when `transparent` the surface lets what is behind the
    /// window show through wherever the frame is not opaque, if the platform supports it.
    /// The window itself has to be created transparent too.
    pub fn with_transparency<T>(
        instance: wgpu::Instance,
        target: T,
        size: (u32, u32),
        transparent: bool,
    ) -> Result<Self>
    where
        T: Into<wgpu::SurfaceTarget<'static>> + Clone + 'static,
    {
        let create_surface: SurfaceFactory =
            Box::new(move |instance| instance.create_surface(target.clone()));
        let surface = create_surface(&instance)?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| caps.formats.first().copied())
            .ok_or("surface supports no texture formats")?;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            // Opaque unless the window is transparent, so a window the compositor blends never shows through a faded layer.
            alpha_mode: [
                wgpu::CompositeAlphaMode::PostMultiplied,
                wgpu::CompositeAlphaMode::PreMultiplied,
            ]
            .into_iter()
            .filter(|_| transparent)
            .chain([wgpu::CompositeAlphaMode::Opaque])
            .find(|mode| caps.alpha_modes.contains(mode))
            .unwrap_or_else(|| caps.alpha_modes.first().copied().unwrap_or_default()),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);
        Ok(Self::build(
            Some(instance),
            device,
            queue,
            format,
            surface_config,
            Some(surface),
            Some(create_surface),
        ))
    }

    /// Creates a renderer with no window, drawing into pictures on request
    /// through [`Renderer::snapshot`]. It needs a GPU but no display.
    pub fn offscreen(width: u32, height: u32) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        Ok(Self::build(
            Some(instance),
            device,
            queue,
            format,
            surface_config,
            None,
            None,
        ))
    }

    /// Creates a renderer that draws with a device and queue the caller already has, into
    /// textures the caller provides through [`Renderer::render_into`].
    ///
    /// For a host with its own renderer, such as a game engine, that composites the
    /// interface over its own frame. `format` is the format of every texture the renderer
    /// will draw into, and `size` their size in physical pixels. The renderer creates no
    /// surface and no instance, and never presents.
    pub fn with_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        size: (u32, u32),
    ) -> Self {
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        Self::build(None, device, queue, format, surface_config, None, None)
    }

    fn build(
        instance: Option<wgpu::Instance>,
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        surface_config: wgpu::SurfaceConfiguration,
        surface: Option<wgpu::Surface<'static>>,
        create_surface: Option<SurfaceFactory>,
    ) -> Self {
        let (font_system, font_generation) = shared_fonts();
        let cache = Cache::new(&device);
        let atlas = TextAtlas::new(&device, &queue, &cache, format);
        let quads = Quads::new(&device, format);
        let images = Images::new(&device, format, quads.viewport_layout());
        let paths = Paths::new(&device, format, quads.viewport_layout());
        let composite = Composite::new(&device, &queue, format);
        let blur = Blur::new(&device, &queue, format);
        let shaders = Shaders::new(&device, format, quads.viewport_layout());
        let [r, g, b] = BACKGROUND.map(|channel| {
            let value = f32::from(channel) / 255.0;
            f64::from(if format.is_srgb() {
                srgb_to_linear(value)
            } else {
                value
            })
        });
        let clear = wgpu::Color { r, g, b, a: 1.0 };

        Self {
            instance,
            device,
            queue,
            format,
            surface_config,
            font_system,
            font_generation,
            shaped: HashMap::new(),
            measured: HashMap::new(),
            frame: 0,
            swash_cache: SwashCache::new(),
            text_cache: cache,
            text_viewports: HashMap::new(),
            atlas,
            text_renderers: Vec::new(),
            quads,
            images,
            paths,
            composite,
            blur,
            shaders,
            layer_pool: Default::default(),
            clear,
            surface,
            create_surface,
        }
    }

    /// Reconfigures the surface for a new size in physical pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        // A zero-sized surface cannot be configured (e.g. a minimized window).
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.surface_config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .ok()?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
    }

    #[test]
    fn renderers_on_one_thread_share_one_font_system() {
        let Some((device, queue)) = device() else {
            return;
        };
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let first = Renderer::with_device(device.clone(), queue.clone(), format, (64, 64));
        let second = Renderer::with_device(device, queue, format, (64, 64));
        assert!(
            Rc::ptr_eq(&first.font_system, &second.font_system),
            "a font registered through the text functions must reach every renderer"
        );
        assert!(Rc::ptr_eq(&first.font_generation, &second.font_generation));
    }
}
