//! Filled vector outlines, tessellated into triangles and drawn by the shader in `rustle-shaders`.
//!
//! The renderer has no multisampling, so the edge of a shape is antialiased
//! with a thin fringe of triangles around it that fades out. The fringe is
//! found from the mesh itself: an edge that only one triangle has is on the
//! boundary, and the side that triangle is not on is outside.

use crate::quad::Viewport;
use std::collections::HashMap;
use std::rc::Rc;

use lyon_tessellation::geom::point;
use lyon_tessellation::path::Path as LyonPath;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonFillRule, FillTessellator, FillVertex,
    VertexBuffers,
};
use rustle_graphics::{Color, DrawCommand, FillRule, Path, PathCommand};
use wgpu::util::DeviceExt;

use crate::srgb_to_linear;

/// How far outside the shape the fringe reaches, in physical pixels.
const FEATHER: f32 = 0.8;

/// One corner of a triangle as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Vertex {
    /// In logical pixels.
    position: [f32; 2],
    /// 1 inside the shape, fading to 0 at the outer edge of the fringe.
    coverage: f32,
    /// Linear on an sRGB target, with straight alpha.
    color: [f32; 4],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32,
        2 => Float32x4,
    ];
}

/// The triangles of the outlines of one layer, before they are uploaded.
#[derive(Default)]
pub(crate) struct Geometry {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
}

impl Geometry {
    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
}

/// The triangles of one outline, relative to the rectangle it is drawn into.
struct Mesh {
    /// Position and coverage.
    vertices: Vec<([f32; 2], f32)>,
    indices: Vec<u32>,
}

/// The outlines of one layer, uploaded and ready to draw.
pub(crate) struct Batch {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
}

/// Identifies a mesh: the constant data of the path, the size it is drawn at,
/// and the scale of the window, which sets how fine the curves are cut.
type Key = (usize, usize, u32, u32, u32);

pub(crate) struct Paths {
    pipeline: wgpu::RenderPipeline,
    srgb_target: bool,
    /// Meshes of constant paths kept between frames, with the frame they were last used in.
    meshes: HashMap<Key, (Rc<Mesh>, u64)>,
    tessellator: FillTessellator,
    frame: u64,
}

