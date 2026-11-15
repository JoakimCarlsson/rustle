//! Filled vector outlines, rasterised on the CPU into coverage masks and drawn by the
//! shader in `rustle-shaders`.
//!
//! An outline is rasterised at the physical size it is drawn at, with exact area
//! coverage, into a one-channel mask; its fractional position on the pixel grid is
//! baked into the mask, a quarter pixel at a time, and the mask is drawn at a whole
//! pixel so each texel lands on exactly one pixel of the target. Small outlines such
//! as icons therefore come out as crisp as text at any scale factor and position.
//! Masks are kept between frames, so an outline drawn again at the same size and
//! offset is not rasterised again.

use crate::quad::Viewport;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use rustle_graphics::{Color, DrawCommand, FillRule, Path, PathCommand};
use wgpu::util::DeviceExt;
use zeno::{Command, Fill, Mask as ZenoMask, Placement, Point, Scratch, Transform};

use crate::srgb_to_linear;

/// How many positions within a physical pixel a mask is rasterised for, along each axis.
const SUBPIXEL_STEPS: f32 = 4.0;

/// How many frames a mask is kept after it was last drawn.
const UNUSED_FRAMES: u64 = 240;

/// One outline as the shader reads it, in physical pixels.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Instance {
    /// The top left corner of the mask, on a whole pixel.
    origin: [f32; 2],
    /// The size of the mask, one texel per pixel.
    size: [f32; 2],
    /// Linear on an sRGB target, with straight alpha.
    color: [f32; 4],
}

impl Instance {
    /// The vertex attributes of [`Instance`], in field order.
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];
}

/// Identifies a mask: the outline, the physical size it is drawn at, and the
/// quarter pixel it starts at.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Key {
    /// A hash of the commands, view box and fill rule of the outline.
    outline: u64,
    /// The bits of the physical width the view box is stretched to.
    width: u32,
    /// The bits of the physical height the view box is stretched to.
    height: u32,
    /// Which subpixel step along x and y the outline starts at.
    step: (u8, u8),
}

/// The outlines of one layer, before they are uploaded.
#[derive(Default)]
pub(crate) struct Geometry {
    /// Each outline's mask and where it is drawn, in drawing order.
    instances: Vec<(Key, Instance)>,
}

