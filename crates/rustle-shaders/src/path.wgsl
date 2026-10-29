// Tessellated vector outlines: triangles in logical pixels, each vertex
// carrying a color and how much of the pixel it covers, which is 1 inside the
// shape and fades to 0 across a thin fringe outside it to antialias the edge.

struct Viewport {
    size: vec2<f32>,
    scale: f32,
    padding: f32,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;

struct Vertex {
    @location(0) position: vec2<f32>,
    @location(1) coverage: f32,
    @location(2) color: vec4<f32>,
}

struct Fragment {
    @builtin(position) position: vec4<f32>,
    @location(0) coverage: f32,
    @location(1) color: vec4<f32>,
}

@vertex
fn vertex(vertex: Vertex) -> Fragment {
    let physical = vertex.position * viewport.scale;
    var out: Fragment;
    out.position = vec4<f32>(
        physical.x / viewport.size.x * 2.0 - 1.0,
        1.0 - physical.y / viewport.size.y * 2.0,
        0.0,
        1.0,
    );
    out.coverage = vertex.coverage;
    out.color = vertex.color;
    return out;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    let alpha = in.color.a * in.coverage;
    return vec4<f32>(in.color.rgb * alpha, alpha);
}
