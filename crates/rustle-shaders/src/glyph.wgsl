// Glyphs: one instanced quad each, placed on whole physical pixels and sampled texel for
// texel from an atlas of coverage masks (tinted with the glyph's color) or of color glyphs
// (drawn as they are), such as emoji.

struct Viewport {
    size: vec2<f32>,
    scale: f32,
    padding: f32,
    origin: vec2<f32>,
}

@group(0) @binding(0) var<uniform> viewport: Viewport;
@group(1) @binding(0) var color_atlas: texture_2d<f32>;
@group(1) @binding(1) var mask_atlas: texture_2d<f32>;
@group(1) @binding(2) var atlas_sampler: sampler;

struct Instance {
    // The top-left corner in the target's physical pixels.
    @location(0) position: vec2<i32>,
    // Width in the low 16 bits, height in the high.
    @location(1) size: u32,
    // The top-left texel in the atlas: x in the low 16 bits, y in the high.
    @location(2) texel: u32,
    // Straight-alpha color as 0xAARRGGBB, in sRGB-encoded channels.
    @location(3) color: u32,
    // Which atlas in the low 16 bits (0 color, 1 mask); whether to decode the color to
    // linear light, for a target that encodes to sRGB, in the high.
    @location(4) kind: u32,
}

struct Fragment {
    @invariant @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) kind: u32,
}

fn decode(channel: f32) -> f32 {
    if channel <= 0.04045 {
        return channel / 12.92;
    }
    return pow((channel + 0.055) / 1.055, 2.4);
}

@vertex
fn vertex(@builtin(vertex_index) index: u32, instance: Instance) -> Fragment {
    let corner = vec2<u32>(index & 1u, (index >> 1u) & 1u);
    let size = vec2<u32>(instance.size & 0xffffu, instance.size >> 16u);
    let offset = size * corner;
    let texel = vec2<u32>(instance.texel & 0xffffu, instance.texel >> 16u) + offset;
    let point = vec2<f32>(instance.position + vec2<i32>(offset)) - viewport.origin;

    var out: Fragment;
    out.position = vec4<f32>(
        point.x / viewport.size.x * 2.0 - 1.0,
        1.0 - point.y / viewport.size.y * 2.0,
        0.0,
        1.0,
    );
    let kind = instance.kind & 0xffffu;
    var color = vec4<f32>(
        f32((instance.color >> 16u) & 0xffu) / 255.0,
        f32((instance.color >> 8u) & 0xffu) / 255.0,
        f32(instance.color & 0xffu) / 255.0,
        f32(instance.color >> 24u) / 255.0,
    );
    if (instance.kind >> 16u) == 1u {
        color = vec4<f32>(decode(color.r), decode(color.g), decode(color.b), color.a);
    }
    out.color = color;
    var dimensions = textureDimensions(mask_atlas);
    if kind == 0u {
        dimensions = textureDimensions(color_atlas);
    }
    out.uv = vec2<f32>(texel) / vec2<f32>(dimensions);
    out.kind = kind;
    return out;
}

@fragment
fn fragment(in: Fragment) -> @location(0) vec4<f32> {
    if in.kind == 0u {
        return textureSampleLevel(color_atlas, atlas_sampler, in.uv, 0.0);
    }
    let coverage = textureSampleLevel(mask_atlas, atlas_sampler, in.uv, 0.0).x;
    return vec4<f32>(in.color.rgb, in.color.a * coverage);
}
