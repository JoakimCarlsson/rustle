//! wgpu rendering backend for Rustle.
//!
//! The renderer only consumes [`DrawCommand`]s; it knows nothing about
//! composition or the UI tree. It also measures text for layout.
//!
//! Commands are drawn in order. Consecutive rectangles are drawn together,
//! then the text that follows them, so later commands paint over earlier ones.

use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;

use glyphon::cosmic_text::{Align, Ellipsize, EllipsizeHeightLimit};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight, Wrap,
};
use rustle_geometry::{Rect, Size};
use rustle_graphics::{DrawCommand, FontFamily, StyledSpan, TextAlign};
use rustle_text::{TextMeasurer, TextStyle};

mod composite;
mod image;
mod quad;

use composite::{Composite, Placement};
use image::Images;
use quad::{Instance, Quads};

type SurfaceFactory = Box<
    dyn Fn(
        &wgpu::Instance,
    ) -> std::result::Result<wgpu::Surface<'static>, wgpu::CreateSurfaceError>,
>;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// The window background, as sRGB.
const BACKGROUND: [u8; 3] = [24, 24, 30];

/// Decodes an sRGB channel in 0..=1 to linear light.
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

pub struct Renderer {
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    surface_config: wgpu::SurfaceConfiguration,

    font_system: FontSystem,
    /// Shaped text kept between frames, with the frame it was last used in.
    shaped: HashMap<String, (Rc<Buffer>, u64)>,
    frame: u64,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    /// One per layer that has text, grown as needed and reused across frames.
    text_renderers: Vec<TextRenderer>,
    quads: Quads,
    images: Images,
    composite: Composite,
    /// The background as the surface takes it: linear on an sRGB surface.
    clear: wgpu::Color,

    // Declared before `create_surface`, which may own the window the surface
    // draws into, so the surface is dropped first.
    surface: Option<wgpu::Surface<'static>>,
    create_surface: Option<SurfaceFactory>,
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

