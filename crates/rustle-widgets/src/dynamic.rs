//! Color schemes made from one source color, as Material You builds them from the user's
//! wallpaper and as the Material Theme Builder builds them from a brand color.
//!
//! The source color gives the hue. Five tonal palettes (primary, secondary, tertiary, neutral
//! and neutral variant) are cut from it, and each color role takes one tone of one palette, as
//! the Material 3 color system says. Tones are CIELAB lightness (`L*`), and hue and chroma are
//! CIELAB's too, where Material uses CAM16: close, not identical.

use rustle_graphics::Color;

use crate::theme::ColorScheme;

/// D65 white point, `[X, Y, Z]`.
const WHITE: [f32; 3] = [0.950_47, 1.0, 1.088_83];

/// Linear light of an sRGB channel in `0..=1`.
fn to_linear(channel: f32) -> f32 {
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// An sRGB channel in `0..=1` of linear light.
fn from_linear(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// The CIELAB lightness, chroma and hue (in degrees) of `color`.
pub(crate) fn lightness_chroma_hue(color: Color) -> (f32, f32, f32) {
    let [r, g, b] = [color.r, color.g, color.b].map(|c| to_linear(f32::from(c) / 255.0));
    let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175 * b;
    let z = 0.019_333_9 * r + 0.119_192 * g + 0.950_304_1 * b;
    let f = |t: f32| {
        if t > 216.0 / 24_389.0 {
            t.cbrt()
        } else {
            (24_389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x / WHITE[0]), f(y / WHITE[1]), f(z / WHITE[2]));
    let (l, a, b) = (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz));
    (l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
}

/// The sRGB color of CIELAB lightness `tone`, `chroma` and `hue`, or `None` where it does not
/// fit in sRGB.
fn lab_to_srgb(tone: f32, chroma: f32, hue: f32) -> Option<[f32; 3]> {
    let (a, b) = (
        chroma * hue.to_radians().cos(),
        chroma * hue.to_radians().sin(),
    );
    let fy = (tone + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let inverse = |t: f32| {
        if t.powi(3) > 216.0 / 24_389.0 {
            t.powi(3)
        } else {
            (116.0 * t - 16.0) * 27.0 / 24_389.0
        }
    };
    let (x, y, z) = (
        inverse(fx) * WHITE[0],
        inverse(fy) * WHITE[1],
        inverse(fz) * WHITE[2],
    );
    let linear = [
        3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z,
        -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z,
        0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z,
    ];
    let encoded = linear.map(from_linear);
    encoded
        .iter()
        .all(|c| (-0.000_5..=1.000_5).contains(c))
        .then(|| encoded.map(|c| c.clamp(0.0, 1.0)))
}

/// The color of `tone` (0 black, 100 white) in the palette of `hue` and `chroma`, with the
/// chroma lowered as far as it takes to fit in sRGB.
pub(crate) fn tone(hue: f32, chroma: f32, tone: f32) -> Color {
    let tone = tone.clamp(0.0, 100.0);
    let (mut low, mut high) = (0.0_f32, chroma.max(0.0));
    let rgb = if let Some(rgb) = lab_to_srgb(tone, high, hue) {
        rgb
    } else {
        for _ in 0..24 {
            let middle = (low + high) / 2.0;
            if lab_to_srgb(tone, middle, hue).is_some() {
                low = middle;
            } else {
                high = middle;
            }
        }
        lab_to_srgb(tone, low, hue).unwrap_or([tone / 100.0; 3])
    };
    let [r, g, b] = rgb.map(|c| (c * 255.0).round() as u8);
    Color::rgb(r, g, b)
}

/// One tonal palette: a hue and a chroma.
#[derive(Clone, Copy)]
struct Palette {
    hue: f32,
    chroma: f32,
}

impl Palette {
    /// The color of `level` in this palette.
    fn at(self, level: f32) -> Color {
        tone(self.hue, self.chroma, level)
    }
}

impl ColorScheme {
    /// A scheme made from `seed`, dark or light, the way the Material 3 color system builds
    /// one from a source color: `seed`'s hue gives the primary palette, a quieter palette of
    /// the same hue the secondary, one turned by 60 degrees the tertiary, and two nearly
    /// gray ones the surfaces.
    ///
    /// Android's dynamic color takes its seed from the wallpaper. No desktop system hands
    /// one to programs, so pass a brand color, or one the user picked.
    pub fn from_seed(seed: Color, dark: bool) -> Self {
        let (_, seed_chroma, hue) = lightness_chroma_hue(seed);
        let primary = Palette {
            hue,
            chroma: seed_chroma.max(48.0),
        };
        let secondary = Palette { hue, chroma: 16.0 };
        let tertiary = Palette {
            hue: (hue + 60.0).rem_euclid(360.0),
            chroma: 24.0,
        };
        let neutral = Palette { hue, chroma: 4.0 };
        let variant = Palette { hue, chroma: 8.0 };
        let error = Palette {
            hue: 40.0,
            chroma: 84.0,
        };
        let (fixed, fixed_dim, on_fixed, on_fixed_variant) = (90.0, 80.0, 10.0, 30.0);
        let pick = |light: f32, dark_tone: f32| if dark { dark_tone } else { light };
        Self {
            primary: primary.at(pick(40.0, 80.0)),
            on_primary: primary.at(pick(100.0, 20.0)),
            primary_container: primary.at(pick(90.0, 30.0)),
            on_primary_container: primary.at(pick(10.0, 90.0)),
            inverse_primary: primary.at(pick(80.0, 40.0)),
            secondary: secondary.at(pick(40.0, 80.0)),
            on_secondary: secondary.at(pick(100.0, 20.0)),
            secondary_container: secondary.at(pick(90.0, 30.0)),
            on_secondary_container: secondary.at(pick(10.0, 90.0)),
            tertiary: tertiary.at(pick(40.0, 80.0)),
            on_tertiary: tertiary.at(pick(100.0, 20.0)),
            tertiary_container: tertiary.at(pick(90.0, 30.0)),
            on_tertiary_container: tertiary.at(pick(10.0, 90.0)),
            background: neutral.at(pick(98.0, 6.0)),
            on_background: neutral.at(pick(10.0, 90.0)),
            surface: neutral.at(pick(98.0, 6.0)),
            on_surface: neutral.at(pick(10.0, 90.0)),
            surface_variant: variant.at(pick(90.0, 30.0)),
            on_surface_variant: variant.at(pick(30.0, 80.0)),
            surface_tint: primary.at(pick(40.0, 80.0)),
            inverse_surface: neutral.at(pick(20.0, 90.0)),
            inverse_on_surface: neutral.at(pick(95.0, 20.0)),
            error: error.at(pick(40.0, 80.0)),
            on_error: error.at(pick(100.0, 20.0)),
            error_container: error.at(pick(90.0, 30.0)),
            on_error_container: error.at(pick(10.0, 90.0)),
            outline: variant.at(pick(50.0, 60.0)),
            outline_variant: variant.at(pick(80.0, 30.0)),
            scrim: Color::rgb(0, 0, 0),
            surface_bright: neutral.at(pick(98.0, 24.0)),
            surface_dim: neutral.at(pick(87.0, 6.0)),
            surface_container_lowest: neutral.at(pick(100.0, 4.0)),
            surface_container_low: neutral.at(pick(96.0, 10.0)),
            surface_container: neutral.at(pick(94.0, 12.0)),
            surface_container_high: neutral.at(pick(92.0, 17.0)),
            surface_container_highest: neutral.at(pick(90.0, 22.0)),
            primary_fixed: primary.at(fixed),
            primary_fixed_dim: primary.at(fixed_dim),
            on_primary_fixed: primary.at(on_fixed),
            on_primary_fixed_variant: primary.at(on_fixed_variant),
            secondary_fixed: secondary.at(fixed),
            secondary_fixed_dim: secondary.at(fixed_dim),
            on_secondary_fixed: secondary.at(on_fixed),
            on_secondary_fixed_variant: secondary.at(on_fixed_variant),
            tertiary_fixed: tertiary.at(fixed),
            tertiary_fixed_dim: tertiary.at(fixed_dim),
            on_tertiary_fixed: tertiary.at(on_fixed),
            on_tertiary_fixed_variant: tertiary.at(on_fixed_variant),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tone is lightness: 0 is black, 100 is white and a middle tone measures what it says.
    #[test]
    fn tones_are_lightness() {
        assert_eq!(tone(270.0, 40.0, 0.0), Color::rgb(0, 0, 0));
        assert_eq!(tone(270.0, 40.0, 100.0), Color::rgb(255, 255, 255));
        let (l, _, _) = lightness_chroma_hue(tone(120.0, 30.0, 50.0));
        assert!((l - 50.0).abs() < 1.0, "{l}");
    }

    /// A seed's hue survives into the palette cut from it.
    #[test]
    fn the_seed_gives_the_hue() {
        let seed = Color::rgb(0x67, 0x50, 0xA4);
        let (_, _, hue) = lightness_chroma_hue(seed);
        let (_, _, primary_hue) = lightness_chroma_hue(ColorScheme::from_seed(seed, false).primary);
        assert!((hue - primary_hue).abs() < 6.0, "{hue} {primary_hue}");
    }

    /// Light schemes put primary at tone 40 and dark ones at 80, and the surfaces sit at the
    /// ends of the scale.
    #[test]
    fn schemes_follow_the_material_tones() {
        let seed = Color::rgb(0x00, 0x7a, 0x5e);
        let (light, dark) = (
            ColorScheme::from_seed(seed, false),
            ColorScheme::from_seed(seed, true),
        );
        let l = |color| lightness_chroma_hue(color).0;
        assert!((l(light.primary) - 40.0).abs() < 2.0);
        assert!((l(dark.primary) - 80.0).abs() < 2.0);
        assert!(l(light.surface) > 95.0 && l(dark.surface) < 10.0);
        assert!(l(light.on_surface) < 15.0 && l(dark.on_surface) > 85.0);
        assert_ne!(light.primary, dark.primary);
    }
}
