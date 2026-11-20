//! Colors and their maths, as Compose's `androidx.compose.ui.graphics.Color`
//! has them: an sRGB color with straight alpha, its channels as fractions,
//! `copy`, `compositeOver`, `luminance`, `toArgb`, the perceptual `lerp`, and
//! conversion to and from the color spaces of [`ColorSpaces`].

/// An sRGB color with straight alpha, eight bits a channel, as Compose keeps
/// a color in the sRGB space.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
pub struct Color {
    /// The red channel, sRGB-encoded.
    pub r: u8,
    /// The green channel, sRGB-encoded.
    pub g: u8,
    /// The blue channel, sRGB-encoded.
    pub b: u8,
    /// The opacity, 0 transparent to 255 opaque.
    pub a: u8,
}

impl Color {
    /// Fully transparent black, Compose's `Color.Transparent`.
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    /// Opaque black, Compose's `Color.Black`.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque dark gray `#444444`, Compose's `Color.DarkGray`.
    pub const DARK_GRAY: Self = Self::rgb(0x44, 0x44, 0x44);
    /// Opaque gray `#888888`, Compose's `Color.Gray`.
    pub const GRAY: Self = Self::rgb(0x88, 0x88, 0x88);
    /// Opaque light gray `#CCCCCC`, Compose's `Color.LightGray`.
    pub const LIGHT_GRAY: Self = Self::rgb(0xcc, 0xcc, 0xcc);
    /// Opaque white, Compose's `Color.White`.
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    /// Opaque red, Compose's `Color.Red`.
    pub const RED: Self = Self::rgb(255, 0, 0);
    /// Opaque green, Compose's `Color.Green`.
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    /// Opaque blue, Compose's `Color.Blue`.
    pub const BLUE: Self = Self::rgb(0, 0, 255);
    /// Opaque yellow, Compose's `Color.Yellow`.
    pub const YELLOW: Self = Self::rgb(255, 255, 0);
    /// Opaque cyan, Compose's `Color.Cyan`.
    pub const CYAN: Self = Self::rgb(0, 255, 255);
    /// Opaque magenta, Compose's `Color.Magenta`.
    pub const MAGENTA: Self = Self::rgb(255, 0, 255);

    /// An opaque color from 8-bit channels.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A color from 8-bit channels and opacity.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// A color from a packed `0xAARRGGBB` value, as Compose's `Color(0xFF3B82F6)`.
    pub const fn from_argb(argb: u32) -> Self {
        Self::rgba(
            (argb >> 16) as u8,
            (argb >> 8) as u8,
            argb as u8,
            (argb >> 24) as u8,
        )
    }

    /// A color from sRGB channels and opacity, each from 0.0 to 1.0, clamped
    /// and rounded to eight bits, as Compose's `Color(red, green, blue, alpha)`.
    pub fn new(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self::rgba(byte(red), byte(green), byte(blue), byte(alpha))
    }

    /// The color with components `x`, `y` and `z` in `space` and opacity
    /// `alpha`, as Compose's `Color(red, green, blue, alpha, colorSpace)`, kept
    /// in sRGB.
    ///
    /// A perceptual color outside the sRGB gamut loses chroma at constant
    /// lightness and hue until it fits, rather than being clipped channel by
    /// channel, so its hue stays what was asked for.
    pub fn in_space(x: f32, y: f32, z: f32, alpha: f32, space: ColorSpace) -> Self {
        let [red, green, blue] = match space {
            ColorSpace::Srgb => [x, y, z],
            ColorSpace::LinearSrgb => [x, y, z].map(|channel| encode(channel.clamp(0.0, 1.0))),
            ColorSpace::Oklab => oklab_to_srgb(x, y, z),
            ColorSpace::Oklch => {
                let radians = z.to_radians();
                oklab_to_srgb(x, y * radians.cos(), y * radians.sin())
            }
        };
        Self::new(red, green, blue, alpha)
    }

    /// The red channel from 0.0 to 1.0, Compose's `Color.red`.
    pub fn red(self) -> f32 {
        f32::from(self.r) / 255.0
    }

