//! Creating the renderer: the device, the surface and what is built on them.

use glyphon::{Cache, FontSystem, SwashCache, TextAtlas, Viewport};
use std::collections::HashMap;

use crate::composite::Composite;
use crate::image::Images;
use crate::path::Paths;
use crate::quad::Quads;
use crate::{Renderer, Result, SurfaceFactory, srgb_to_linear};

/// The window background, as sRGB.
const BACKGROUND: [u8; 3] = [24, 24, 30];

impl Renderer {
    /// Creates a renderer drawing into `target` (e.g. an `Arc<winit::window::Window>`).
    ///
    /// `size` is the target's size in physical pixels. `target` is cloned if the
    /// surface has to be recreated after being lost.
    pub fn new<T>(instance: wgpu::Instance, target: T, size: (u32, u32)) -> Result<Self>
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
            // Opaque, so a window the compositor blends never shows through a faded layer.
            alpha_mode: if caps.alpha_modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
                wgpu::CompositeAlphaMode::Opaque
            } else {
                caps.alpha_modes.first().copied().unwrap_or_default()
            },
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);
        Ok(Self::build(
            instance,
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
            instance,
            device,
            queue,
            format,
            surface_config,
            None,
            None,
        ))
    }

    fn build(
        instance: wgpu::Instance,
        device: wgpu::Device,
        queue: wgpu::Queue,
        format: wgpu::TextureFormat,
        surface_config: wgpu::SurfaceConfiguration,
        surface: Option<wgpu::Surface<'static>>,
        create_surface: Option<SurfaceFactory>,
    ) -> Self {
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let atlas = TextAtlas::new(&device, &queue, &cache, format);
        let quads = Quads::new(&device, format);
        let images = Images::new(&device, format, quads.viewport_layout());
        let paths = Paths::new(&device, format, quads.viewport_layout());
        let composite = Composite::new(&device, format);
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
            font_system: FontSystem::new(),
            shaped: HashMap::new(),
            frame: 0,
            swash_cache: SwashCache::new(),
            viewport,
            atlas,
            text_renderers: Vec::new(),
            quads,
            images,
            paths,
            composite,
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