    /// Draws one frame. `scale_factor` converts the commands' logical
    /// coordinates to physical pixels.
    ///
    /// Returns `Ok(false)` if no frame was presented and the caller should
    /// try again later.
    pub fn render(&mut self, commands: &[DrawCommand], scale_factor: f32) -> Result<bool> {
        let (Some(surface), Some(create_surface)) = (&self.surface, &self.create_surface) else {
            return Err("this renderer has no window; use `snapshot`".into());
        };
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                surface.configure(&self.device, &self.surface_config);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                let surface = create_surface(&self.instance)?;
                surface.configure(&self.device, &self.surface_config);
                self.surface = Some(surface);
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("surface texture acquisition failed validation".into());
            }
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let commands = self.encode(commands, scale_factor, &view)?;
        self.queue.submit(Some(commands));
        self.queue.present(frame);
        self.atlas.trim();
        Ok(true)
    }

    /// Draws one frame into a picture and returns its size and straight RGBA
    /// pixels. Works on any renderer; the picture is as large as its target.
    pub fn snapshot(
        &mut self,
        commands: &[DrawCommand],
        scale_factor: f32,
    ) -> Result<(u32, u32, Vec<u8>)> {
        let (width, height) = (self.surface_config.width, self.surface_config.height);
        if self.format != wgpu::TextureFormat::Rgba8UnormSrgb
            && self.format != wgpu::TextureFormat::Rgba8Unorm
        {
            return Err("snapshots need a renderer with an RGBA target".into());
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder_commands = self.encode_into(commands, scale_factor, &view)?;

        // Rows in the copy are padded to a multiple of 256 bytes.
        let row = width * 4;
        let padded = row.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snapshot"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder_commands.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder_commands.finish()));
        self.atlas.trim();

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely())?;
        receiver.recv()??;
        let mapped = slice.get_mapped_range()?;
        let mut pixels = Vec::with_capacity((row * height) as usize);
        for line in mapped.chunks(padded as usize).take(height as usize) {
            pixels.extend_from_slice(&line[..row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        Ok((width, height, pixels))
    }

    fn encode(
        &mut self,
        commands: &[DrawCommand],
        scale_factor: f32,
        view: &wgpu::TextureView,
    ) -> Result<wgpu::CommandBuffer> {
        Ok(self.encode_into(commands, scale_factor, view)?.finish())
    }

    /// Records drawing `commands` into `view`, leaving the encoder open for more.
    fn encode_into(
        &mut self,
        commands: &[DrawCommand],
        scale_factor: f32,
        view: &wgpu::TextureView,
    ) -> Result<wgpu::CommandEncoder> {
        let target = (self.surface_config.width, self.surface_config.height);
        self.viewport.update(
            &self.queue,
            Resolution {
                width: target.0,
                height: target.1,
            },
        );
        self.quads.set_viewport(&self.queue, target, scale_factor);
        self.images.begin_frame();

        let items = self.layers(commands, scale_factor, target);
        let mut next_text = 0;
        let prepared = self.prepare_items(items, scale_factor, target, &mut next_text)?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.draw_items(&mut encoder, view, &prepared, Some(self.clear), target)?;
        Ok(encoder)
    }

    /// Uploads what each layer of `items` needs, in drawing order.
    fn prepare_items(
        &mut self,
        items: Vec<Item>,
        scale_factor: f32,
        target: (u32, u32),
        next_text: &mut usize,
    ) -> Result<Vec<PreparedItem>> {
        let mut prepared = Vec::with_capacity(items.len());
        for item in items {
            prepared.push(match item {
                Item::Layer(layer) => {
                    let quads = self.quads.upload(&self.device, &layer.quads);
                    let images = self.images.upload(&self.device, &layer.images);
                    let text = if layer.texts.is_empty() {
                        None
                    } else {
                        let index = *next_text;
                        *next_text += 1;
                        self.prepare_text(index, &layer.texts, scale_factor, target)?;
                        Some(index)
                    };
                    PreparedItem::Layer(Prepared {
                        scissor: layer.scissor,
                        quads,
                        images,
                        text,
                    })
                }
                Item::Group {
                    placement,
                    scissor,
                    items,
                } => PreparedItem::Group {
                    placement,
                    scissor,
                    items: self.prepare_items(items, scale_factor, target, next_text)?,
                },
            });
        }
        Ok(prepared)
    }

    /// Draws `items` into `view`, clearing it to `clear` first if given. A group
    /// is drawn into a texture of its own, then composited onto `view`.
    fn draw_items(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        items: &[PreparedItem],
        mut clear: Option<wgpu::Color>,
        target: (u32, u32),
    ) -> Result<()> {
        if items.is_empty() {
            // Nothing to draw still has to clear.
            drop(begin_pass(encoder, view, clear.take()));
            return Ok(());
        }
        let mut index = 0;
        while index < items.len() {
            match &items[index] {
                PreparedItem::Layer(_) => {
                    let start = index;
                    while index < items.len() && matches!(items[index], PreparedItem::Layer(_)) {
                        index += 1;
                    }
                    let mut pass = begin_pass(encoder, view, clear.take());
                    for item in &items[start..index] {
                        if let PreparedItem::Layer(layer) = item {
                            self.draw_layer(&mut pass, layer, target)?;
                        }
                    }
                }
                PreparedItem::Group {
                    placement,
                    scissor,
                    items: inner,
                } => {
                    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("layer"),
                        size: wgpu::Extent3d {
                            width: target.0,
                            height: target.1,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    });
                    let group_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                    self.draw_items(
                        encoder,
                        &group_view,
                        inner,
                        Some(wgpu::Color::TRANSPARENT),
                        target,
                    )?;
                    let mut pass = begin_pass(encoder, view, clear.take());
                    let (x, y, width, height) = scissor.unwrap_or((0, 0, target.0, target.1));
                    if width > 0 && height > 0 {
                        pass.set_scissor_rect(x, y, width, height);
                        self.composite.draw(
                            &self.device,
                            &mut pass,
                            &group_view,
                            target,
                            *placement,
                        );
                    }
                    index += 1;
                }
            }
        }
        Ok(())
    }

    fn draw_layer(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        layer: &Prepared,
        target: (u32, u32),
    ) -> Result<()> {
        let (x, y, width, height) = layer.scissor.unwrap_or((0, 0, target.0, target.1));
        if width == 0 || height == 0 {
            return Ok(());
        }
        pass.set_scissor_rect(x, y, width, height);
        if let Some(quads) = &layer.quads {
            self.quads.draw(pass, quads);
        }
        if let Some(images) = &layer.images {
            self.images.draw(pass, self.quads.viewport_group(), images);
        }
        if let Some(index) = layer.text {
            self.text_renderers[index].render(&self.atlas, &self.viewport, pass)?;
        }
        Ok(())
    }

    /// Splits `commands` into layers: each is the rectangles drawn together,
    /// the text drawn over them, and the clip they are cut to.
    fn layers(
        &mut self,
        commands: &[DrawCommand],
        scale_factor: f32,
        target: (u32, u32),
    ) -> Vec<Item> {
        let mut frames = vec![Frame::default()];
        let mut clips: Vec<Rect> = Vec::new();
        let mut current = Layer::new(scissor(&clips, scale_factor, target));
        for command in commands {
            match command {
                DrawCommand::PushLayer {
                    rect,
                    alpha,
                    rotation,
                    scale,
                } => {
                    push_layer(&mut frames, current.take());
                    let center = (
                        (rect.origin.x + rect.size.width / 2.0) * scale_factor,
                        (rect.origin.y + rect.size.height / 2.0) * scale_factor,
                    );
                    let scissor = scissor(&clips, scale_factor, target);
                    frames.push(Frame {
                        items: Vec::new(),
                        placement: Some(Placement {
                            pivot: center,
                            rotation: *rotation,
                            scale: *scale,
                            opacity: *alpha,
                        }),
                        scissor,
                    });
                    current = Layer::new(scissor);
                }
                DrawCommand::PopLayer => {
                    push_layer(&mut frames, current.take());
                    if frames.len() > 1 {
                        close_group(&mut frames);
                    }
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PushClip { rect } => {
                    push_layer(&mut frames, current.take());
                    clips.push(*rect);
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::PopClip => {
                    push_layer(&mut frames, current.take());
                    clips.pop();
                    current = Layer::new(scissor(&clips, scale_factor, target));
                }
                DrawCommand::Rect { .. }
                | DrawCommand::Shadow { .. }
                | DrawCommand::Gradient { .. }
                | DrawCommand::Arc { .. }
                | DrawCommand::Line { .. } => {
                    // A rectangle after text or a picture must paint over it, so it starts a new layer.
                    if !current.texts.is_empty() || !current.images.is_empty() {
                        let scissor = current.scissor;
                        push_layer(&mut frames, current.take());
                        current = Layer::new(scissor);
                    }
                    current.quads.extend(self.quads.instance(command));
                }
                DrawCommand::Image { .. } => {
                    // A picture after text or a rectangle must paint over it.
                    if !current.texts.is_empty() || !current.quads.is_empty() {
                        let scissor = current.scissor;
                        push_layer(&mut frames, current.take());
                        current = Layer::new(scissor);
                    }
                    current.images.extend(self.images.instance(
                        &self.device,
                        &self.queue,
                        command,
                        scale_factor,
                    ));
                }
                DrawCommand::Text {
                    x,
                    y,
                    width,
                    content,
                    color,
                    font_size,
                    line_height,
                    font_weight,
                    font_family,
                    align,
                    max_lines,
                } => {
                    let cache_key = format!(
                        "{content}\u{0}{font_weight}{font_family:?}{font_size}{line_height}{align:?}{max_lines:?}{width}"
                    );
                    let buffer = match self.shaped.get_mut(&cache_key) {
                        Some((buffer, used)) => {
                            *used = self.frame;
                            Rc::clone(buffer)
                        }
                        None => {
                            let buffer = Rc::new(shape(
                                &mut self.font_system,
                                Setting {
                                    content: Content::Plain {
                                        text: content,
                                        font_weight: *font_weight,
                                        font_family: *font_family,
                                    },
                                    font_size: *font_size,
                                    line_height: *line_height,
                                    align: *align,
                                    max_lines: *max_lines,
                                },
                                Some(*width + 1.0),
                            ));
                            self.shaped
                                .insert(cache_key, (Rc::clone(&buffer), self.frame));
                            buffer
                        }
                    };
                    current.texts.push(Shaped {
                        position: (*x, *y),
                        color: Color::rgba(color.r, color.g, color.b, color.a),
                        buffer,
                    });
                }
                DrawCommand::RichText {
                    x,
                    y,
                    width,
                    spans,
                    font_size,
                    line_height,
                    align,
                    max_lines,
                } => {
                    let buffer = Rc::new(shape(
                        &mut self.font_system,
                        Setting {
                            content: Content::Rich(spans),
                            font_size: *font_size,
                            line_height: *line_height,
                            align: *align,
                            max_lines: *max_lines,
                        },
                        Some(*width + 1.0),
                    ));
                    current.texts.push(Shaped {
                        position: (*x, *y),
                        color: Color::rgb(255, 255, 255),
                        buffer,
                    });
                }
            }
        }
        self.frame += 1;
        let frame = self.frame;
        self.shaped.retain(|_, (_, used)| frame - *used < 240);
        push_layer(&mut frames, current.take());
        while frames.len() > 1 {
            close_group(&mut frames);
        }
        frames.pop().map(|frame| frame.items).unwrap_or_default()
    }

    fn prepare_text(
        &mut self,
        index: usize,
        texts: &[Shaped],
        scale_factor: f32,
        target: (u32, u32),
    ) -> Result<()> {
        while self.text_renderers.len() <= index {
            self.text_renderers.push(TextRenderer::new(
                &mut self.atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                None,
            ));
        }
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: target.0 as i32,
            bottom: target.1 as i32,
        };
        let areas = texts.iter().map(|text| TextArea {
            buffer: &text.buffer,
            left: text.position.0 * scale_factor,
            top: text.position.1 * scale_factor,
            scale: scale_factor,
            bounds,
            default_color: text.color,
            custom_glyphs: &[],
        });
        self.text_renderers[index].prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        )?;
        Ok(())
    }
}

/// A run of rectangles with the text over them, cut to a clip.
struct Layer {
    /// In physical pixels as x, y, width and height; `None` is the whole target.
    scissor: Option<(u32, u32, u32, u32)>,
    quads: Vec<Instance>,
    images: Vec<(u64, image::Instance)>,
    texts: Vec<Shaped>,
}

impl Layer {
    fn new(scissor: Option<(u32, u32, u32, u32)>) -> Self {
        Self {
            scissor,
            quads: Vec::new(),
            images: Vec::new(),
            texts: Vec::new(),
        }
    }

    /// The layer, or nothing if it has nothing to draw.
    fn take(self) -> Option<Self> {
        (!self.quads.is_empty() || !self.images.is_empty() || !self.texts.is_empty())
            .then_some(self)
    }
}

struct Shaped {
    position: (f32, f32),
    color: Color,
    buffer: Rc<Buffer>,
}

/// What is ready to draw of a layer.
struct Prepared {
    scissor: Option<(u32, u32, u32, u32)>,
    quads: Option<quad::Batch>,
    images: Option<image::Batch>,
    /// Which text renderer holds the layer's text.
    text: Option<usize>,
}

/// Layers and groups of layers, in drawing order.
enum Item {
    Layer(Layer),
    /// Layers drawn as one picture and put on what is below as `placement` says.
    Group {
        placement: Placement,
        scissor: Option<(u32, u32, u32, u32)>,
        items: Vec<Item>,
    },
}

enum PreparedItem {
    Layer(Prepared),
    Group {
        placement: Placement,
        scissor: Option<(u32, u32, u32, u32)>,
        items: Vec<PreparedItem>,
    },
}

/// The items of a group being collected, and how it is placed once closed.
#[derive(Default)]
struct Frame {
    items: Vec<Item>,
    placement: Option<Placement>,
    scissor: Option<(u32, u32, u32, u32)>,
}

fn push_layer(frames: &mut [Frame], layer: Option<Layer>) {
    if let (Some(layer), Some(frame)) = (layer, frames.last_mut()) {
        frame.items.push(Item::Layer(layer));
    }
}

/// Ends the innermost group, adding it to the one around it.
fn close_group(frames: &mut Vec<Frame>) {
    let Some(frame) = frames.pop() else {
        return;
    };
    if let (Some(placement), Some(parent)) = (frame.placement, frames.last_mut()) {
        parent.items.push(Item::Group {
            placement,
            scissor: frame.scissor,
            items: frame.items,
        });
    }
}

/// Begins a pass drawing into `view`, clearing it first if `clear` is given.
fn begin_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    clear: Option<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: None,
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: match clear {
                    Some(color) => wgpu::LoadOp::Clear(color),
                    None => wgpu::LoadOp::Load,
                },
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

/// The pixels the innermost of `clips` leaves visible, or `None` without clips.
fn scissor(clips: &[Rect], scale_factor: f32, target: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let (first, rest) = clips.split_first()?;
    let edges = |rect: &Rect| {
        (
            rect.origin.x,
            rect.origin.y,
            rect.origin.x + rect.size.width,
            rect.origin.y + rect.size.height,
        )
    };
    let (left, top, right, bottom) = rest.iter().fold(edges(first), |(l, t, r, b), clip| {
        let (cl, ct, cr, cb) = edges(clip);
        (l.max(cl), t.max(ct), r.min(cr), b.min(cb))
    });
    let to_pixels =
        |value: f32, limit: u32| ((value * scale_factor).round().max(0.0) as u32).min(limit);
    let (left, top) = (to_pixels(left, target.0), to_pixels(top, target.1));
    let (right, bottom) = (to_pixels(right, target.0), to_pixels(bottom, target.1));
    Some((
        left,
        top,
        right.saturating_sub(left),
        bottom.saturating_sub(top),
    ))
}

impl TextMeasurer for Renderer {
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size {
        let buffer = shape(
            &mut self.font_system,
            Setting {
                content: Content::Plain {
                    text: content,
                    font_weight: style.font_weight.0,
                    font_family: style.font_family,
                },
                font_size: style.font_size,
                line_height: style.line_height,
                align: TextAlign::Start,
                max_lines: style.max_lines,
            },
            max_width.is_finite().then_some(max_width),
        );
        // A trailing line break leaves an empty last line the shaper does not report.
        let breaks = content.matches('\n').count() as u32 + 1;
        Self::size_of(&buffer, style.line_height, breaks)
    }

    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size {
        let buffer = shape(
            &mut self.font_system,
            Setting {
                content: Content::Rich(spans),
                font_size: style.font_size,
                line_height: style.line_height,
                align: TextAlign::Start,
                max_lines: style.max_lines,
            },
            max_width.is_finite().then_some(max_width),
        );
        Self::size_of(&buffer, style.line_height, 0)
    }
}

impl Renderer {
    fn size_of(buffer: &Buffer, line_height: f32, at_least: u32) -> Size {
        let (width, lines) = buffer
            .layout_runs()
            .fold((0.0_f32, 0_u32), |(width, lines), run| {
                (width.max(run.line_w), lines + 1)
            });
        Size::new(width, lines.max(at_least).max(1) as f32 * line_height)
    }
}

/// How a piece of text is set.
struct Setting<'a> {
    content: Content<'a>,
    font_size: f32,
    line_height: f32,
    align: TextAlign,
    max_lines: Option<usize>,
}