impl Geometry {
    /// Whether the layer draws no outline.
    pub(crate) fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

/// The outlines of one layer, uploaded and ready to draw.
pub(crate) struct Batch {
    /// The instances, in drawing order.
    buffer: wgpu::Buffer,
    /// The mask each instance samples.
    keys: Vec<Key>,
}

/// A rasterised outline kept between frames.
struct Mask {
    /// The texture of the mask, or `None` when the outline covers no pixel.
    group: Option<wgpu::BindGroup>,
    /// Where the mask starts, in whole pixels from the pixel the outline's box starts in.
    left: i32,
    /// Where the mask starts, in whole pixels from the pixel row the outline's box starts in.
    top: i32,
    /// The width of the mask in pixels.
    width: u32,
    /// The height of the mask in pixels.
    height: u32,
    /// The frame it was last drawn in.
    last_used: u64,
}

/// Draws filled outlines.
pub(crate) struct Paths {
    /// The pipeline that draws mask quads.
    pipeline: wgpu::RenderPipeline,
    /// The layout of a mask's texture and sampler.
    layout: wgpu::BindGroupLayout,
    /// Samples masks texel for texel.
    sampler: wgpu::Sampler,
    /// Whether the target encodes linear values to sRGB on write.
    srgb_target: bool,
    /// The largest texture side the device takes.
    max_side: u32,
    /// Masks kept between frames.
    masks: HashMap<Key, Mask>,
    /// Memory the rasteriser reuses between outlines.
    scratch: Scratch,
    /// The number of the current frame.
    frame: u64,
}

impl Paths {
    /// Makes the pipeline for drawing outlines into targets of `format`.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("path mask"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("path mask"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::PATH.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("path"),
            bind_group_layouts: &[Some(viewport_layout), Some(&layout)],
            immediate_size: 0,
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("path"),
            layout: Some(&pipeline_layout),
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: premultiplied,
                        alpha: premultiplied,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            pipeline,
            layout,
            sampler,
            srgb_target: format.is_srgb(),
            max_side: device.limits().max_texture_dimension_2d,
            masks: HashMap::new(),
            scratch: Scratch::new(),
            frame: 0,
        }
    }

    /// Adds the quad that draws `command`, if it is an outline, to `geometry`,
    /// rasterising and uploading its mask if it has none yet.
    pub(crate) fn push(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        geometry: &mut Geometry,
        command: &DrawCommand,
        scale: f32,
    ) {
        let DrawCommand::Path { path, rect, color } = command else {
            return;
        };
        let size = (rect.size.width * scale, rect.size.height * scale);
        if !(size.0 > 0.0 && size.1 > 0.0) {
            return;
        }
        let (pixel_x, step_x) = snapped(rect.origin.x * scale);
        let (pixel_y, step_y) = snapped(rect.origin.y * scale);
        let key = Key {
            outline: outline_hash(path),
            width: size.0.to_bits(),
            height: size.1.to_bits(),
            step: (step_x, step_y),
        };
        if !self.masks.contains_key(&key) {
            let mask = self.rasterised(device, queue, path, size, key.step);
            self.masks.insert(key, mask);
        }
        let Some(mask) = self.masks.get_mut(&key) else {
            return;
        };
        mask.last_used = self.frame;
        if mask.group.is_none() {
            return;
        }
        geometry.instances.push((
            key,
            Instance {
                origin: [pixel_x + mask.left as f32, pixel_y + mask.top as f32],
                size: [mask.width as f32, mask.height as f32],
                color: self.channels(*color),
            },
        ));
    }

    /// Rasterises `path` stretched to `size` physical pixels and starting at subpixel
    /// `step`, and uploads the mask.
    fn rasterised(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        path: &Path,
        size: (f32, f32),
        step: (u8, u8),
    ) -> Mask {
        let (coverage, placement) = rasterise(&mut self.scratch, path, size, step);
        let empty = placement.width == 0
            || placement.height == 0
            || placement.width > self.max_side
            || placement.height > self.max_side
            || coverage.iter().all(|value| *value == 0);
        let group = (!empty).then(|| {
            let texture = device.create_texture_with_data(
                queue,
                &wgpu::TextureDescriptor {
                    label: Some("path mask"),
                    size: wgpu::Extent3d {
                        width: placement.width,
                        height: placement.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::R8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &coverage,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("path mask"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            })
        });
        Mask {
            group,
            left: placement.left,
            top: placement.top,
            width: placement.width,
            height: placement.height,
            last_used: self.frame,
        }
    }

    /// Forgets masks nothing has drawn for a while. Called once a frame.
    pub(crate) fn end_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.masks
            .retain(|_, mask| frame - mask.last_used < UNUSED_FRAMES);
    }

    /// Uploads `geometry`, or returns `None` if it is empty.
    pub(crate) fn upload(&self, device: &wgpu::Device, geometry: &Geometry) -> Option<Batch> {
        if geometry.is_empty() {
            return None;
        }
        let raw: Vec<Instance> = geometry
            .instances
            .iter()
            .map(|(_, instance)| *instance)
            .collect();
        Some(Batch {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("path instances"),
                contents: bytemuck::cast_slice(&raw),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            keys: geometry.instances.iter().map(|(key, _)| *key).collect(),
        })
    }

    /// Records drawing `batch` into `pass`.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, viewport: &Viewport, batch: &Batch) {
        pass.set_pipeline(&self.pipeline);
        viewport.set(pass);
        pass.set_vertex_buffer(0, batch.buffer.slice(..));
        for (index, key) in batch.keys.iter().enumerate() {
            if let Some(group) = self.masks.get(key).and_then(|mask| mask.group.as_ref()) {
                pass.set_bind_group(1, group, &[]);
                pass.draw(0..6, index as u32..index as u32 + 1);
            }
        }
    }

    /// The color as the shader takes it: linear if the target encodes to sRGB on write.
    fn channels(&self, color: Color) -> [f32; 4] {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if self.srgb_target {
                srgb_to_linear(value)
            } else {
                value
            }
        };
        [
            channel(color.r),
            channel(color.g),
            channel(color.b),
            f32::from(color.a) / 255.0,
        ]
    }
}

/// Splits a physical coordinate into the whole pixel a mask is drawn at and the
/// subpixel step, from 0 to [`SUBPIXEL_STEPS`] - 1, its outline is rasterised at.
fn snapped(coordinate: f32) -> (f32, u8) {
    let steps = (coordinate * SUBPIXEL_STEPS).round();
    let pixel = (steps / SUBPIXEL_STEPS).floor();
    (pixel, (steps - pixel * SUBPIXEL_STEPS) as u8)
}

