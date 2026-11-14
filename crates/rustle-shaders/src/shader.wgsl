// The frame around a fragment shader the program supplies: a rounded rectangle,
// one quad each, whose color comes from `shade`, appended after this source.

struct Viewport {
    size: vec2<f32>,
    scale: f32,
    padding: f32,
    origin: vec2<f32>,
}

struct Uniforms {
    rect: vec4<f32>,
    size: vec2<f32>,
    time: f32,
    corner: f32,
    flags: vec4<f32>,
    params: array<vec4<f32>, 4>,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var<uniform> uniforms: Uniforms;

struct Fragment {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
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
fn vertex(@builtin(vertex_index) index: u32) -> Fragment {
    let margin = 1.0;
    let local = unit_corner(index) * (uniforms.size + vec2<f32>(margin * 2.0)) - vec2<f32>(margin);
    let physical = (uniforms.rect.xy + local) * viewport.scale - viewport.origin;
    var out: Fragment;
    out.position = vec4<f32>(
        physical.x / viewport.size.x * 2.0 - 1.0,
        1.0 - physical.y / viewport.size.y * 2.0,
        0.0,
        1.0,
    );
    out.local = local;
    return out;
}

fn rustle_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    let half_size = uniforms.size * 0.5;
    let centered = in.local - half_size;
    let radius = min(uniforms.corner, min(half_size.x, half_size.y));
    let inner = abs(centered) - half_size + vec2<f32>(radius);
    let distance = length(max(inner, vec2<f32>(0.0))) + min(max(inner.x, inner.y), 0.0) - radius;
    let edge = 0.5 / viewport.scale;
    let coverage = 1.0 - smoothstep(-edge, edge, distance);
    let uv = in.local / max(uniforms.size, vec2<f32>(0.0001));
    var color = shade(uv, in.local);
    if uniforms.flags.x > 0.5 {
        color = vec4<f32>(
            rustle_to_linear(color.r),
            rustle_to_linear(color.g),
            rustle_to_linear(color.b),
            color.a,
        );
    }
    return vec4<f32>(color.rgb * color.a, color.a) * coverage;
}

