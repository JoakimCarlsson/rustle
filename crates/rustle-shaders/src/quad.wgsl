// Rounded, bordered, optionally blurred rectangles: one instanced quad each,
// shaped by a signed distance field so corners, borders and soft edges
// antialias without geometry.

struct Viewport {
    size: vec2<f32>,
    scale: f32,
    padding: f32,
    origin: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) fill: vec4<f32>,
    @location(2) border_color: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) matrix: vec4<f32>,
    @location(5) params: vec4<f32>,
    @location(6) arc: vec4<f32>,
    @location(7) extra: vec4<f32>,
}

struct Fragment {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) half_size: vec2<f32>,
    @location(2) fill: vec4<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) radii: vec4<f32>,
    @location(5) params: vec4<f32>,
    @location(6) arc: vec4<f32>,
    @location(7) extra: vec4<f32>,
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

// The radius of the corner `point` lies in, with the radii running clockwise
// from the top-left.
fn corner_radius(radii: vec4<f32>, point: vec2<f32>) -> f32 {
    let top = select(radii.x, radii.y, point.x > 0.0);
    let bottom = select(radii.w, radii.z, point.x > 0.0);
    return select(top, bottom, point.y > 0.0);
}

fn rounded_box(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let inner = abs(point) - half_size + vec2<f32>(radius);
    return length(max(inner, vec2<f32>(0.0))) + min(max(inner.x, inner.y), 0.0) - radius;
}

// The distance from `point` to a ring of radius `ring` and thickness
// `thickness` that covers `sweep` of the way round, clockwise from the top,
// with round ends: negative inside.
fn arc_distance(point: vec2<f32>, ring: f32, thickness: f32, sweep: f32) -> f32 {
    let half_angle = sweep * 3.14159265;
    let middle = vec2<f32>(sin(half_angle), -cos(half_angle));
    let across = vec2<f32>(-middle.y, middle.x);
    var turned = vec2<f32>(dot(point, across), dot(point, middle));
    turned.x = abs(turned.x);
    let ends = vec2<f32>(sin(half_angle), cos(half_angle));
    let to_ring = select(
        abs(length(turned) - ring),
        length(turned - ends * ring),
        ends.y * turned.x > ends.x * turned.y,
    );
    return to_ring - thickness * 0.5;
}

// The error function of both components, approximated closely enough for a blur.
fn erf(value: vec2<f32>) -> vec2<f32> {
    let sign_value = sign(value);
    let magnitude = abs(value);
    var denominator = 1.0 + (0.278393 + (0.230389 + 0.078108 * (magnitude * magnitude)) * magnitude) * magnitude;
    denominator = denominator * denominator;
    return sign_value - sign_value / (denominator * denominator);
}

fn gaussian(value: f32, sigma: f32) -> f32 {
    return exp(-(value * value) / (2.0 * sigma * sigma)) / (2.50662827 * sigma);
}

// How much of a Gaussian of deviation `sigma` centred on the row at `y`
// falls on a rounded box, integrated along the row.
fn blur_along_x(x: f32, y: f32, sigma: f32, corner: f32, half_size: vec2<f32>) -> f32 {
    let delta = min(half_size.y - corner - abs(y), 0.0);
    let curved = half_size.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let integral = 0.5 + 0.5 * erf((x + vec2<f32>(-curved, curved)) * (0.70710678 / sigma));
    return integral.y - integral.x;
}

// A rounded box blurred by a Gaussian of deviation `sigma`, 0 to 1.
fn blurred_box(point: vec2<f32>, half_size: vec2<f32>, corner: f32, sigma: f32) -> f32 {
    let low = point.y - half_size.y;
    let high = point.y + half_size.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let step = (end - start) / 4.0;
    var y = start + step * 0.5;
    var total = 0.0;
    for (var index = 0; index < 4; index = index + 1) {
        total = total + blur_along_x(point.x, point.y - y, sigma, corner, half_size) * gaussian(y, sigma) * step;
        y = y + step;
    }
    return clamp(total, 0.0, 1.0);
}

