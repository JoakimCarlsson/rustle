// One direction of a Gaussian blur of a rectangle of a texture, drawn as a
// full-target quad into the top-left corner of the destination. Run once along
// x and once along y. Taps are `step` texels apart, so a large deviation costs
// no more taps than a small one. Taps outside the rectangle read its nearest edge.

struct Params {
    direction: vec2<f32>,
    sigma: f32,
    step: f32,
    taps: vec4<f32>,
    // The rectangle read, as left and top in texels, then width and height.
    area: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct Fragment {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vertex(@builtin(vertex_index) index: u32) -> Fragment {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let corner = corners[index];
    var out: Fragment;
    out.position = vec4<f32>(corner.x * 2.0 - 1.0, 1.0 - corner.y * 2.0, 0.0, 1.0);
    out.uv = corner;
    return out;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    let dimensions = vec2<f32>(textureDimensions(source));
    let low = params.area.xy + vec2<f32>(0.5);
    let high = params.area.xy + params.area.zw - vec2<f32>(0.5);
    let start = params.area.xy + in.position.xy;
    let reach = i32(params.taps.x);
    let spread = params.sigma / params.step;
    var total = vec4<f32>(0.0);
    var weight_sum = 0.0;
    for (var tap = -reach; tap <= reach; tap = tap + 1) {
        let offset = f32(tap);
        let weight = exp(-(offset * offset) / (2.0 * spread * spread));
        let point = clamp(start + params.direction * offset * params.step, low, high);
        total = total + textureSampleLevel(source, source_sampler, point / dimensions, 0.0) * weight;
        weight_sum = weight_sum + weight;
    }
    return total / weight_sum;
}