impl Paths {
    pub(crate) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        viewport_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("path"),
            source: wgpu::ShaderSource::Wgsl(rustle_shaders::PATH.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("path"),
            bind_group_layouts: &[Some(viewport_layout)],
            immediate_size: 0,
        });
        // The shader outputs premultiplied alpha.
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("path"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vertex"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &Vertex::ATTRIBUTES,
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
            srgb_target: format.is_srgb(),
            meshes: HashMap::new(),
            tessellator: FillTessellator::new(),
            frame: 0,
        }
    }

    /// Adds the triangles that draw `command`, if it is an outline, to `geometry`.
    pub(crate) fn push(&mut self, geometry: &mut Geometry, command: &DrawCommand, scale: f32) {
        let DrawCommand::Path { path, rect, color } = command else {
            return;
        };
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let size = (rect.size.width, rect.size.height);
        let mesh = match path.static_key() {
            Some((address, length)) => {
                let key = (
                    address,
                    length,
                    size.0.to_bits(),
                    size.1.to_bits(),
                    scale.to_bits(),
                );
                if let Some((mesh, used)) = self.meshes.get_mut(&key) {
                    *used = self.frame;
                    Rc::clone(mesh)
                } else {
                    let mesh = Rc::new(self.tessellate(path, size, scale));
                    self.meshes.insert(key, (Rc::clone(&mesh), self.frame));
                    mesh
                }
            }
            None => Rc::new(self.tessellate(path, size, scale)),
        };
        let color = self.channels(*color);
        let base = geometry.vertices.len() as u32;
        geometry
            .vertices
            .extend(mesh.vertices.iter().map(|(position, coverage)| Vertex {
                position: [position[0] + rect.origin.x, position[1] + rect.origin.y],
                coverage: *coverage,
                color,
            }));
        geometry
            .indices
            .extend(mesh.indices.iter().map(|index| base + index));
    }

    /// Forgets meshes nothing has drawn for a while. Called once a frame.
    pub(crate) fn end_frame(&mut self) {
        self.frame += 1;
        let frame = self.frame;
        self.meshes.retain(|_, (_, used)| frame - *used < 240);
    }

    /// The triangles of `path` stretched over a box of `size` logical pixels, with the fringe.
    fn tessellate(&mut self, path: &Path, size: (f32, f32), scale: f32) -> Mesh {
        let (width, height) = path.size();
        let (sx, sy) = (
            size.0 / width.max(f32::EPSILON),
            size.1 / height.max(f32::EPSILON),
        );
        let mut builder = LyonPath::builder();
        let (mut open, mut start) = (false, (0.0, 0.0));
        let at = |x: f32, y: f32| point(x * sx, y * sy);
        for command in path.commands() {
            // Drawing after a close begins a new subpath where the last one began.
            if !matches!(command, PathCommand::MoveTo(..) | PathCommand::Close) && !open {
                builder.begin(at(start.0, start.1));
                open = true;
            }
            match *command {
                PathCommand::MoveTo(x, y) => {
                    if open {
                        builder.end(false);
                    }
                    builder.begin(at(x, y));
                    (open, start) = (true, (x, y));
                }
                PathCommand::LineTo(x, y) => {
                    builder.line_to(at(x, y));
                }
                PathCommand::QuadTo(cx, cy, x, y) => {
                    builder.quadratic_bezier_to(at(cx, cy), at(x, y));
                }
                PathCommand::CubicTo(ax, ay, bx, by, x, y) => {
                    builder.cubic_bezier_to(at(ax, ay), at(bx, by), at(x, y));
                }
                PathCommand::Close => {
                    if open {
                        builder.end(true);
                        open = false;
                    }
                }
            }
        }
        if open {
            builder.end(false);
        }
        let lyon_path = builder.build();

        let mut buffers: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        let options =
            FillOptions::tolerance(0.1 / scale.max(0.1)).with_fill_rule(match path.fill_rule() {
                FillRule::NonZero => LyonFillRule::NonZero,
                FillRule::EvenOdd => LyonFillRule::EvenOdd,
            });
        let result = self.tessellator.tessellate_path(
            &lyon_path,
            &options,
            &mut BuffersBuilder::new(&mut buffers, |vertex: FillVertex| {
                vertex.position().to_array()
            }),
        );
        if result.is_err() {
            return Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
            };
        }
        feathered(buffers, FEATHER / scale.max(0.1))
    }

    /// Uploads `geometry`, or returns `None` if it is empty.
    pub(crate) fn upload(&self, device: &wgpu::Device, geometry: &Geometry) -> Option<Batch> {
        if geometry.is_empty() {
            return None;
        }
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("path vertices"),
            contents: bytemuck::cast_slice(&geometry.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("path indices"),
            contents: bytemuck::cast_slice(&geometry.indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Some(Batch {
            vertices,
            indices,
            count: geometry.indices.len() as u32,
        })
    }

    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>, viewport: &Viewport, batch: &Batch) {
        pass.set_pipeline(&self.pipeline);
        viewport.set(pass);
        pass.set_vertex_buffer(0, batch.vertices.slice(..));
        pass.set_index_buffer(batch.indices.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..batch.count, 0, 0..1);
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

/// `buffers` with a fringe `reach` wide added outside every boundary edge.
fn feathered(buffers: VertexBuffers<[f32; 2], u32>, reach: f32) -> Mesh {
    let VertexBuffers { vertices, indices } = buffers;
    // For every edge, how many triangles have it and the outward normal from the first.
    let mut edges: HashMap<(u32, u32), (u32, [f32; 2])> = HashMap::new();
    for triangle in indices.as_chunks::<3>().0 {
        for (from, to, other) in [
            (triangle[0], triangle[1], triangle[2]),
            (triangle[1], triangle[2], triangle[0]),
            (triangle[2], triangle[0], triangle[1]),
        ] {
            let (p, q, r) = (
                vertices[from as usize],
                vertices[to as usize],
                vertices[other as usize],
            );
            let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
            let length = (dx * dx + dy * dy).sqrt();
            if length <= f32::EPSILON {
                continue;
            }
            let mut normal = [dy / length, -dx / length];
            if normal[0] * (r[0] - p[0]) + normal[1] * (r[1] - p[1]) > 0.0 {
                normal = [-normal[0], -normal[1]];
            }
            let entry = edges
                .entry((from.min(to), from.max(to)))
                .or_insert((0, normal));
            entry.0 += 1;
        }
    }
    let boundary: Vec<((u32, u32), [f32; 2])> = edges
        .into_iter()
        .filter(|(_, (count, _))| *count == 1)
        .map(|(edge, (_, normal))| (edge, normal))
        .collect();

    // Each boundary vertex moves out along the average of the normals of its edges.
    let mut sums: HashMap<u32, ([f32; 2], u32)> = HashMap::new();
    for ((a, b), normal) in &boundary {
        for vertex in [a, b] {
            let entry = sums.entry(*vertex).or_insert(([0.0; 2], 0));
            entry.0[0] += normal[0];
            entry.0[1] += normal[1];
            entry.1 += 1;
        }
    }
    let mut mesh = Mesh {
        vertices: vertices.iter().map(|position| (*position, 1.0)).collect(),
        indices,
    };
    let mut outer: HashMap<u32, u32> = HashMap::new();
    for (vertex, (sum, count)) in sums {
        let length = (sum[0] * sum[0] + sum[1] * sum[1]).sqrt();
        if length <= 1e-4 {
            continue;
        }
        // Where edges meet at an angle the offset is longer, up to a limit.
        let miter = (length / count as f32).max(0.4);
        let push = reach / miter;
        let position = vertices[vertex as usize];
        outer.insert(vertex, mesh.vertices.len() as u32);
        mesh.vertices.push((
            [
                position[0] + sum[0] / length * push,
                position[1] + sum[1] / length * push,
            ],
            0.0,
        ));
    }
    for ((a, b), _) in boundary {
        if let (Some(&outer_a), Some(&outer_b)) = (outer.get(&a), outer.get(&b)) {
            mesh.indices.extend([a, b, outer_b, a, outer_b, outer_a]);
        }
    }
    mesh
}
