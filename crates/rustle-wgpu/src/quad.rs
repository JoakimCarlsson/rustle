//! Filled, rounded rectangles drawn as instanced quads by the shader in `rustle-shaders`.

use rustle_graphics::{Color, DrawCommand};

use crate::instances::InstanceBuffer;
use crate::srgb_to_linear;
use crate::uniforms::Arena;

/// The viewport the shader turns logical pixels into clip space with.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewportData {
    /// Target size in physical pixels.
    size: [f32; 2],
    /// Physical pixels per logical pixel.
    scale: f32,
    /// Padding to the 16-byte alignment a uniform block needs.
    padding: f32,
    /// Where the texture's top-left corner lies in the target's physical pixels.
    origin: [f32; 2],
    /// Pads to 32 bytes, which the binding of a viewport covers.
    reserved: [f32; 2],
}

/// One quad as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Instance {
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
    /// For gradients: the center as fractions of the rectangle, the radius of a radial
    /// gradient, and the kind (0 linear, 1 radial, 2 sweep).
    extra: [f32; 4],
}

impl Instance {
    const ATTRIBUTES: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
        0 => Float32x4,
        1 => Float32x4,
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4,
        7 => Float32x4,
    ];
}

/// The quads of one layer, staged in the frame's instance buffer and ready to draw.
pub(crate) struct Batch {
    /// Where the layer's instances lie in the instance buffer, in bytes.
    bytes: std::ops::Range<u64>,
}

/// Where a pipeline finds the viewport: a bind group for group 0, and the offset into it.
pub(crate) struct Viewport {
    group: wgpu::BindGroup,
    offset: u32,
}

impl Viewport {
    /// Sets the viewport as group 0 of the pass.
    pub(crate) fn set(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.group, &[self.offset]);
    }
}