    /// The green channel from 0.0 to 1.0, Compose's `Color.green`.
    pub fn green(self) -> f32 {
        f32::from(self.g) / 255.0
    }

    /// The blue channel from 0.0 to 1.0, Compose's `Color.blue`.
    pub fn blue(self) -> f32 {
        f32::from(self.b) / 255.0
    }

    /// The opacity from 0.0 to 1.0, Compose's `Color.alpha`.
    pub fn alpha(self) -> f32 {
        f32::from(self.a) / 255.0
    }

    /// This color with opacity `alpha`, from 0.0 to 1.0, as Compose's
    /// `color.copy(alpha = alpha)`; `color.copy(color.alpha() * share)` fades it.
    pub fn copy(self, alpha: f32) -> Self {
        Self {
            a: byte(alpha),
            ..self
        }
    }

    /// The components of this color in `space` and its opacity, as
    /// `[x, y, z, alpha]`, as Compose's `color.convert(colorSpace)` gives them:
    /// red, green and blue for the RGB spaces, lightness, `a` and `b` for
    /// Oklab, and lightness, chroma and hue in degrees for Oklch.
    pub fn convert(self, space: ColorSpace) -> [f32; 4] {
        let alpha = self.alpha();
        let srgb = [self.red(), self.green(), self.blue()];
        let [x, y, z] = match space {
            ColorSpace::Srgb => srgb,
            ColorSpace::LinearSrgb => srgb.map(decode),
            ColorSpace::Oklab => srgb_to_oklab(srgb),
            ColorSpace::Oklch => {
                let [lightness, a, b] = srgb_to_oklab(srgb);
                let hue = b.atan2(a).to_degrees();
                [lightness, a.hypot(b), hue.rem_euclid(360.0)]
            }
        };
        [x, y, z, alpha]
    }

    /// This color drawn over `background`, as Compose's
    /// `color.compositeOver(background)`: source-over in the sRGB-encoded
    /// values, the way the renderer blends.
    pub fn composite_over(self, background: Self) -> Self {
        let (front, back) = (self.alpha(), background.alpha());
        let alpha = front + back * (1.0 - front);
        if alpha <= 0.0 {
            return Self::TRANSPARENT;
        }
        let channel = |over: f32, under: f32| (over * front + under * back * (1.0 - front)) / alpha;
        Self::new(
            channel(self.red(), background.red()),
            channel(self.green(), background.green()),
            channel(self.blue(), background.blue()),
            alpha,
        )
    }

    /// The relative luminance of this color, 0.0 for black to 1.0 for white,
    /// from its linear channels, as Compose's `color.luminance()`.
    pub fn luminance(self) -> f32 {
        let [red, green, blue, _] = self.convert(ColorSpace::LinearSrgb);
        (0.2126 * red + 0.7152 * green + 0.0722 * blue).clamp(0.0, 1.0)
    }

    /// The color packed as `0xAARRGGBB`, as Compose's `color.toArgb()`.
    pub const fn to_argb(self) -> u32 {
        ((self.a as u32) << 24) | ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }
}

/// The color `fraction` of the way from `start` to `stop`, as Compose's
/// `lerp(start, stop, fraction)`: interpolated in Oklab, so the colors between
/// keep an even lightness, with the opacity interpolated straight.
pub fn lerp(start: Color, stop: Color, fraction: f32) -> Color {
    let [from_l, from_a, from_b, from_alpha] = start.convert(ColorSpace::Oklab);
    let [to_l, to_a, to_b, to_alpha] = stop.convert(ColorSpace::Oklab);
    let between = |from: f32, to: f32| from + (to - from) * fraction;
    Color::in_space(
        between(from_l, to_l),
        between(from_a, to_a),
        between(from_b, to_b),
        between(from_alpha, to_alpha),
        ColorSpace::Oklab,
    )
}

