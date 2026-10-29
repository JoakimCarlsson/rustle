//! Filled, rounded rectangles drawn as instanced quads by the shader in `rustle-shaders`.

use rustle_graphics::{Color, DrawCommand};
use wgpu::util::DeviceExt;

/// The viewport the shader turns logical pixels into clip space with.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Viewport {
    /// Target size in physical pixels.
    size: [f32; 2],
    /// Physical pixels per logical pixel.
    scale: f32,
    /// Padding to the 16-byte alignment a uniform block needs.
    padding: f32,
}

/// One quad as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    /// Left, top, width and height in logical pixels.
    rect: [f32; 4],
    fill: [f32; 4],
    border_color: [f32; 4],
    /// Corner radii, clockwise from the top-left.
    radii: [f32; 4],
    /// A 2x2 transform; the identity here.
    matrix: [f32; 4],
    /// Translation, border width and softness; all unused here.
    params: [f32; 4],
    /// Arc sweep and ring width; a width of zero draws a rectangle.
    arc: [f32; 4],
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
    ];
}

/// The quads of one frame, uploaded and ready to draw.
pub(crate) struct Batch {
    buffer: wgpu::Buffer,
    count: u32,
}

pub(crate) struct Quads {
    pipeline: wgpu::RenderPipeline,
    viewport_buffer: wgpu::Buffer,
    viewport_group: wgpu::BindGroup,
    /// Whether the target encodes linear values to sRGB on write.
    srgb_target: bool,
}

impl Quads {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport"),
            size: size_of::<Viewport>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let viewport_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport"),
            layout: &viewport_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::QUAD.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad"),
            bind_group_layouts: &[Some(&viewport_layout)],
            immediate_size: 0,
        });
        // The shader outputs premultiplied alpha.
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quad"),
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
            viewport_buffer,
            viewport_group,
            srgb_target: format.is_srgb(),
        }
    }

    /// Uploads the rectangles among `commands`, or returns `None` if there are none.
    pub(crate) fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        commands: &[DrawCommand],
        target_size: (u32, u32),
        scale: f32,
    ) -> Option<Batch> {
        let instances: Vec<Instance> = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Rect {
                    rect,
                    color,
                    corner_radius,
                } => Some(Instance {
                    rect: [
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    ],
                    fill: self.channels(*color),
                    border_color: [0.0; 4],
                    radii: [*corner_radius; 4],
                    matrix: [1.0, 0.0, 0.0, 1.0],
                    params: [0.0; 4],
                    arc: [0.0; 4],
                }),
                _ => None,
            })
            .collect();
        if instances.is_empty() {
            return None;
        }

        queue.write_buffer(
            &self.viewport_buffer,
            0,
            bytemuck::bytes_of(&Viewport {
                size: [target_size.0 as f32, target_size.1 as f32],
                scale,
                padding: 0.0,
            }),
        );
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quads"),
            contents: bytemuck::cast_slice(&instances),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Some(Batch {
            buffer,
            count: instances.len() as u32,
        })
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, batch: &Batch) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.viewport_group, &[]);
        pass.set_vertex_buffer(0, batch.buffer.slice(..));
        pass.draw(0..6, 0..batch.count);
    }

    /// The color as the shader takes it. An sRGB target encodes what the
    /// shader writes, so the sRGB values are decoded to linear first.
    fn channels(&self, color: Color) -> [f32; 4] {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if !self.srgb_target {
                value
            } else if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
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
