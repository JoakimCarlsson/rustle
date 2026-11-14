// A rendered layer drawn onto its parent, turned about a pivot, scaled and faded.
// The layer's pixels are premultiplied, so fading scales all four channels.

struct Params {
    // The texture drawn into: its top-left corner in target pixels, then its size.
    dest: vec4<f32>,
    // The texture drawn from, the same way.
    source: vec4<f32>,
    // The rectangle of the target covered, as left, top, width and height.
    area: vec4<f32>,
    pivot: vec2<f32>,
    rotation: vec2<f32>,
    scale: f32,
    opacity: f32,
    mask_rect: vec4<f32>,
    mask_radii: vec4<f32>,
    masked: vec4<f32>,
    filter_rows: array<vec4<f32>, 4>,
    filter_offset: vec4<f32>,
}

@group(0) @binding(0)
var layer: texture_2d<f32>;
@group(0) @binding(1)
var layer_sampler: sampler;
@group(0) @binding(2)
var<uniform> params: Params;

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
    let world = params.area.xy + corner * params.area.zw;
    let relative = (world - params.pivot) * params.scale;
    let turned = vec2<f32>(
            relative.x * params.rotation.x - relative.y * params.rotation.y,
            relative.x * params.rotation.y + relative.y * params.rotation.x,
        );
    let point = params.pivot + turned - params.dest.xy;

    var out: Fragment;
    out.position = vec4<f32>(
            point.x / params.dest.z * 2.0 - 1.0,
            1.0 - point.y / params.dest.w * 2.0,
            0.0,
            1.0,
        );
    out.uv = (world - params.source.xy) / params.source.zw;
    return out;
}

// Distance from `point` to the edge of a rounded rectangle, negative inside.
fn rounded_distance(point: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let right = point.x > 0.0;
    let below = point.y > 0.0;
    let top = select(radii.x, radii.y, right);
    let bottom = select(radii.w, radii.z, right);
    let corner = select(top, bottom, below);
    let reach = abs(point) - half_size + vec2<f32>(corner);
    return length(max(reach, vec2<f32>(0.0))) + min(max(reach.x, reach.y), 0.0) - corner;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    var coverage = 1.0;
    if params.masked.x > 0.5 {
        let half_size = params.mask_rect.zw * 0.5;
        // The radii cannot exceed what fits, or the corners would overlap.
        let limit = min(half_size.x, half_size.y);
        let radii = min(params.mask_radii, vec4<f32>(limit));
        let center = params.mask_rect.xy + half_size;
        let distance = rounded_distance(in.position.xy + params.dest.xy - center, half_size, radii);
        coverage = clamp(0.5 - distance, 0.0, 1.0);
    }
    var color = textureSample(layer, layer_sampler, in.uv);
    if params.masked.y > 0.5 {
        let straight = vec4<f32>(color.rgb / max(color.a, 0.0001), color.a);
        let filtered = clamp(
            vec4<f32>(
                dot(params.filter_rows[0], straight),
                dot(params.filter_rows[1], straight),
                dot(params.filter_rows[2], straight),
                dot(params.filter_rows[3], straight),
            ) + params.filter_offset,
            vec4<f32>(0.0),
            vec4<f32>(1.0),
        );
        color = vec4<f32>(filtered.rgb * filtered.a, filtered.a);
    }
    return color * (params.opacity * coverage);
}