/// A space colors are written in, as Compose's `ColorSpace`; the spaces
/// themselves are named as Compose names them in [`ColorSpaces`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ColorSpace {
    /// sRGB with its transfer curve: red, green and blue from 0.0 to 1.0.
    Srgb,
    /// sRGB in linear light: red, green and blue from 0.0 to 1.0.
    LinearSrgb,
    /// Oklab: perceived lightness from 0.0 to 1.0 and the green-to-red `a`
    /// and blue-to-yellow `b` axes.
    Oklab,
    /// Oklch, the polar form of Oklab: lightness from 0.0 to 1.0, chroma from
    /// 0.0 up to about 0.4, and the hue angle in degrees.
    Oklch,
}

/// The color spaces there are, as Compose's `ColorSpaces` object names them.
pub struct ColorSpaces;

impl ColorSpaces {
    /// Compose's `ColorSpaces.Srgb`.
    pub const SRGB: ColorSpace = ColorSpace::Srgb;
    /// Compose's `ColorSpaces.LinearSrgb`.
    pub const LINEAR_SRGB: ColorSpace = ColorSpace::LinearSrgb;
    /// Compose's `ColorSpaces.Oklab`.
    pub const OKLAB: ColorSpace = ColorSpace::Oklab;
    /// Oklab in its polar form of lightness, chroma and hue.
    pub const OKLCH: ColorSpace = ColorSpace::Oklch;
}

