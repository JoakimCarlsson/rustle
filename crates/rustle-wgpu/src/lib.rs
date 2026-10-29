//! wgpu rendering backend for Rustle.
//!
//! The renderer only consumes [`DrawCommand`]s; it knows nothing about
//! composition or the UI tree. It also measures text for layout.
//!
//! All rectangles are drawn first, then all text, so text is always on top.

use std::error::Error;

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use rustle_geometry::Size;
use rustle_graphics::DrawCommand;
use rustle_text::{TextMeasurer, TextStyle};

mod quad;

use quad::Quads;

type SurfaceFactory = Box<
    dyn Fn(
        &wgpu::Instance,
    ) -> std::result::Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError>,
>;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.07,
    g: 0.07,
    b: 0.09,
    a: 1.0,
};

pub struct Renderer {
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface_config: wgpu::SurfaceConfiguration,

    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    quads: Quads,

    // Declared before `create_surface`, which may own the window the surface
    // draws into, so the surface is dropped first.
    surface: wgpu::Surface<'static>,
    create_surface: SurfaceFactory,
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
            alpha_mode: caps.alpha_modes.first().copied().unwrap_or_default(),
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &surface_config);

        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text_renderer =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);

        let quads = Quads::new(&device, format);

        Ok(Self {
            instance,
            device,
            queue,
            surface_config,
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            viewport,
            atlas,
            text_renderer,
            quads,
            surface,
            create_surface,
        })
    }

    /// Reconfigures the surface for a new size in physical pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        // A zero-sized surface cannot be configured (e.g. a minimized window).
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    /// Draws one frame. `scale_factor` converts the commands' logical
    /// coordinates to physical pixels.
    ///
    /// Returns `Ok(false)` if no frame was presented and the caller should
    /// try again later.
    pub fn render(&mut self, commands: &[DrawCommand], scale_factor: f32) -> Result<bool> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = (self.create_surface)(&self.instance)?;
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface texture acquisition failed validation".into());
            }
        };

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.surface_config.width,
                height: self.surface_config.height,
            },
        );

        let texts: Vec<_> = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text {
                    x,
                    y,
                    content,
                    color,
                    font_size,
                    line_height,
                } => Some((
                    (*x, *y),
                    *color,
                    shape(&mut self.font_system, content, *font_size, *line_height),
                )),
                _ => None,
            })
            .collect();
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: self.surface_config.width as i32,
            bottom: self.surface_config.height as i32,
        };
        let areas = texts.iter().map(|((x, y), color, buffer)| TextArea {
            buffer,
            left: x * scale_factor,
            top: y * scale_factor,
            scale: scale_factor,
            bounds,
            default_color: Color::rgba(color.r, color.g, color.b, color.a),
            custom_glyphs: &[],
        });
        self.text_renderer.prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        )?;

        let batch = self.quads.prepare(
            &self.device,
            &self.queue,
            commands,
            (self.surface_config.width, self.surface_config.height),
            scale_factor,
        );

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(batch) = &batch {
                self.quads.draw(&mut pass, batch);
            }
            self.text_renderer
                .render(&self.atlas, &self.viewport, &mut pass)?;
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        self.atlas.trim();
        Ok(true)
    }
}

impl TextMeasurer for Renderer {
    fn measure(&mut self, content: &str, style: &TextStyle) -> Size {
        let buffer = shape(
            &mut self.font_system,
            content,
            style.font_size,
            style.line_height,
        );
        let (width, lines) = buffer
            .layout_runs()
            .fold((0.0_f32, 0_u32), |(width, lines), run| {
                (width.max(run.line_w), lines + 1)
            });
        Size::new(width, lines.max(1) as f32 * style.line_height)
    }
}

/// Shapes `content` on unbounded lines, in logical pixels.
fn shape(font_system: &mut FontSystem, content: &str, font_size: f32, line_height: f32) -> Buffer {
    let mut buffer = Buffer::new(font_system, Metrics::new(font_size, line_height));
    buffer.set_size(None, None);
    buffer.set_text(
        content,
        &Attrs::new().family(Family::SansSerif),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(font_system, false);
    buffer
}
