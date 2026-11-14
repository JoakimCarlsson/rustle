use std::sync::Arc;

use crate::color::Color;

/// How a layer's pixels combine with what is already drawn below it.
///
/// `SrcOver` is ordinary painting. The others follow the Porter-Duff and
/// separable blend operators Compose names: for the ones that multiply or
/// compare colors, what is below is taken as opaque.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// The layer over what is below: the usual way of drawing.
    #[default]
    SrcOver,
    /// What is below over the layer.
    DstOver,
    /// Only the layer, replacing what is below inside its area.
    Src,
    /// Nothing: erases what is below.
    Clear,
    /// The layer, where there is something below.
    SrcIn,
    /// The layer, where there is nothing below.
    SrcOut,
    /// The layer over what is below, but only where there is something below.
    SrcAtop,
    /// What is below, where the layer is.
    DstIn,
    /// What is below, where the layer is not: cuts the layer's shape out.
    DstOut,
    /// The layer and what is below, each only where the other is not.
    Xor,
    /// The two added, brightening up to white.
    Plus,
    /// The two multiplied, darkening.
    Multiply,
    /// The inverse of multiplying the inverses, brightening.
    Screen,
    /// The darker of each channel.
    Darken,
    /// The lighter of each channel.
    Lighten,
}

/// A change of every color a layer draws, applied to the layer as a whole.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorFilter {
    /// Four rows of five numbers: the factors of red, green, blue and alpha and
    /// an offset (0 to 1), that make the new red, green, blue and alpha.
    Matrix([f32; 20]),
    /// Every pixel takes `color`'s red, green and blue, and keeps its own shape
    /// (its alpha times the color's).
    Tint(Color),
}

impl ColorFilter {
    /// The filter that leaves colors as they are.
    pub const IDENTITY: Self = Self::Matrix([
        1.0, 0.0, 0.0, 0.0, 0.0, //
        0.0, 1.0, 0.0, 0.0, 0.0, //
        0.0, 0.0, 1.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, 1.0, 0.0,
    ]);

    /// Colors drained toward grey: 1 leaves them, 0 is fully grey, above 1 is more vivid.
    pub fn saturation(amount: f32) -> Self {
        let (r, g, b) = (0.2126, 0.7152, 0.0722);
        let s = amount;
        Self::Matrix([
            r + (1.0 - r) * s,
            g - g * s,
            b - b * s,
            0.0,
            0.0,
            r - r * s,
            g + (1.0 - g) * s,
            b - b * s,
            0.0,
            0.0,
            r - r * s,
            g - g * s,
            b + (1.0 - b) * s,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
        ])
    }

    /// Brightness added to every channel: 0 leaves colors, 1 is white, -1 is black.
    pub fn brightness(amount: f32) -> Self {
        Self::Matrix([
            1.0, 0.0, 0.0, 0.0, amount, //
            0.0, 1.0, 0.0, 0.0, amount, //
            0.0, 0.0, 1.0, 0.0, amount, //
            0.0, 0.0, 0.0, 1.0, 0.0,
        ])
    }

    /// Contrast about mid grey: 1 leaves colors, 0 is flat grey, above 1 is harsher.
    pub fn contrast(amount: f32) -> Self {
        let offset = 0.5 - 0.5 * amount;
        Self::Matrix([
            amount, 0.0, 0.0, 0.0, offset, //
            0.0, amount, 0.0, 0.0, offset, //
            0.0, 0.0, amount, 0.0, offset, //
            0.0, 0.0, 0.0, 1.0, 0.0,
        ])
    }

    /// This filter as the four rows of five numbers of [`ColorFilter::Matrix`],
    /// in sRGB channel values from 0 to 1.
    pub fn matrix(&self) -> [f32; 20] {
        match self {
            Self::Matrix(matrix) => *matrix,
            Self::Tint(color) => {
                let channel = |value: u8| f32::from(value) / 255.0;
                [
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    channel(color.r),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    channel(color.g),
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    channel(color.b),
                    0.0,
                    0.0,
                    0.0,
                    channel(color.a),
                    0.0,
                ]
            }
        }
    }
}

/// What is done to a layer as it is put on what is below, between
/// [`DrawCommand::PushEffect`](crate::DrawCommand::PushEffect) and `PopEffect`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerEffect {
    /// The standard deviation of a Gaussian blur of the layer, in logical pixels;
    /// 0 is none. A blurred layer is cut to the effect's rectangle.
    pub blur: f32,
    /// A change of the layer's colors, applied after the blur.
    pub color_filter: Option<ColorFilter>,
    /// How the layer combines with what is below.
    pub blend_mode: BlendMode,
    /// From 0 (invisible) to 1 (as is).
    pub alpha: f32,
}

impl Default for LayerEffect {
    fn default() -> Self {
        Self {
            blur: 0.0,
            color_filter: None,
            blend_mode: BlendMode::SrcOver,
            alpha: 1.0,
        }
    }
}

/// A fill computed by a fragment shader the program supplies, as WGSL.
///
/// The source holds a function `fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32>`
/// that returns the color (straight alpha) at a point: `uv` runs from (0, 0) at the
/// top-left of the filled rectangle to (1, 1) at the bottom-right, and `position` is the
/// same point in logical pixels from the top-left. The source may use the global
/// `uniforms`, with `uniforms.time` (seconds, as set by [`ShaderBrush::with_time`]),
/// `uniforms.size` (the rectangle's width and height in logical pixels) and
/// `uniforms.params`, an `array<vec4<f32>, 4>` that holds the floats given with
/// [`ShaderBrush::with_params`] in order (`uniforms.params[0].x` is the first). It may
/// define other functions and constants, but not bindings or entry points.
/// Renderers keep the compiled shader for as long as the same source is drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderBrush {
    source: Arc<str>,
    params: Vec<f32>,
    time: f32,
}

impl ShaderBrush {
    /// The most floats a shader can be given.
    pub const MAX_PARAMS: usize = 16;

    /// A brush that runs the WGSL `source`, which defines `shade`.
    pub fn new(source: impl Into<Arc<str>>) -> Self {
        Self {
            source: source.into(),
            params: Vec::new(),
            time: 0.0,
        }
    }

    /// This brush with `params` as the floats its shader reads from `uniforms.params`.
    /// At most [`ShaderBrush::MAX_PARAMS`] are used.
    pub fn with_params(mut self, params: &[f32]) -> Self {
        self.params = params.iter().copied().take(Self::MAX_PARAMS).collect();
        self
    }

    /// This brush with `seconds` as the `uniforms.time` of its shader. Set it from a
    /// value that changes every frame to animate the shader.
    pub fn with_time(mut self, seconds: f32) -> Self {
        self.time = seconds;
        self
    }

    /// The WGSL source.
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }

    /// The floats the shader reads from `uniforms.params`.
    pub fn params(&self) -> &[f32] {
        &self.params
    }

    /// The value of `uniforms.time`, in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }
}