/// `fraction`, from 0.0 to 1.0, as an 8-bit channel, clamped and rounded.
fn byte(fraction: f32) -> u8 {
    (fraction.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Undoes the sRGB transfer curve of one channel.
fn decode(channel: f32) -> f32 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Applies the sRGB transfer curve to one linear channel.
fn encode(channel: f32) -> f32 {
    if channel <= 0.003_130_8 {
        channel * 12.92
    } else {
        1.055 * channel.powf(1.0 / 2.4) - 0.055
    }
}

/// The Oklab lightness, `a` and `b` of sRGB-encoded channels.
fn srgb_to_oklab(srgb: [f32; 3]) -> [f32; 3] {
    let [red, green, blue] = srgb.map(decode);
    let long = (0.412_221_47 * red + 0.536_332_55 * green + 0.051_445_995 * blue).cbrt();
    let medium = (0.211_903_5 * red + 0.680_699_5 * green + 0.107_396_96 * blue).cbrt();
    let short = (0.088_302_46 * red + 0.281_718_85 * green + 0.629_978_7 * blue).cbrt();
    [
        0.210_454_26 * long + 0.793_617_8 * medium - 0.004_072_047 * short,
        1.977_998_5 * long - 2.428_592_2 * medium + 0.450_593_7 * short,
        0.025_904_037 * long + 0.782_771_77 * medium - 0.808_675_77 * short,
    ]
}

/// The sRGB-encoded channels nearest an Oklab color: out-of-gamut colors lose
/// chroma at constant lightness and hue until they fit.
fn oklab_to_srgb(lightness: f32, a: f32, b: f32) -> [f32; 3] {
    let lightness = lightness.clamp(0.0, 1.0);
    let chroma = a.hypot(b);
    let direction = if chroma > 0.0 {
        (a / chroma, b / chroma)
    } else {
        (0.0, 0.0)
    };
    let at = |scale: f32| linear_channels(lightness, direction.0 * scale, direction.1 * scale);
    let mut channels = at(chroma);
    if !in_gamut(channels) {
        let (mut inside, mut outside) = (0.0, chroma);
        for _ in 0..24 {
            let middle = (inside + outside) / 2.0;
            if in_gamut(at(middle)) {
                inside = middle;
            } else {
                outside = middle;
            }
        }
        channels = at(inside);
    }
    channels.map(|channel| encode(channel.clamp(0.0, 1.0)))
}

/// Linear-light RGB of an Oklab coordinate, unclamped.
fn linear_channels(lightness: f32, a: f32, b: f32) -> [f32; 3] {
    let long = (lightness + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let medium = (lightness - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let short = (lightness - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * long - 3.307_711_6 * medium + 0.230_969_93 * short,
        -1.268_438 * long + 2.609_757_4 * medium - 0.341_319_4 * short,
        -0.004_196_086_3 * long - 0.703_418_6 * medium + 1.707_614_7 * short,
    ]
}

/// Whether every linear channel lies inside the sRGB gamut.
fn in_gamut(channels: [f32; 3]) -> bool {
    channels
        .iter()
        .all(|channel| (-0.000_5..=1.000_5).contains(channel))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Packed values, channel fractions and `copy` agree with the channels.
    #[test]
    fn channels_read_and_pack_as_compose_does() {
        let blue = Color::from_argb(0x803b_82f6);
        assert_eq!(blue, Color::rgba(0x3b, 0x82, 0xf6, 0x80));
        assert_eq!(blue.to_argb(), 0x803b_82f6);
        assert!((blue.alpha() - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(blue.copy(1.0), Color::rgb(0x3b, 0x82, 0xf6));
        assert_eq!(Color::new(1.0, 0.5, 0.0, 2.0), Color::rgb(255, 128, 0));
    }

    /// A color taken to every space and back is the color it was.
    #[test]
    fn colors_survive_every_space_round_trip() {
        let spaces = [
            ColorSpaces::SRGB,
            ColorSpaces::LINEAR_SRGB,
            ColorSpaces::OKLAB,
            ColorSpaces::OKLCH,
        ];
        for argb in [
            0xff00_0000,
            0xffff_ffff,
            0xff3b_82f6,
            0x80ef_4444,
            0xff7f_7f7f,
        ] {
            let color = Color::from_argb(argb);
            for space in spaces {
                let [x, y, z, alpha] = color.convert(space);
                assert_eq!(
                    Color::in_space(x, y, z, alpha, space),
                    color,
                    "{argb:08x} {space:?}"
                );
            }
        }
    }

    /// Oklch lightness, chroma and hue read as expected for white and a pure red.
    #[test]
    fn oklch_components_are_lightness_chroma_and_hue() {
        let [lightness, chroma, _, _] = Color::WHITE.convert(ColorSpaces::OKLCH);
        assert!((lightness - 1.0).abs() < 1e-3 && chroma < 1e-3);
        let [lightness, chroma, hue, _] = Color::RED.convert(ColorSpaces::OKLCH);
        assert!((lightness - 0.628).abs() < 1e-3);
        assert!((chroma - 0.2577).abs() < 1e-3);
        assert!((hue - 29.23).abs() < 0.1);
    }

    /// An Oklch color beyond sRGB keeps its lightness and hue and gives up chroma.
    #[test]
    fn out_of_gamut_colors_keep_their_hue() {
        let vivid = Color::in_space(0.7, 0.4, 150.0, 1.0, ColorSpaces::OKLCH);
        let [lightness, chroma, hue, _] = vivid.convert(ColorSpaces::OKLCH);
        assert!((lightness - 0.7).abs() < 0.01);
        assert!(chroma < 0.4);
        assert!((hue - 150.0).abs() < 2.0);
    }

    /// `lerp` runs from one color to the other through an even lightness, and
    /// interpolates the opacity straight.
    #[test]
    fn lerp_runs_from_start_to_stop() {
        assert_eq!(lerp(Color::BLACK, Color::WHITE, 0.0), Color::BLACK);
        assert_eq!(lerp(Color::BLACK, Color::WHITE, 1.0), Color::WHITE);
        let middle = lerp(Color::BLACK, Color::WHITE, 0.5);
        let [lightness, ..] = middle.convert(ColorSpaces::OKLAB);
        assert!((lightness - 0.5).abs() < 0.01);
        assert_eq!(lerp(Color::TRANSPARENT, Color::BLACK, 0.5).a, 128);
    }

    /// Compositing over a background follows source-over, and luminance runs
    /// from black to white.
    #[test]
    fn compositing_and_luminance() {
        let half_red = Color::RED.copy(0.5);
        assert_eq!(
            half_red.composite_over(Color::BLUE),
            Color::rgb(128, 0, 127)
        );
        assert_eq!(
            Color::TRANSPARENT.composite_over(Color::TRANSPARENT),
            Color::TRANSPARENT
        );
        assert_eq!(Color::BLACK.luminance(), 0.0);
        assert!((Color::WHITE.luminance() - 1.0).abs() < 1e-6);
        assert!((Color::GREEN.luminance() - 0.7152).abs() < 1e-4);
    }
}
