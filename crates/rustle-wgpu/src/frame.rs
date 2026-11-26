//! Drawing a frame: uploading what the layers need, then recording the passes.

use std::rc::Rc;

use rustle_graphics::{BlendMode, DrawCommand};

use crate::composite::{Canvas, Mask, Placement};
use crate::layers::{Item, Run, grow, intersection};
use crate::quad::Viewport;
use crate::{Renderer, Result, image, path, quad, shaders};

impl Renderer {
    /// Draws one frame. `scale_factor` converts the commands' logical
    /// coordinates to physical pixels.
    ///
    /// Returns `Ok(false)` if no frame was presented and the caller should
    /// try again later.
    pub fn render(&mut self, commands: &[DrawCommand], scale_factor: f32) -> Result<bool> {
        let (Some(surface), Some(create_surface), Some(instance)) =
            (&self.surface, &self.create_surface, &self.instance)
        else {
            return Err("this renderer has no window; use `snapshot` or `render_into`".into());
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
                let surface = create_surface(instance)?;
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

    /// Records drawing `commands` into `view` on the caller's `encoder`, which the caller
    /// submits, and leaves presenting to the caller.
    ///
    /// `view` is a texture of this renderer's format and `size` physical pixels. `clear` is
    /// the color to clear it to first, or `None` to draw over what it holds, which is how an
    /// interface goes over a frame the host has already drawn. Call it at most once per
    /// submission, because the renderer keeps one set of uniforms between submissions.
    pub fn render_into(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        size: (u32, u32),
        commands: &[DrawCommand],
        scale_factor: f32,
        clear: Option<wgpu::Color>,
    ) -> Result<()> {
        self.resize(size.0, size.1);
        self.record(encoder, commands, scale_factor, view, clear)?;
        self.atlas.trim();
        Ok(())
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
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.record(&mut encoder, commands, scale_factor, view, Some(self.clear))?;
        Ok(encoder)
    }

    /// Uploads what `commands` need and records drawing them into `view` on `encoder`, at
    /// the size the renderer is configured for, clearing `view` to `clear` first if given.
    fn record(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        commands: &[DrawCommand],
        scale_factor: f32,
        view: &wgpu::TextureView,
        clear: Option<wgpu::Color>,
    ) -> Result<()> {
        let target = (self.surface_config.width, self.surface_config.height);
        self.images.begin_frame();
        self.quads.begin_frame();
        self.paths.begin_frame();
        self.composite.begin_frame();
        self.blur.begin_frame();
        self.layer_pool.borrow_mut().begin_frame(self.frame);
        if self.text_viewports.len() > 64 {
            self.text_viewports.clear();
        }
        let root = Canvas {
            origin: (0, 0),
            size: target,
        };
        let context = Context {
            scale: scale_factor,
            target,
        };

        let items = self.layers(commands, scale_factor, target);
        let mut next_text = 0;
        let prepared = self.prepare_items(items, scale_factor, root, &mut next_text)?;
        self.quads.flush(&self.device, &self.queue);
        self.paths.flush(&self.device, &self.queue);
        self.images.flush(&self.device, &self.queue);
        if !has_backdrop(&prepared) {
            return self.draw_items(
                encoder,
                (view, root),
                &mut Vec::new(),
                &prepared,
                clear,
                context,
            );
        }
        let texture = self.layer_texture(target);
        let scene = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut chain = vec![Ancestor {
            texture: Rc::clone(&texture),
            canvas: root,
        }];
        self.draw_items(
            encoder,
            (&scene, root),
            &mut chain,
            &prepared,
            Some(clear.unwrap_or(wgpu::Color::TRANSPARENT)),
            context,
        )?;
        let blend = if clear.is_some() {
            BlendMode::Src
        } else {
            BlendMode::SrcOver
        };
        let mut pass = begin_pass(encoder, view, None);
        self.composite.draw(
            &self.device,
            &mut pass,
            &scene,
            root,
            (0, 0, target.0, target.1),
            root,
            Placement {
                blend,
                ..Placement::plain(1.0, None)
            },
        );
        Ok(())
    }

    /// Uploads what each layer of `items` needs, in drawing order. `canvas` is the texture
    /// the items are drawn into.
    fn prepare_items(
        &mut self,
        items: Vec<Item>,
        scale_factor: f32,
        canvas: Canvas,
        next_text: &mut usize,
    ) -> Result<Vec<PreparedItem>> {
        let mut prepared = Vec::with_capacity(items.len());
        for item in items {
            prepared.push(match item {
                Item::Layer(layer) => {
                    let quads = self.quads.upload(&layer.quads);
                    let paths = self.paths.upload(&layer.paths);
                    let images = self.images.upload(&layer.images);
                    let text_clip = layer.text_clip();
                    let text = if layer.texts.is_empty() {
                        None
                    } else {
                        let index = *next_text;
                        *next_text += 1;
                        self.prepare_text(
                            index,
                            &layer.texts,
                            text_clip.is_some(),
                            scale_factor,
                            canvas,
                        )?;
                        Some(index)
                    };
                    PreparedItem::Layer(Prepared {
                        quads: quads.map(|batch| (batch, layer.quad_runs)),
                        paths: paths.map(|batch| (batch, layer.path_runs)),
                        images: images.map(|batch| (batch, layer.image_runs)),
                        text,
                        text_clip,
                    })
                }
                Item::Shader { scissor, call } => match self.shaders.prepare(&self.device, &call) {
                    Some(batch) => PreparedItem::Shader { scissor, batch },
                    None => continue,
                },
                Item::Backdrop {
                    rect,
                    corner_radius,
                    sigma,
                    scissor,
                } => PreparedItem::Backdrop {
                    rect,
                    corner_radius,
                    sigma,
                    scissor,
                },
                Item::Group {
                    placement,
                    scissor,
                    region,
                    items,
                } => {
                    if region.2 == 0 || region.3 == 0 {
                        continue;
                    }
                    let inner = self.canvas_for(region);
                    PreparedItem::Group {
                        placement,
                        scissor,
                        region,
                        canvas: inner,
                        items: self.prepare_items(items, scale_factor, inner, next_text)?,
                    }
                }
            });
        }
        Ok(prepared)
    }

    /// The canvas of a texture that holds `region` (left, top, width and height in target
    /// pixels). Its size is rounded up so that textures of similar sizes can be reused.
    fn canvas_for(&self, region: (u32, u32, u32, u32)) -> Canvas {
        let limit = self.device.limits().max_texture_dimension_2d;
        let round = |value: u32| (value.div_ceil(BUCKET) * BUCKET).min(limit.max(value));
        Canvas {
            origin: (region.0, region.1),
            size: (round(region.2), round(region.3)),
        }
    }

    /// Draws `items` into `view`, a view of the texture at `canvas` (given together), clearing it to `clear`
    /// first if given. A group is drawn into a texture of its own, just big enough for what it
    /// holds, then composited onto `view`.
    ///
    /// `chain` holds the textures the renderer made that are being drawn into, outermost
    /// first and `view`'s last, which a backdrop blur reads to see what has been drawn.
    fn draw_items(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (view, canvas): (&wgpu::TextureView, Canvas),
        chain: &mut Vec<Ancestor>,
        items: &[PreparedItem],
        mut clear: Option<wgpu::Color>,
        context: Context,
    ) -> Result<()> {
        if items.is_empty() {
            drop(begin_pass(encoder, view, clear.take()));
            return Ok(());
        }
        let viewport = self.quads.viewport(
            &self.device,
            &self.queue,
            canvas.origin,
            canvas.size,
            context.scale,
        );
        let clear_was_given = clear.is_some();
        let mut index = 0;
        while index < items.len() {
            match &items[index] {
                PreparedItem::Layer(_) | PreparedItem::Shader { .. } => {
                    let start = index;
                    while index < items.len()
                        && matches!(
                            items[index],
                            PreparedItem::Layer(_) | PreparedItem::Shader { .. }
                        )
                    {
                        index += 1;
                    }
                    let mut pass = begin_pass(encoder, view, clear.take());
                    for item in &items[start..index] {
                        match item {
                            PreparedItem::Layer(layer) => {
                                self.draw_layer(&mut pass, layer, canvas, &viewport)?
                            }
                            PreparedItem::Shader { scissor, batch } => {
                                if let Some((x, y, width, height)) = local_scissor(*scissor, canvas)
                                {
                                    pass.set_scissor_rect(x, y, width, height);
                                    self.shaders.draw(&mut pass, &viewport, batch);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                PreparedItem::Backdrop {
                    rect,
                    corner_radius,
                    sigma,
                    scissor,
                } => {
                    index += 1;
                    if clear.is_some() {
                        drop(begin_pass(encoder, view, clear.take()));
                    }
                    let below = if index == 1 && clear_was_given {
                        &chain[..chain.len().saturating_sub(1)]
                    } else {
                        &chain[..]
                    };
                    if *sigma <= 0.0 || below.is_empty() {
                        continue;
                    }
                    let Some((x, y, width, height)) = local_scissor(*scissor, canvas) else {
                        continue;
                    };
                    let area = (x + canvas.origin.0, y + canvas.origin.1, width, height);
                    let reach = (*sigma * 3.0).ceil() as u32 + 2;
                    let region = grow(area, reach, context.target);
                    let blurred = self.blurred_backdrop(encoder, below, region, *sigma);
                    let mut pass = begin_pass(encoder, view, None);
                    pass.set_scissor_rect(x, y, width, height);
                    self.composite.draw(
                        &self.device,
                        &mut pass,
                        &blurred.view,
                        blurred.canvas,
                        area,
                        canvas,
                        Placement::plain(
                            1.0,
                            Some(Mask {
                                rect: *rect,
                                radii: [*corner_radius; 4],
                            }),
                        ),
                    );
                }
                PreparedItem::Group { .. } => {
                    let mut run = Vec::new();
                    let mut pixels = 0_u64;
                    while let Some(PreparedItem::Group {
                        canvas: inner,
                        items: content,
                        ..
                    }) = items.get(index)
                    {
                        let backdrop = has_backdrop(content);
                        let size = u64::from(inner.size.0) * u64::from(inner.size.1);
                        if !run.is_empty() && (backdrop || pixels + size > BATCH_PIXELS) {
                            break;
                        }
                        run.push(index);
                        pixels += size;
                        index += 1;
                        if backdrop {
                            break;
                        }
                    }
                    let mut drawn = Vec::with_capacity(run.len());
                    for position in run {
                        let PreparedItem::Group {
                            placement,
                            scissor,
                            region,
                            canvas: inner,
                            items: content,
                        } = &items[position]
                        else {
                            continue;
                        };
                        let texture = self.layer_texture(inner.size);
                        let group_view =
                            texture.create_view(&wgpu::TextureViewDescriptor::default());
                        chain.push(Ancestor {
                            texture: Rc::clone(&texture),
                            canvas: *inner,
                        });
                        let result = self.draw_items(
                            encoder,
                            (&group_view, *inner),
                            chain,
                            content,
                            Some(wgpu::Color::TRANSPARENT),
                            context,
                        );
                        chain.pop();
                        result?;
                        let shown = if placement.blur > 0.0 {
                            self.blurred(encoder, (&group_view, *inner), *region, placement.blur)
                        } else {
                            Blurred {
                                view: group_view,
                                canvas: *inner,
                            }
                        };
                        drawn.push((shown, *placement, *scissor, *region));
                    }
                    let mut pass = begin_pass(encoder, view, clear.take());
                    for (shown, placement, scissor, region) in &drawn {
                        let area = intersection(scissor.unwrap_or(*region), *region);
                        let Some((x, y, width, height)) = local_scissor(*scissor, canvas) else {
                            continue;
                        };
                        if area.2 > 0 && area.3 > 0 {
                            pass.set_scissor_rect(x, y, width, height);
                            self.composite.draw(
                                &self.device,
                                &mut pass,
                                &shown.view,
                                shown.canvas,
                                area,
                                canvas,
                                *placement,
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// `region` (left, top, width and height in target pixels) of the texture `source` shows,
    /// which lies at the canvas given, blurred by `sigma` physical pixels: a pass along x into
    /// one pooled texture, then along y into another. Pixels beyond the region read its edge.
    fn blurred(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        (source, from): (&wgpu::TextureView, Canvas),
        region: (u32, u32, u32, u32),
        sigma: f32,
    ) -> Blurred {
        let canvas = self.canvas_for((region.0, region.1, region.2, region.3));
        let horizontal = self.layer_texture(canvas.size);
        let horizontal_view = horizontal.create_view(&wgpu::TextureViewDescriptor::default());
        self.blur.pass(
            &self.device,
            encoder,
            (
                source,
                (
                    region.0.saturating_sub(from.origin.0),
                    region.1.saturating_sub(from.origin.1),
                    region.2,
                    region.3,
                ),
            ),
            &horizontal_view,
            [1.0, 0.0],
            sigma,
        );
        let vertical = self.layer_texture(canvas.size);
        let vertical_view = vertical.create_view(&wgpu::TextureViewDescriptor::default());
        self.blur.pass(
            &self.device,
            encoder,
            (&horizontal_view, (0, 0, region.2, region.3)),
            &vertical_view,
            [0.0, 1.0],
            sigma,
        );
        Blurred {
            view: vertical_view,
            canvas,
        }
    }

    /// What has been drawn so far in `region`, blurred by `sigma`. With one texture being
    /// drawn into it is read directly; inside groups the textures are first put together,
    /// outermost first, so the blur sees what lies below the group as well.
    fn blurred_backdrop(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        chain: &[Ancestor],
        region: (u32, u32, u32, u32),
        sigma: f32,
    ) -> Blurred {
        if let [only] = chain {
            let view = only
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            return self.blurred(encoder, (&view, only.canvas), region, sigma);
        }
        let canvas = self.canvas_for(region);
        let flat = self.layer_texture(canvas.size);
        let flat_view = flat.create_view(&wgpu::TextureViewDescriptor::default());
        for (position, ancestor) in chain.iter().enumerate() {
            let clear = (position == 0).then_some(wgpu::Color::TRANSPARENT);
            let mut pass = begin_pass(encoder, &flat_view, clear);
            let covered = intersection(
                region,
                (
                    ancestor.canvas.origin.0,
                    ancestor.canvas.origin.1,
                    ancestor.canvas.size.0,
                    ancestor.canvas.size.1,
                ),
            );
            if covered.2 > 0 && covered.3 > 0 {
                let source = ancestor
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                self.composite.draw(
                    &self.device,
                    &mut pass,
                    &source,
                    ancestor.canvas,
                    covered,
                    canvas,
                    Placement::plain(1.0, None),
                );
            }
        }
        self.blurred(encoder, (&flat_view, canvas), region, sigma)
    }

    /// A texture of `size` to draw a group into, reused from an earlier frame if possible.
    fn layer_texture(&self, size: (u32, u32)) -> Rc<wgpu::Texture> {
        let mut pool = self.layer_pool.borrow_mut();
        if let Some(texture) = pool.take(size) {
            return texture;
        }
        let texture = Rc::new(self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("layer"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
        pool.created += 1;
        pool.hand_out(size, Rc::clone(&texture));
        texture
    }

    /// How many layer textures have been created so far.
    #[cfg(test)]
    pub(crate) fn layer_textures_made(&self) -> usize {
        self.layer_pool.borrow().created
    }

    /// Draws one layer of rectangles, outlines, pictures and text into the pass, which draws
    /// into the texture at `canvas`.
    fn draw_layer(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        layer: &Prepared,
        canvas: Canvas,
        viewport: &Viewport,
    ) -> Result<()> {
        if let Some((batch, runs)) = &layer.quads {
            for (scissor, range) in scissored(runs, canvas) {
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                self.quads.draw(pass, viewport, batch, range);
            }
        }
        if let Some((batch, runs)) = &layer.paths {
            for (scissor, range) in scissored(runs, canvas) {
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                self.paths.draw(pass, viewport, batch, range);
            }
        }
        if let Some((batch, runs)) = &layer.images {
            for (scissor, range) in scissored(runs, canvas) {
                pass.set_scissor_rect(scissor.0, scissor.1, scissor.2, scissor.3);
                self.images.draw(pass, viewport, batch, range);
            }
        }
        if let Some(index) = layer.text {
            let Some((x, y, width, height)) = local_scissor(layer.text_clip.flatten(), canvas)
            else {
                return Ok(());
            };
            pass.set_scissor_rect(x, y, width, height);
            self.text_renderers[index].render(
                &self.atlas,
                &self.text_viewports[&canvas.size],
                pass,
            )?;
        }
        Ok(())
    }
}

/// The most pixels of group textures drawn before they are composited together in one pass.
const BATCH_PIXELS: u64 = 16 * 1024 * 1024;

/// How many pixels the sizes of the textures groups are drawn into are rounded up to.
const BUCKET: u32 = 64;

/// What a frame is drawn with.
#[derive(Clone, Copy)]
struct Context {
    /// Physical pixels per logical pixel.
    scale: f32,
    /// The size of the frame's target in physical pixels.
    target: (u32, u32),
}

/// A texture being drawn into, with where it lies in the target.
struct Ancestor {
    texture: Rc<wgpu::Texture>,
    canvas: Canvas,
}

/// A blurred picture of a region, in a pooled texture.
struct Blurred {
    view: wgpu::TextureView,
    canvas: Canvas,
}

/// Whether any of `items`, or the groups in them, blurs what is behind it.
fn has_backdrop(items: &[PreparedItem]) -> bool {
    items.iter().any(|item| match item {
        PreparedItem::Backdrop { .. } => true,
        PreparedItem::Group { items, .. } => has_backdrop(items),
        _ => false,
    })
}

/// `scissor` (target pixels; `None` is everything) cut to the texture at `canvas` and made
/// relative to it, or `None` if nothing of it is left.
pub(crate) fn local_scissor(
    scissor: Option<(u32, u32, u32, u32)>,
    canvas: Canvas,
) -> Option<(u32, u32, u32, u32)> {
    let whole = (
        canvas.origin.0,
        canvas.origin.1,
        canvas.size.0,
        canvas.size.1,
    );
    let (x, y, width, height) = scissor.map_or(whole, |scissor| intersection(scissor, whole));
    (width > 0 && height > 0).then_some((x - canvas.origin.0, y - canvas.origin.1, width, height))
}

/// Textures groups are drawn into: kept between frames, because clips and
/// layers need one each every frame and creating them is slow.
#[derive(Default)]
pub(crate) struct LayerPool {
    frame: u64,
    free: Vec<Pooled>,
    /// Textures handed out this frame; free again when the next frame begins.
    busy: Vec<Pooled>,
    /// How many textures the pool has made, which tests use to see that they are reused.
    pub(crate) created: usize,
}

/// A pooled texture and the frame it was last handed out in.
struct Pooled {
    size: (u32, u32),
    texture: Rc<wgpu::Texture>,
    used: u64,
}

impl LayerPool {
    /// Makes last frame's textures available, dropping those not used for a while.
    fn begin_frame(&mut self, frame: u64) {
        self.frame = frame;
        self.free.append(&mut self.busy);
        self.free.retain(|pooled| frame - pooled.used < 120);
    }

    /// A free texture of exactly `size`, handed out for the rest of the frame.
    fn take(&mut self, size: (u32, u32)) -> Option<Rc<wgpu::Texture>> {
        let position = self.free.iter().position(|pooled| pooled.size == size)?;
        let mut pooled = self.free.swap_remove(position);
        pooled.used = self.frame;
        let texture = Rc::clone(&pooled.texture);
        self.busy.push(pooled);
        Some(texture)
    }

    /// Records a texture just made as handed out for the rest of the frame.
    fn hand_out(&mut self, size: (u32, u32), texture: Rc<wgpu::Texture>) {
        self.busy.push(Pooled {
            size,
            texture,
            used: self.frame,
        });
    }
}

/// What is ready to draw of a layer: each kind's instances with the scissors they are cut to.
struct Prepared {
    quads: Option<(quad::Batch, Vec<Run>)>,
    paths: Option<(path::Batch, Vec<Run>)>,
    images: Option<(image::Batch, Vec<Run>)>,
    /// Which text renderer holds the layer's text.
    text: Option<usize>,
    /// The clip the scissor cuts all the layer's text to, when glyphon's bounds cannot;
    /// `None` when glyphon cuts each text to its own.
    text_clip: Option<Option<(u32, u32, u32, u32)>>,
}

/// The runs of `runs` that show in the texture at `canvas`, with their scissors made
/// relative to it and the instances each draws.
fn scissored(
    runs: &[Run],
    canvas: Canvas,
) -> impl Iterator<Item = ((u32, u32, u32, u32), std::ops::Range<u32>)> + '_ {
    runs.iter().filter_map(move |run| {
        local_scissor(run.scissor, canvas).map(|scissor| (scissor, run.start..run.end))
    })
}

enum PreparedItem {
    Layer(Prepared),
    Shader {
        scissor: Option<(u32, u32, u32, u32)>,
        batch: shaders::Batch,
    },
    Backdrop {
        rect: [f32; 4],
        corner_radius: f32,
        sigma: f32,
        scissor: Option<(u32, u32, u32, u32)>,
    },
    Group {
        placement: Placement,
        scissor: Option<(u32, u32, u32, u32)>,
        region: (u32, u32, u32, u32),
        /// The texture the group is drawn into.
        canvas: Canvas,
        items: Vec<PreparedItem>,
    },
}

/// Begins a pass drawing into `view`, clearing it first if `clear` is given.
pub(crate) fn begin_pass<'a>(
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
