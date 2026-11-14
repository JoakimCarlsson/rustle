//! Puts a rendered layer onto its parent, turned, scaled and faded.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::uniforms::Arena;
use rustle_graphics::BlendMode;

/// How a layer is placed on its parent, in physical pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    pub(crate) pivot: (f32, f32),
    pub(crate) rotation: f32,
    pub(crate) scale: f32,
    pub(crate) opacity: f32,
    /// Cuts the layer to a rounded rectangle as it is put on its parent.
    pub(crate) mask: Option<Mask>,
    /// Four rows of five numbers that recolor the layer as it is put on its parent.
    pub(crate) filter: Option<[f32; 20]>,
    /// How the layer combines with its parent.
    pub(crate) blend: BlendMode,
    /// The standard deviation, in physical pixels, of a blur applied to the layer first.
    pub(crate) blur: f32,
}

impl Placement {
    /// A placement that puts a layer on its parent as it is, with `mask` and `opacity`.
    pub(crate) fn plain(opacity: f32, mask: Option<Mask>) -> Self {
        Self {
            pivot: (0.0, 0.0),
            rotation: 0.0,
            scale: 1.0,
            opacity,
            mask,
            filter: None,
            blend: BlendMode::SrcOver,
            blur: 0.0,
        }
    }
}

/// A rounded rectangle in physical pixels that a layer is cut to, with an
/// antialiased edge.
///
/// Rounded clips are done by drawing the clipped content to a layer and
/// masking the layer as it is composited. A stencil would give a hard,
/// aliased edge; a mask in each pipeline's fragment shader would have to be
/// repeated for quads, glyphs, pictures and paths, and could not handle
/// nesting. The layer costs a texture, so rectangular clips stay scissors.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mask {
    /// x, y, width and height.
    pub(crate) rect: [f32; 4],
    /// Top left, top right, bottom right and bottom left.
    pub(crate) radii: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    dest: [f32; 4],
    source: [f32; 4],
    area: [f32; 4],
    pivot: [f32; 2],
    rotation: [f32; 2],
    scale: f32,
    opacity: f32,
    /// Pads to the 16-byte alignment of the vector that follows.
    padding: [f32; 2],
    mask_rect: [f32; 4],
    mask_radii: [f32; 4],
    /// The first is 1 when the mask applies and the second when the filter does.
    masked: [f32; 4],
    filter_rows: [[f32; 4]; 4],
    filter_offset: [f32; 4],
}

/// A texture a pass draws into or a layer is drawn from, and where it lies in the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Canvas {
    /// The texture's top-left corner in the target's physical pixels.
    pub(crate) origin: (u32, u32),
    /// The texture's size in physical pixels.
    pub(crate) size: (u32, u32),
}

/// `canvas` as the origin and size a shader reads.
fn canvas_vector(canvas: Canvas) -> [f32; 4] {
    [
        canvas.origin.0 as f32,
        canvas.origin.1 as f32,
        canvas.size.0 as f32,
        canvas.size.1 as f32,
    ]
}

/// The blend state that makes `mode` of a premultiplied layer onto its parent.
fn blend_state(mode: BlendMode) -> wgpu::BlendState {
    use wgpu::BlendFactor::{
        Dst, DstAlpha, One, OneMinusDstAlpha, OneMinusSrc, OneMinusSrcAlpha, SrcAlpha, Zero,
    };
    let component = |src_factor, dst_factor| wgpu::BlendComponent {
        src_factor,
        dst_factor,
        operation: wgpu::BlendOperation::Add,
    };
    let over = component(One, OneMinusSrcAlpha);
    let same = |src_factor, dst_factor| {
        let both = component(src_factor, dst_factor);
        wgpu::BlendState {
            color: both,
            alpha: both,
        }
    };
    match mode {
        BlendMode::SrcOver => same(One, OneMinusSrcAlpha),
        BlendMode::DstOver => same(OneMinusDstAlpha, One),
        BlendMode::Src => same(One, Zero),
        BlendMode::Clear => same(Zero, Zero),
        BlendMode::SrcIn => same(DstAlpha, Zero),
        BlendMode::SrcOut => same(OneMinusDstAlpha, Zero),
        BlendMode::SrcAtop => same(DstAlpha, OneMinusSrcAlpha),
        BlendMode::DstIn => same(Zero, SrcAlpha),
        BlendMode::DstOut => same(Zero, OneMinusSrcAlpha),
        BlendMode::Xor => same(OneMinusDstAlpha, OneMinusSrcAlpha),
        BlendMode::Plus => same(One, One),
        BlendMode::Multiply => wgpu::BlendState {
            color: component(Dst, OneMinusSrcAlpha),
            alpha: over,
        },
        BlendMode::Screen => wgpu::BlendState {
            color: component(One, OneMinusSrc),
            alpha: over,
        },
        BlendMode::Darken => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: One,
                dst_factor: One,
                operation: wgpu::BlendOperation::Min,
            },
            alpha: over,
        },
        BlendMode::Lighten => wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: One,
                dst_factor: One,
                operation: wgpu::BlendOperation::Max,
            },
            alpha: over,
        },
    }
}

