//! Glyphs drawn from atlases of their own, as instanced quads.
//!
//! Each glyph is rasterised once into an atlas, a coverage mask or a color picture, and kept
//! there. A shaped text works out once, at a scale, which glyphs it draws where and where in
//! the atlases they are; drawing it in a frame copies that list, moved to where the text is
//! and cut to its clip. So a frame of text costs a copy of a few bytes a glyph, with no glyph
//! looked up or hashed, however much text it draws.

use std::collections::HashMap;
use std::ops::Range;

use etagere::{BucketedAtlasAllocator, size2};
use glyphon::cosmic_text::{CacheKey, SwashContent};
use glyphon::{Buffer, FontSystem, SwashCache};

use crate::instances::InstanceBuffer;
use crate::quad::Viewport;

/// The side of an atlas when it is made, in texels.
const FIRST_SIDE: u32 = 512;

/// The largest side an atlas grows to, in texels: 16 MB of masks and 64 MB of color glyphs
/// at most. An atlas that fills is emptied before the next frame, and its glyphs put in again
/// as they are drawn.
const MOST_SIDE: u32 = 4096;

/// One glyph as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    /// The top-left corner in the target's physical pixels.
    position: [i32; 2],
    /// Width in the low 16 bits, height in the high.
    size: u32,
    /// The top-left texel in the atlas: x in the low 16 bits, y in the high.
    texel: u32,
    /// Straight-alpha color as `0xAARRGGBB`.
    color: u32,
    /// Which atlas, and whether to decode the color to linear light; see the shader.
    kind: u32,
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        0 => Sint32x2,
        1 => Uint32,
        2 => Uint32,
        3 => Uint32,
        4 => Uint32,
    ];
}

/// Where a rasterised glyph is: which atlas, where in it, and how its picture lies from the
/// pen position.
#[derive(Clone, Copy)]
struct Entry {
    /// Whether it is in the color atlas rather than the mask atlas.
    color: bool,
    /// Its top-left texel in the atlas.
    texel: (u16, u16),
    /// Its size in texels.
    size: (u16, u16),
    /// How far its picture starts right of the pen position.
    left: i32,
    /// How far its picture starts above the baseline.
    top: i32,
}

/// One glyph of a shaped text, placed from the text's origin on whole physical pixels.
#[derive(Clone, Copy)]
pub(crate) struct Placed {
    /// Its top-left corner from the text's origin.
    position: (i32, i32),
    /// Its size, its top-left texel and whether it is in the color atlas.
    entry: Entry,
    /// Its own color as `0xAARRGGBB`, or `None` for the text's.
    color: Option<u32>,
    /// The top and bottom of the box of its line from the text's origin, which decide
    /// whether the line shows inside a clip, as each glyph's own picture does not.
    line: (i32, i32),
}

/// Where the glyphs of a shaped text land, worked out at a scale and an atlas generation.
pub(crate) struct Laid {
    /// The bits of the scale the glyphs were placed at.
    scale: u32,
    /// The generation of the atlases their texels are in.
    generation: u64,
    /// The glyphs that draw something, in order.
    pub(crate) glyphs: Vec<Placed>,
}

impl Laid {
    /// Whether these glyphs were placed at `scale` in the atlases of `generation`.
    pub(crate) fn fits(&self, scale: f32, generation: u64) -> bool {
        self.scale == scale.to_bits() && self.generation == generation
    }
}

/// A rectangle of pixels as left, top, right and bottom edges.
pub(crate) type Edges = (i32, i32, i32, i32);

/// One atlas texture and the allocator of its space.
struct Page {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    packer: BucketedAtlasAllocator,
    side: u32,
    format: wgpu::TextureFormat,
    /// Bytes a texel.
    channels: u32,
}

impl Page {
    /// An empty atlas `side` texels square of `format`, `channels` bytes a texel.
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, channels: u32, side: u32) -> Self {
        let texture = Self::texture(device, format, side);
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
            packer: BucketedAtlasAllocator::new(size2(side as i32, side as i32)),
            side,
            format,
            channels,
        }
    }

    /// A texture for an atlas `side` texels square.
    fn texture(device: &wgpu::Device, format: wgpu::TextureFormat, side: u32) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d {
                width: side,
                height: side,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })
    }

    /// Room for a picture of `size`, growing the atlas up to `most` texels a side, copying
    /// what it holds, when it has none. `None` when even the largest atlas has none.
    fn allocate(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        size: (u32, u32),
        most: u32,
    ) -> Option<(u16, u16)> {
        loop {
            if let Some(allocation) = self.packer.allocate(size2(size.0 as i32, size.1 as i32)) {
                let min = allocation.rectangle.min;
                return Some((min.x as u16, min.y as u16));
            }
            if self.side >= most {
                return None;
            }
            let side = (self.side * 2).min(most);
            let texture = Self::texture(device, self.format, side);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("glyph atlas growth"),
            });
            encoder.copy_texture_to_texture(
                self.texture.as_image_copy(),
                texture.as_image_copy(),
                wgpu::Extent3d {
                    width: self.side,
                    height: self.side,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(Some(encoder.finish()));
            self.packer.grow(size2(side as i32, side as i32));
            self.view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.texture = texture;
            self.side = side;
        }
    }

    /// Writes `data`, a picture of `size`, at `texel`.
    fn write(&self, queue: &wgpu::Queue, texel: (u16, u16), size: (u32, u32), data: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: u32::from(texel.0),
                    y: u32::from(texel.1),
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.0 * self.channels),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
        );
    }
}

