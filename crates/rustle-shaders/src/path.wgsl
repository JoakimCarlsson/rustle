// Filled vector outlines: one instanced quad each, in physical pixels, sampling a
// coverage mask rasterised on the CPU at the size and subpixel offset it is
// drawn at, so every texel lands on exactly one pixel of the target.

struct Viewport {
    size: vec2<f32>,
    scale: f32,
    padding: f32,
    origin: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var mask: texture_2d<f32>;
@group(1) @binding(1) var mask_sampler: sampler;

struct Instance {
    @location(0) origin: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct Fragment {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

fn unit_corner(index: u32) -> vec2<f32> {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    return corners[index];
}

@vertex
fn vertex(@builtin(vertex_index) index: u32, instance: Instance) -> Fragment {
    let corner = unit_corner(index);
    let point = instance.origin + corner * instance.size - viewport.origin;
    var out: Fragment;
    out.position = vec4<f32>(
        point.x / viewport.size.x * 2.0 - 1.0,
        1.0 - point.y / viewport.size.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = corner;
    out.color = instance.color;
    return out;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    let coverage = textureSample(mask, mask_sampler, in.uv).r;
    let alpha = in.color.a * coverage;
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
