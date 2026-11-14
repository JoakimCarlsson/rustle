//! Pictures drawn as instanced quads by the image shader in `rustle-shaders`.

use crate::quad::Viewport;
use std::collections::HashMap;

use rustle_graphics::{DrawCommand, ImageData};
use wgpu::util::DeviceExt;

/// One picture as the shader reads it, in physical pixels.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Instance {
    origin: [f32; 2],
    size: [f32; 2],
    /// Left, top, right and bottom in texture coordinates.
    uv: [f32; 4],
    /// The corner radius in physical pixels, then unused channels.
    color: [f32; 4],
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
    ];
}

/// The pictures of one layer: their instances and which texture each uses.
pub(crate) struct Batch {
    buffer: wgpu::Buffer,
    ids: Vec<u64>,
}

struct Cached {
    group: wgpu::BindGroup,
    last_used: u64,
}

pub(crate) struct Images {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<u64, Cached>,
    srgb_target: bool,
    frame: u64,
}

impl Images {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("picture"),
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
            label: Some("picture"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("image"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::IMAGE.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("image"),
            bind_group_layouts: &[Some(viewport_layout), Some(&layout)],
            immediate_size: 0,
        });
        // The shader outputs premultiplied alpha.
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("image"),
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
            textures: HashMap::new(),
            srgb_target: format.is_srgb(),
            frame: 0,
        }
    }

    /// Starts a frame; pictures not drawn for a while are forgotten.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.textures
            .retain(|_, cached| frame - cached.last_used < 120);
    }

    /// The instance that draws `command` if it is a picture, uploading the texture if new.
    pub(crate) fn instance(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        command: &DrawCommand,
        scale: f32,
    ) -> Option<(u64, Instance)> {
        let DrawCommand::Image {
            rect,
            image,
            corner_radius,
            opacity,
        } = command
        else {
            return None;
        };
        self.ensure(device, queue, image);
        Some((
            image.id(),
            Instance {
                origin: [rect.origin.x * scale, rect.origin.y * scale],
                size: [rect.size.width * scale, rect.size.height * scale],
                uv: [0.0, 0.0, 1.0, 1.0],
                color: [corner_radius * scale, *opacity, 0.0, 0.0],
            },
        ))
    }

    fn ensure(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, image: &ImageData) {
        if let Some(cached) = self.textures.get_mut(&image.id()) {
            cached.last_used = self.frame;
            return;
        }
        let format = if self.srgb_target {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("picture"),
                size: wgpu::Extent3d {
                    width: image.width(),
                    height: image.height(),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            image.pixels(),
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("picture"),
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
        });
        self.textures.insert(
            image.id(),
            Cached {
                group,
                last_used: self.frame,
            },
        );
    }

    /// Uploads `instances`, or returns `None` if there are none.
    pub(crate) fn upload(
        &self,
        device: &wgpu::Device,
        instances: &[(u64, Instance)],
    ) -> Option<Batch> {
        if instances.is_empty() {
            return None;
        }
        let raw: Vec<Instance> = instances.iter().map(|(_, instance)| *instance).collect();
        Some(Batch {
            buffer: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pictures"),
                contents: bytemuck::cast_slice(&raw),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            ids: instances.iter().map(|(id, _)| *id).collect(),
        })
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, viewport: &Viewport, batch: &Batch) {
        pass.set_pipeline(&self.pipeline);
        viewport.set(pass);
        pass.set_vertex_buffer(0, batch.buffer.slice(..));
        for (index, id) in batch.ids.iter().enumerate() {
            if let Some(cached) = self.textures.get(id) {
                pass.set_bind_group(1, &cached.group, &[]);
                pass.draw(0..6, index as u32..index as u32 + 1);
            }
        }
    }
}