/// The glyph atlases, the glyphs in them, and the pipeline that draws glyphs from them.
pub(crate) struct Glyphs {
    pipeline: wgpu::RenderPipeline,
    atlas_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    mask: Page,
    color: Page,
    /// The atlases bound for drawing, made again when one grows.
    group: wgpu::BindGroup,
    /// The glyphs rasterised, by what identifies their picture; `None` for one that draws
    /// nothing.
    entries: HashMap<CacheKey, Option<Entry>, foldhash::fast::FixedState>,
    /// Counts the times the atlases were emptied, which places worked out before are not in.
    generation: u64,
    /// Whether a glyph found no room this frame, so the atlases are emptied before the next.
    full: bool,
    /// The largest side an atlas may have.
    most: u32,
    /// The instance bits that tell the shader to decode colors to linear light.
    decode: u32,
    /// The glyphs of the frame, every layer's in one buffer.
    instances: InstanceBuffer,
}

impl Glyphs {
    /// The atlases and the pipeline for drawing into targets of `format`, with the viewports
    /// laid out as `viewport_layout` says.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let srgb = format.is_srgb();
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("glyph atlas"),
            entries: &[
                texture_entry(0),
                texture_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("glyph"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::GLYPH.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("glyph"),
            bind_group_layouts: &[Some(viewport_layout), Some(&atlas_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("glyph"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &Instance::ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas"),
            ..Default::default()
        });
        let most = device.limits().max_texture_dimension_2d.min(MOST_SIDE);
        let side = FIRST_SIDE.min(most);
        let mask = Page::new(device, wgpu::TextureFormat::R8Unorm, 1, side);
        let color_format = if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let color = Page::new(device, color_format, 4, side);
        let group = Self::group(device, &atlas_layout, &sampler, &mask, &color);
        Self {
            pipeline,
            atlas_layout,
            sampler,
            mask,
            color,
            group,
            entries: HashMap::default(),
            generation: 0,
            full: false,
            most,
            decode: u32::from(srgb) << 16,
            instances: InstanceBuffer::new("glyphs"),
        }
    }

    /// The bind group of the atlases `mask` and `color`.
    fn group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        mask: &Page,
        color: &Page,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("glyph atlas"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mask.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    /// The generation of the atlases: places worked out in another are not valid.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Starts a frame: the glyphs of the last one are forgotten, and the atlases are emptied
    /// if a glyph found no room in them.
    pub(crate) fn begin_frame(&mut self) {
        self.instances.begin_frame();
        if self.full {
            self.full = false;
            self.entries.clear();
            self.mask.packer.clear();
            self.color.packer.clear();
            self.generation += 1;
        }
    }

    /// Writes the glyphs staged this frame to the GPU; called before anything is drawn.
    pub(crate) fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.instances.flush(device, queue);
    }

    /// Where the glyph identified by `key` is in the atlases, rasterising it with
    /// `swash_cache` and putting it in one the first time it is asked for.
    fn entry(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        key: CacheKey,
    ) -> Option<Entry> {
        if let Some(entry) = self.entries.get(&key) {
            return *entry;
        }
        let entry = swash_cache
            .get_image_uncached(font_system, key)
            .and_then(|image| {
                let size = (image.placement.width, image.placement.height);
                if size.0 == 0 || size.1 == 0 {
                    return None;
                }
                let color = matches!(image.content, SwashContent::Color);
                let page = if color {
                    &mut self.color
                } else {
                    &mut self.mask
                };
                let before = page.side;
                let Some(texel) = page.allocate(device, queue, size, self.most) else {
                    self.full = true;
                    return None;
                };
                let data = match image.content {
                    SwashContent::SubpixelMask => subpixel_coverage(&image.data, size),
                    _ => image.data,
                };
                page.write(queue, texel, size, &data);
                if page.side != before {
                    self.group = Self::group(
                        device,
                        &self.atlas_layout,
                        &self.sampler,
                        &self.mask,
                        &self.color,
                    );
                }
                Some(Entry {
                    color,
                    texel,
                    size: (size.0 as u16, size.1 as u16),
                    left: image.placement.left,
                    top: image.placement.top,
                })
            });
        if !self.full {
            self.entries.insert(key, entry);
        }
        entry
    }

    /// Places the glyphs of `buffer` at `scale` from the origin on whole physical pixels,
    /// putting the ones not drawn before in the atlases, and says where their pictures land,
    /// grown by a pixel, and whether one reaches outside the box of its line.
    pub(crate) fn lay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
        buffer: &Buffer,
        scale: f32,
    ) -> (Laid, Option<Edges>, bool) {
        let mut glyphs = Vec::new();
        let mut ink: Option<Edges> = None;
        let mut overhang = false;
        for run in buffer.layout_runs() {
            let start = (run.line_top * scale) as i32;
            let end = start + (run.line_height * scale) as i32;
            let baseline = (run.line_y * scale).round() as i32;
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, 0.0), scale);
                let Some(entry) =
                    self.entry(device, queue, font_system, swash_cache, physical.cache_key)
                else {
                    continue;
                };
                let left = physical.x + entry.left;
                let top = baseline + physical.y - entry.top;
                let right = left + i32::from(entry.size.0);
                let bottom = top + i32::from(entry.size.1);
                overhang |= top < start + 1 || bottom > end;
                let grown = (left - 1, top - 1, right + 1, bottom + 1);
                ink = Some(ink.map_or(grown, |(l, t, r, b)| {
                    (
                        l.min(grown.0),
                        t.min(grown.1),
                        r.max(grown.2),
                        b.max(grown.3),
                    )
                }));
                glyphs.push(Placed {
                    position: (left, top),
                    entry,
                    color: glyph.color_opt.map(|color| color.0),
                    line: (start, end),
                });
            }
        }
        let laid = Laid {
            scale: scale.to_bits(),
            generation: self.generation,
            glyphs,
        };
        (laid, ink, overhang)
    }

    /// Stages the glyphs of `laid`, drawn from `origin` in `color` (`0xAARRGGBB`) where a
    /// glyph has none of its own, cut to `bounds`, and returns how many it staged. A line
    /// whose box lies outside `bounds` is left out whole, and a glyph reaching outside them
    /// is cut to them.
    pub(crate) fn stage(
        &mut self,
        laid: &Laid,
        origin: (i32, i32),
        color: u32,
        bounds: Edges,
    ) -> u32 {
        let mut staged = 0;
        for glyph in &laid.glyphs {
            let (line_top, line_bottom) = (origin.1 + glyph.line.0, origin.1 + glyph.line.1);
            if line_top > bounds.3 || line_bottom < bounds.1 {
                continue;
            }
            let entry = glyph.entry;
            let (mut x, mut y) = (origin.0 + glyph.position.0, origin.1 + glyph.position.1);
            let (mut width, mut height) = (i32::from(entry.size.0), i32::from(entry.size.1));
            let (mut u, mut v) = (i32::from(entry.texel.0), i32::from(entry.texel.1));
            if x > bounds.2 || x + width < bounds.0 || y > bounds.3 || y + height < bounds.1 {
                continue;
            }
            if x < bounds.0 {
                u += bounds.0 - x;
                width -= bounds.0 - x;
                x = bounds.0;
            }
            if x + width > bounds.2 {
                width = bounds.2 - x;
            }
            if y < bounds.1 {
                v += bounds.1 - y;
                height -= bounds.1 - y;
                y = bounds.1;
            }
            if y + height > bounds.3 {
                height = bounds.3 - y;
            }
            if width <= 0 || height <= 0 {
                continue;
            }
            let instance = Instance {
                position: [x, y],
                size: width as u32 | (height as u32) << 16,
                texel: u as u32 | (v as u32) << 16,
                color: glyph.color.unwrap_or(color),
                kind: u32::from(!entry.color) | self.decode,
            };
            self.instances.push(bytemuck::bytes_of(&instance));
            staged += 1;
        }
        staged
    }

    /// Where the next glyph staged goes in the frame's buffer, counted in glyphs.
    pub(crate) fn staged(&self) -> u32 {
        (self.instances.len() / size_of::<Instance>()) as u32
    }

    /// Draws the glyphs `range` of the frame's buffer into the pass, which draws into the
    /// texture `viewport` describes.
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        viewport: &Viewport,
        range: Range<u32>,
    ) {
        if range.is_empty() {
            return;
        }
        let bytes = u64::from(range.start) * size_of::<Instance>() as u64
            ..u64::from(range.end) * size_of::<Instance>() as u64;
        let Some(slice) = self.instances.slice(bytes) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        viewport.set(pass);
        pass.set_bind_group(1, &self.group, &[]);
        pass.set_vertex_buffer(0, slice);
        pass.draw(0..4, 0..range.len() as u32);
    }
}

/// The coverage of a subpixel mask of `size` pixels, a byte for each of its channels, as one
/// byte a pixel: the mean of its color channels.
fn subpixel_coverage(data: &[u8], size: (u32, u32)) -> Vec<u8> {
    let channels = (data.len() / (size.0 * size.1).max(1) as usize).max(1);
    let colors = channels.min(3);
    data.chunks_exact(channels)
        .map(|pixel| {
            let sum: u32 = pixel[..colors].iter().map(|&value| u32::from(value)).sum();
            (sum / colors as u32) as u8
        })
        .collect()
}
