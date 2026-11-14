//! Fragment shaders the program supplies, compiled once per source and drawn as quads.

use crate::quad::Viewport;
use std::collections::HashMap;
use std::sync::Arc;

use rustle_graphics::ShaderBrush;
use wgpu::util::DeviceExt;

/// What the shader frame reads, matching `Uniforms` in `shader.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    /// Left, top, width and height in logical pixels.
    rect: [f32; 4],
    size: [f32; 2],
    time: f32,
    corner: f32,
    /// The first is 1 when the target decodes sRGB, so the shader's colors are linearized.
    flags: [f32; 4],
    params: [[f32; 4]; 4],
}

/// One rounded rectangle filled by a program's shader, in logical pixels.
pub(crate) struct Call {
    pub(crate) rect: [f32; 4],
    pub(crate) corner_radius: f32,
    pub(crate) brush: ShaderBrush,
}

/// A shader draw ready to record: which pipeline, and its uniforms.
pub(crate) struct Batch {
    source: Arc<str>,
    group: wgpu::BindGroup,
}

pub(crate) struct Shaders {
    /// Compiled pipelines by source; `None` for a source that did not compile.
    pipelines: HashMap<Arc<str>, Option<wgpu::RenderPipeline>>,
    layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
}

impl Shaders {
    /// A cache of shaders for targets of `format`, drawn with the viewport group of `viewport_layout`.
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shader"),
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shader"),
            bind_group_layouts: &[Some(viewport_layout), Some(&layout)],
            immediate_size: 0,
        });
        Self {
            pipelines: HashMap::new(),
            layout,
            pipeline_layout,
            format,
        }
    }

    /// Compiles `source`, or returns `None` and reports why if the device rejects it.
    fn compile(&self, device: &wgpu::Device, source: &str) -> Option<wgpu::RenderPipeline> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("custom shader"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::custom(source).into()),
        });
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("custom shader"),
            layout: Some(&self.pipeline_layout),
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
                    format: self.format,
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
        match pollster::block_on(scope.pop()) {
            Some(error) => {
                eprintln!("rustle: a custom shader was rejected and is not drawn: {error}");
                None
            }
            None => Some(pipeline),
        }
    }

    /// Makes the uniforms for `call` and the pipeline for its source, compiling it the first time.
    /// Returns `None` if the source does not compile.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device, call: &Call) -> Option<Batch> {
        let source = call.brush.source();
        if !self.pipelines.contains_key(source) {
            let pipeline = self.compile(device, source);
            self.pipelines.insert(Arc::clone(source), pipeline);
        }
        self.pipelines.get(source)?.as_ref()?;
        let mut params = [[0.0; 4]; 4];
        for (index, value) in call.brush.params().iter().enumerate() {
            params[index / 4][index % 4] = *value;
        }
        let srgb = self.format.is_srgb();
        let uniforms = Uniforms {
            rect: call.rect,
            size: [call.rect[2], call.rect[3]],
            time: call.brush.time(),
            corner: call.corner_radius,
            flags: [f32::from(u8::from(srgb)), 0.0, 0.0, 0.0],
            params,
        };
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shader"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shader"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        Some(Batch {
            source: Arc::clone(source),
            group,
        })
    }

    /// Draws `batch` onto the pass, with the frame's viewport as group 0.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, viewport: &Viewport, batch: &Batch) {
        let Some(Some(pipeline)) = self.pipelines.get(&batch.source) else {
            return;
        };
        pass.set_pipeline(pipeline);
        viewport.set(pass);
        pass.set_bind_group(1, &batch.group, &[]);
        pass.draw(0..6, 0..1);
    }
}