/// What a piece of text says.
enum Content<'a> {
    /// One run in one style.
    Plain {
        text: &'a str,
        font_weight: u16,
        font_family: FontFamily,
    },
    /// Runs in their own styles.
    Rich(&'a [StyledSpan]),
}

fn family(family: FontFamily) -> Family<'static> {
    match family {
        FontFamily::SansSerif => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    }
}

/// Shapes text in logical pixels, wrapping at `width` if there is one.
fn shape(font_system: &mut FontSystem, setting: Setting<'_>, width: Option<f32>) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(setting.font_size, setting.line_height),
    );
    buffer.set_size(width, None);
    buffer.set_wrap(Wrap::Word);
    buffer.set_ellipsize(match setting.max_lines {
        Some(lines) => Ellipsize::End(EllipsizeHeightLimit::Lines(lines)),
        None => Ellipsize::None,
    });
    let alignment = match setting.align {
        TextAlign::Start => None,
        TextAlign::Left => Some(Align::Left),
        TextAlign::Center => Some(Align::Center),
        TextAlign::Right => Some(Align::Right),
    };
    match setting.content {
        Content::Plain {
            text,
            font_weight,
            font_family,
        } => buffer.set_text(
            text,
            &Attrs::new()
                .family(family(font_family))
                .weight(Weight(font_weight)),
            Shaping::Advanced,
            alignment,
        ),
        Content::Rich(spans) => buffer.set_rich_text(
            spans.iter().map(|span| {
                (
                    span.text.as_str(),
                    Attrs::new()
                        .family(family(span.font_family))
                        .weight(Weight(span.font_weight))
                        .color(Color::rgba(
                            span.color.r,
                            span.color.g,
                            span.color.b,
                            span.color.a,
                        )),
                )
            }),
            &Attrs::new(),
            Shaping::Advanced,
            alignment,
        ),
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}