pub(crate) struct Composite {
    /// One pipeline per blend mode, made the first time the mode is drawn.
    pipelines: RefCell<HashMap<BlendMode, wgpu::RenderPipeline>>,
    module: wgpu::ShaderModule,
    pipeline_layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Where each draw's parameters are written.
    arena: Arena,
    queue: wgpu::Queue,
}

impl Composite {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("composite"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::COMPOSITE.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        Self {
            pipelines: RefCell::new(HashMap::new()),
            module,
            pipeline_layout,
            format,
            layout,
            sampler,
            arena: Arena::new(device),
            queue: queue.clone(),
        }
    }

    /// Starts a frame: the parameters of the last one are written over.
    pub(crate) fn begin_frame(&self) {
        self.arena.begin_frame();
    }

    /// The pipeline that draws a layer with blend `mode`, made if it is the first use.
    fn pipeline(&self, device: &wgpu::Device, mode: BlendMode) -> wgpu::RenderPipeline {
        self.pipelines
            .borrow_mut()
            .entry(mode)
            .or_insert_with(|| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("composite"),
                    layout: Some(&self.pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &self.module,
                        entry_point: Some("vertex"),
                        compilation_options: Default::default(),
                        buffers: &[],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &self.module,
                        entry_point: Some("fragment"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: self.format,
                            blend: Some(blend_state(mode)),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .clone()
    }

    #[allow(clippy::too_many_arguments)]
    /// Draws `area` (left, top, width and height in target pixels) of `layer`, a texture
    /// that lies at `from` in the target, onto the pass, which draws into the texture at `onto`.
    pub(crate) fn draw(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        layer: &wgpu::TextureView,
        from: Canvas,
        area: (u32, u32, u32, u32),
        onto: Canvas,
        placement: Placement,
    ) {
        let params = Params {
            dest: canvas_vector(onto),
            source: canvas_vector(from),
            area: [area.0 as f32, area.1 as f32, area.2 as f32, area.3 as f32],
            pivot: [placement.pivot.0, placement.pivot.1],
            rotation: [placement.rotation.cos(), placement.rotation.sin()],
            scale: placement.scale,
            opacity: placement.opacity,
            padding: [0.0; 2],
            mask_rect: placement.mask.map_or([0.0; 4], |mask| mask.rect),
            mask_radii: placement.mask.map_or([0.0; 4], |mask| mask.radii),
            masked: [
                f32::from(u8::from(placement.mask.is_some())),
                f32::from(u8::from(placement.filter.is_some())),
                0.0,
                0.0,
            ],
            filter_rows: std::array::from_fn(|row| {
                let matrix = placement.filter.unwrap_or_default();
                std::array::from_fn(|column| matrix[row * 5 + column])
            }),
            filter_offset: std::array::from_fn(|row| {
                placement.filter.map_or(0.0, |matrix| matrix[row * 5 + 4])
            }),
        };
        let slot = self
            .arena
            .push(device, &self.queue, bytemuck::bytes_of(&params));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(layer),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &slot.buffer,
                        offset: slot.offset,
                        size: wgpu::BufferSize::new(size_of::<Params>() as u64),
                    }),
                },
            ],
        });
        pass.set_pipeline(&self.pipeline(device, placement.blend));
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..6, 0..1);
    }
}
