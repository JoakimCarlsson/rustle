// A rendered layer drawn onto its parent, turned about a pivot, scaled and faded.
// The layer's pixels are premultiplied, so fading scales all four channels.

struct Params {
    size: vec2<f32>,
    pivot: vec2<f32>,
    rotation: vec2<f32>,
    scale: f32,
    opacity: f32,
}

@group(0) @binding(0) var layer: texture_2d<f32>;
@group(0) @binding(1) var layer_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct Fragment {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
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
    let corner = unit_corner(index);
    let relative = (corner * params.size - params.pivot) * params.scale;
    let turned = vec2<f32>(
        relative.x * params.rotation.x - relative.y * params.rotation.y,
        relative.x * params.rotation.y + relative.y * params.rotation.x,
    );
    let point = params.pivot + turned;

    var out: Fragment;
    out.position = vec4<f32>(
        point.x / params.size.x * 2.0 - 1.0,
        1.0 - point.y / params.size.y * 2.0,
        0.0,
        1.0,
    );
    out.uv = corner;
    return out;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    return textureSample(layer, layer_sampler, in.uv) * params.opacity;
}