@vertex
fn vertex(@builtin(vertex_index) index: u32, instance: Instance) -> Fragment {
    let softness = instance.params.w;
    let margin = max(softness * 2.0, 1.0);
    let size = instance.rect.zw;
    let corner = unit_corner(index);
    let local = (corner - vec2<f32>(0.5)) * (size + vec2<f32>(margin * 2.0));
    let centre = instance.rect.xy + size * 0.5;
    let point = centre + local;
    let moved = vec2<f32>(
        instance.matrix.x * point.x + instance.matrix.z * point.y + instance.params.x,
        instance.matrix.y * point.x + instance.matrix.w * point.y + instance.params.y,
    );
    let physical = moved * viewport.scale - viewport.origin;

    var out: Fragment;
    out.position = vec4<f32>(
        physical.x / viewport.size.x * 2.0 - 1.0,
        1.0 - physical.y / viewport.size.y * 2.0,
        0.0,
        1.0,
    );
    out.local = local;
    out.half_size = size * 0.5;
    out.fill = instance.fill;
    out.border_color = instance.border_color;
    out.radii = instance.radii;
    out.params = instance.params;
    out.arc = instance.arc;
    out.extra = instance.extra;
    return out;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    if in.arc.y > 0.0 {
        let ring = min(in.half_size.x, in.half_size.y) - in.arc.y * 0.5;
        // The ring starts `arc.z` radians clockwise from the top.
        let turn = cos(in.arc.z);
        let lean = sin(in.arc.z);
        let turned = vec2<f32>(
            turn * in.local.x + lean * in.local.y,
            turn * in.local.y - lean * in.local.x,
        );
        let distance = arc_distance(turned, ring, in.arc.y, in.arc.x);
        let edge = 0.5 / viewport.scale;
        let coverage = 1.0 - smoothstep(-edge, edge, distance);
        return vec4<f32>(in.fill.rgb * in.fill.a, in.fill.a) * coverage;
    }
    let limit = min(in.half_size.x, in.half_size.y);
    let radius = min(corner_radius(in.radii, in.local), limit);
    let distance = rounded_box(in.local, in.half_size, radius);
    let edge = 0.5 / viewport.scale;
    let softness = in.params.w;
    var coverage = 1.0 - smoothstep(-edge, edge, distance);
    if softness > 0.0 {
        coverage = blurred_box(in.local, in.half_size, radius, softness * 0.5);
    }

    // A gradient blends from the fill color to the border color along an angle.
    var base = in.fill;
    if in.arc.w > 0.0 {
        let direction = vec2<f32>(cos(in.arc.z), sin(in.arc.z));
        let extent = abs(direction.x) * in.half_size.x + abs(direction.y) * in.half_size.y;
        var t = clamp(0.5 + dot(in.local, direction) / (2.0 * max(extent, 0.0001)), 0.0, 1.0);
        let from_center = in.local - (in.extra.xy - vec2<f32>(0.5)) * (in.half_size * 2.0);
        if in.extra.w > 1.5 {
            let turns = (atan2(from_center.y, from_center.x) - in.arc.z) / 6.28318531;
            t = turns - floor(turns);
        } else if in.extra.w > 0.5 {
            t = clamp(length(from_center) / max(in.extra.z, 0.0001), 0.0, 1.0);
        }
        base = mix(in.fill, in.border_color, t);
    }
    let fill = vec4<f32>(base.rgb * base.a, base.a);
    let border = vec4<f32>(in.border_color.rgb * in.border_color.a, in.border_color.a);
    var colour = fill;
    let border_width = in.params.z;
    if border_width > 0.0 {
        let inside = 1.0 - smoothstep(-edge, edge, distance + border_width);
        colour = mix(border, fill, inside);
    }
    return colour * coverage;
}