/// A hash of what decides the shape of `path`: its commands, view box and fill rule.
fn outline_hash(path: &Path) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let (width, height) = path.size();
    (width.to_bits(), height.to_bits()).hash(&mut hasher);
    matches!(path.fill_rule(), FillRule::EvenOdd).hash(&mut hasher);
    for command in path.commands() {
        let values: &[f32] = match command {
            PathCommand::MoveTo(x, y) => &[0.0, *x, *y],
            PathCommand::LineTo(x, y) => &[1.0, *x, *y],
            PathCommand::QuadTo(cx, cy, x, y) => &[2.0, *cx, *cy, *x, *y],
            PathCommand::CubicTo(ax, ay, bx, by, x, y) => &[3.0, *ax, *ay, *bx, *by, *x, *y],
            PathCommand::Close => &[4.0],
        };
        for value in values {
            value.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// The commands of `path` as the rasteriser takes them, in view box units. Drawing
/// after a close begins a new subpath where the closed one began.
fn commands(path: &Path) -> Vec<Command> {
    let mut out = Vec::with_capacity(path.commands().len() + 1);
    let (mut open, mut start) = (false, Point::new(0.0, 0.0));
    for command in path.commands() {
        if !matches!(command, PathCommand::MoveTo(..) | PathCommand::Close) && !open {
            out.push(Command::MoveTo(start));
            open = true;
        }
        match *command {
            PathCommand::MoveTo(x, y) => {
                start = Point::new(x, y);
                open = true;
                out.push(Command::MoveTo(start));
            }
            PathCommand::LineTo(x, y) => out.push(Command::LineTo(Point::new(x, y))),
            PathCommand::QuadTo(cx, cy, x, y) => {
                out.push(Command::QuadTo(Point::new(cx, cy), Point::new(x, y)));
            }
            PathCommand::CubicTo(ax, ay, bx, by, x, y) => out.push(Command::CurveTo(
                Point::new(ax, ay),
                Point::new(bx, by),
                Point::new(x, y),
            )),
            PathCommand::Close => {
                if open {
                    out.push(Command::Close);
                    open = false;
                }
            }
        }
    }
    out
}

/// The coverage of `path` stretched over `size` physical pixels and moved right and
/// down by `step` subpixel steps, one byte per pixel, and where the mask lies relative
/// to the pixel the stretched view box starts in.
fn rasterise(
    scratch: &mut Scratch,
    path: &Path,
    size: (f32, f32),
    step: (u8, u8),
) -> (Vec<u8>, Placement) {
    let (width, height) = path.size();
    let transform = Transform::scale(
        size.0 / width.max(f32::EPSILON),
        size.1 / height.max(f32::EPSILON),
    )
    .then_translate(
        f32::from(step.0) / SUBPIXEL_STEPS,
        f32::from(step.1) / SUBPIXEL_STEPS,
    );
    let fill = match path.fill_rule() {
        FillRule::NonZero => Fill::NonZero,
        FillRule::EvenOdd => Fill::EvenOdd,
    };
    let commands = commands(path);
    ZenoMask::with_scratch(commands.as_slice(), scratch)
        .style(fill)
        .transform(Some(transform))
        .render()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A square filling a view box of `side` units.
    fn square(side: f32) -> Path {
        Path::new(
            vec![
                PathCommand::MoveTo(0.0, 0.0),
                PathCommand::LineTo(side, 0.0),
                PathCommand::LineTo(side, side),
                PathCommand::LineTo(0.0, side),
                PathCommand::Close,
            ],
            side,
            side,
            FillRule::NonZero,
        )
    }

    /// The mask has one texel per physical pixel at every scale factor.
    #[test]
    fn an_outline_is_rasterised_at_its_physical_size() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            let (coverage, placement) = rasterise(
                &mut Scratch::new(),
                &square(256.0),
                (16.0 * scale, 16.0 * scale),
                (0, 0),
            );
            let side = (16.0 * scale) as u32;
            assert_eq!((placement.left, placement.top), (0, 0));
            assert_eq!((placement.width, placement.height), (side, side));
            assert!(coverage.iter().all(|value| *value == 255));
        }
    }

    /// Half a pixel of offset leaves half the coverage in each edge pixel.
    #[test]
    fn a_fractional_position_shares_the_edge_pixels_by_area() {
        let (coverage, placement) =
            rasterise(&mut Scratch::new(), &square(10.0), (4.0, 4.0), (2, 0));
        assert_eq!((placement.width, placement.height), (5, 4));
        let row = &coverage[..5];
        assert!(row[0].abs_diff(128) <= 2);
        assert_eq!(&row[1..4], &[255, 255, 255]);
        assert!(row[4].abs_diff(128) <= 2);
    }

    /// Coordinates round to the nearest quarter pixel.
    #[test]
    fn a_coordinate_snaps_to_a_whole_pixel_and_a_quarter_step() {
        assert_eq!(snapped(10.0), (10.0, 0));
        assert_eq!(snapped(10.5), (10.0, 2));
        assert_eq!(snapped(10.9), (11.0, 0));
        assert_eq!(snapped(-0.25), (-1.0, 3));
    }
}