pub(crate) struct Quads {
    pipeline: wgpu::RenderPipeline,
    viewport_layout: wgpu::BindGroupLayout,
    /// Where the viewports of the frame are written.
    arena: Arena,
    /// A bind group for each of the arena's buffers, made when first needed.
    viewport_groups: std::cell::RefCell<Vec<wgpu::BindGroup>>,
    /// Whether the target encodes linear values to sRGB on write.
    srgb_target: bool,
    /// The quads of the frame, every layer's in one buffer.
    instances: InstanceBuffer,
}
impl Quads {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let viewport_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
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
            viewport_layout,
            arena: Arena::new(device),
            viewport_groups: std::cell::RefCell::new(Vec::new()),
            srgb_target: format.is_srgb(),
            instances: InstanceBuffer::new("quads"),
        }
    }

    /// The layout of the viewport bind group, which other pipelines share.
    pub(crate) fn viewport_layout(&self) -> &wgpu::BindGroupLayout {
        &self.viewport_layout
    }

    /// A viewport for drawing into a texture of `size` physical pixels whose top-left corner
    /// is at `origin` in the target's, at `scale` physical pixels per logical pixel.
    pub(crate) fn viewport(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        origin: (u32, u32),
        size: (u32, u32),
        scale: f32,
    ) -> Viewport {
        let slot = self.arena.push(
            device,
            queue,
            bytemuck::bytes_of(&ViewportData {
                size: [size.0 as f32, size.1 as f32],
                scale,
                padding: 0.0,
                origin: [origin.0 as f32, origin.1 as f32],
                reserved: [0.0; 2],
            }),
        );
        let mut groups = self.viewport_groups.borrow_mut();
        while groups.len() <= slot.chunk {
            groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("viewport"),
                layout: &self.viewport_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &slot.buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<ViewportData>() as u64),
                    }),
                }],
            }));
        }
        Viewport {
            group: groups[slot.chunk].clone(),
            offset: slot.offset as u32,
        }
    }

    /// Starts a frame: the viewports and instances of the last one are written over.
    pub(crate) fn begin_frame(&mut self) {
        self.arena.begin_frame();
        self.instances.begin_frame();
    }

    /// Writes the instances staged this frame to the GPU; called before anything is drawn.
    pub(crate) fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.instances.flush(device, queue);
    }

    /// The quad that draws `command`, if it is a rectangle or a shadow.
    pub(crate) fn instance(&self, command: &DrawCommand) -> Option<Instance> {
        let (rect, fill, corner_radius, border, softness) = match command {
            DrawCommand::Rect {
                rect,
                color,
                corner_radius,
                border,
            } => (rect, *color, *corner_radius, *border, 0.0),
            DrawCommand::Shadow {
                rect,
                corner_radius,
                blur,
                color,
            } => (rect, *color, *corner_radius, None, *blur),
            DrawCommand::Line {
                from,
                to,
                width,
                color,
            } => {
                let (dx, dy) = (to.0 - from.0, to.1 - from.1);
                let length = (dx * dx + dy * dy).sqrt().max(0.001);
                let (centre_x, centre_y) = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
                // A rounded rectangle turned about its centre: the matrix turns it
                // about the origin, so the translation puts the centre back.
                let (cos, sin) = (dx / length, dy / length);
                let turned_x = cos * centre_x - sin * centre_y;
                let turned_y = sin * centre_x + cos * centre_y;
                return Some(Instance {
                    rect: [
                        centre_x - length / 2.0 - width / 2.0,
                        centre_y - width / 2.0,
                        length + width,
                        *width,
                    ],
                    fill: self.channels(*color),
                    border_color: [0.0; 4],
                    radii: [width / 2.0; 4],
                    matrix: [cos, sin, -sin, cos],
                    params: [centre_x - turned_x, centre_y - turned_y, 0.0, 0.0],
                    arc: [0.0; 4],
                    extra: [0.0; 4],
                });
            }
            DrawCommand::Arc {
                rect,
                color,
                thickness,
                start,
                sweep,
            } => {
                return Some(Instance {
                    rect: [
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    ],
                    fill: self.channels(*color),
                    border_color: [0.0; 4],
                    radii: [0.0; 4],
                    matrix: [1.0, 0.0, 0.0, 1.0],
                    params: [0.0; 4],
                    arc: [*sweep, *thickness, *start, 0.0],
                    extra: [0.0; 4],
                });
            }
            DrawCommand::Gradient {
                rect,
                from,
                to,
                angle,
                corner_radius,
            } => {
                return Some(Instance {
                    rect: [
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    ],
                    fill: self.channels(*from),
                    border_color: self.channels(*to),
                    radii: [*corner_radius; 4],
                    matrix: [1.0, 0.0, 0.0, 1.0],
                    params: [0.0; 4],
                    arc: [0.0, 0.0, *angle, 1.0],
                    extra: [0.0; 4],
                });
            }
            DrawCommand::RadialGradient {
                rect,
                center,
                radius,
                from,
                to,
                corner_radius,
            } => {
                return Some(Instance {
                    rect: [
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    ],
                    fill: self.channels(*from),
                    border_color: self.channels(*to),
                    radii: [*corner_radius; 4],
                    matrix: [1.0, 0.0, 0.0, 1.0],
                    params: [0.0; 4],
                    arc: [0.0, 0.0, 0.0, 1.0],
                    extra: [center.0, center.1, *radius, 1.0],
                });
            }
            DrawCommand::SweepGradient {
                rect,
                center,
                start,
                from,
                to,
                corner_radius,
            } => {
                return Some(Instance {
                    rect: [
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height,
                    ],
                    fill: self.channels(*from),
                    border_color: self.channels(*to),
                    radii: [*corner_radius; 4],
                    matrix: [1.0, 0.0, 0.0, 1.0],
                    params: [0.0; 4],
                    arc: [0.0, 0.0, *start, 1.0],
                    extra: [center.0, center.1, 0.0, 2.0],
                });
            }
            _ => return None,
        };
        let (border_color, border_width) = match border {
            Some(border) => (self.channels(border.color), border.width),
            None => ([0.0; 4], 0.0),
        };
        Some(Instance {
            rect: [
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
            ],
            fill: self.channels(fill),
            border_color,
            radii: [corner_radius; 4],
            matrix: [1.0, 0.0, 0.0, 1.0],
            params: [0.0, 0.0, border_width, softness],
            arc: [0.0; 4],
            extra: [0.0; 4],
        })
    }

    /// Stages `instances` in the frame's instance buffer, or returns `None` if there are none.
    pub(crate) fn upload(&mut self, instances: &[Instance]) -> Option<Batch> {
        if instances.is_empty() {
            return None;
        }
        Some(Batch {
            bytes: self.instances.push(bytemuck::cast_slice(instances)),
        })
    }

    /// Records drawing the quads of `batch` numbered `range` into `pass`.
    pub(crate) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        viewport: &Viewport,
        batch: &Batch,
        range: std::ops::Range<u32>,
    ) {
        let Some(slice) = self.instances.slice(batch.bytes.clone()) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        viewport.set(pass);
        pass.set_vertex_buffer(0, slice);
        pass.draw(0..6, range);
    }

    /// The color as the shader takes it. An sRGB target encodes what the
    /// shader writes, so the sRGB values are decoded to linear first.
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
