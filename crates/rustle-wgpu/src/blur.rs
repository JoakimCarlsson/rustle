//! Gaussian blur of a rectangle of a texture, in two passes, one along each axis.

use crate::uniforms::Arena;

/// What the blur shader reads: a direction in texels, the deviation and tap spacing.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    direction: [f32; 2],
    sigma: f32,
    step: f32,
    /// The first is how many taps are taken on each side of the center.
    taps: [f32; 4],
    /// The rectangle read, as left and top in texels, then width and height.
    area: [f32; 4],
}

/// The most taps on a side: a larger deviation spaces the taps further apart instead.
const MAX_TAPS: f32 = 24.0;

pub(crate) struct Blur {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Where each pass's parameters are written.
    arena: Arena,
    queue: wgpu::Queue,
}

impl Blur {
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur"),
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
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
            label: Some("blur"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::BLUR.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur"),
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
                    blend: None,
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
            arena: Arena::new(device),
            queue: queue.clone(),
        }
    }

    /// Starts a frame: the parameters of the last one are written over.
    pub(crate) fn begin_frame(&self) {
        self.arena.begin_frame();
    }

    /// Records one pass that writes `area` of `source` (left, top, width and height in
    /// texels) blurred along `direction` (a unit step along x or y) by `sigma` physical
    /// pixels into the top-left corner of `target`, replacing what it holds. Taps beyond
    /// the area read its nearest edge.
    pub(crate) fn pass(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        (source, area): (&wgpu::TextureView, (u32, u32, u32, u32)),
        target: &wgpu::TextureView,
        direction: [f32; 2],
        sigma: f32,
    ) {
        let step = (sigma * 3.0 / MAX_TAPS).max(1.0);
        let taps = (sigma * 3.0 / step).ceil().max(1.0);
        let params = Params {
            direction,
            sigma,
            step,
            taps: [taps, 0.0, 0.0, 0.0],
            area: [area.0 as f32, area.1 as f32, area.2 as f32, area.3 as f32],
        };
        let slot = self
            .arena
            .push(device, &self.queue, bytemuck::bytes_of(&params));
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
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
        let mut pass = crate::frame::begin_pass(encoder, target, Some(wgpu::Color::TRANSPARENT));
        pass.set_scissor_rect(0, 0, area.2, area.3);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..6, 0..1);
    }
}
