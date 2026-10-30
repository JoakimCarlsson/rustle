//! Drawing a frame: uploading what the layers need, then recording the passes.

use glyphon::Resolution;
use rustle_graphics::DrawCommand;

use crate::composite::Placement;
use crate::layers::Item;
use crate::{Renderer, Result, image, path, quad};

impl Renderer {
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
        self.layer_pool.borrow_mut().begin_frame(target);

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
                    let paths = self.paths.upload(&self.device, &layer.paths);
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
                        paths,
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
                    let texture = self.layer_texture(target);
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

    /// A texture to draw a group into, reused from an earlier frame if possible.
    fn layer_texture(&self, target: (u32, u32)) -> std::rc::Rc<wgpu::Texture> {
        let mut pool = self.layer_pool.borrow_mut();
        let texture = pool.free.pop().unwrap_or_else(|| {
            std::rc::Rc::new(self.device.create_texture(&wgpu::TextureDescriptor {
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
            }))
        });
        pool.busy.push(std::rc::Rc::clone(&texture));
        texture
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
        if let Some(paths) = &layer.paths {
            self.paths.draw(pass, self.quads.viewport_group(), paths);
        }
        if let Some(images) = &layer.images {
            self.images.draw(pass, self.quads.viewport_group(), images);
        }
        if let Some(index) = layer.text {
            self.text_renderers[index].render(&self.atlas, &self.viewport, pass)?;
        }
        Ok(())
    }
}

/// Textures groups are drawn into: kept between frames, because clips and
/// layers need one each every frame and creating them is slow.
#[derive(Default)]
pub(crate) struct LayerPool {
    size: (u32, u32),
    free: Vec<std::rc::Rc<wgpu::Texture>>,
    /// Textures handed out this frame; free again when the next frame begins.
    busy: Vec<std::rc::Rc<wgpu::Texture>>,
}

impl LayerPool {
    /// Makes last frame's textures available, dropping them if the target changed size.
    fn begin_frame(&mut self, target: (u32, u32)) {
        if self.size != target {
            self.size = target;
            self.free.clear();
            self.busy.clear();
        }
        self.free.append(&mut self.busy);
    }
}

/// What is ready to draw of a layer.
struct Prepared {
    scissor: Option<(u32, u32, u32, u32)>,
    quads: Option<quad::Batch>,
    paths: Option<path::Batch>,
    images: Option<image::Batch>,
    /// Which text renderer holds the layer's text.
    text: Option<usize>,
}

enum PreparedItem {
    Layer(Prepared),
    Group {
        placement: Placement,
        scissor: Option<(u32, u32, u32, u32)>,
        items: Vec<PreparedItem>,
    },
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
