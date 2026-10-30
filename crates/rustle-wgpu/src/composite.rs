//! Puts a rendered layer onto its parent, turned, scaled and faded.

use wgpu::util::DeviceExt;

/// How a layer is placed on its parent, in physical pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Placement {
    pub(crate) pivot: (f32, f32),
    pub(crate) rotation: f32,
    pub(crate) scale: f32,
    pub(crate) opacity: f32,
    /// Cuts the layer to a rounded rectangle as it is put on its parent.
    pub(crate) mask: Option<Mask>,
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
    size: [f32; 2],
    pivot: [f32; 2],
    rotation: [f32; 2],
    scale: f32,
    opacity: f32,
    mask_rect: [f32; 4],
    mask_radii: [f32; 4],
    /// The first is 1 when the mask applies.
    masked: [f32; 4],
}

pub(crate) struct Composite {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Composite {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[],
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
        }
    }

    /// Draws `layer`, which covers the whole target, onto the pass.
    pub(crate) fn draw(
        &self,
        device: &wgpu::Device,
        pass: &mut wgpu::RenderPass<'_>,
        layer: &wgpu::TextureView,
        target: (u32, u32),
        placement: Placement,
    ) {
        let params = Params {
            size: [target.0 as f32, target.1 as f32],
            pivot: [placement.pivot.0, placement.pivot.1],
            rotation: [placement.rotation.cos(), placement.rotation.sin()],
            scale: placement.scale,
            opacity: placement.opacity,
            mask_rect: placement.mask.map_or([0.0; 4], |mask| mask.rect),
            mask_radii: placement.mask.map_or([0.0; 4], |mask| mask.radii),
            masked: [f32::from(u8::from(placement.mask.is_some())), 0.0, 0.0, 0.0],
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("composite"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
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
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..6, 0..1);
    }
}
