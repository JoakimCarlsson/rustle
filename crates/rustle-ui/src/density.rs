//! How many physical pixels make a logical one where the interface is shown.

use rustle_runtime::CompositionLocal;
use rustle_unit::Dp;

/// The scale of the screen the interface is shown on: Compose's `Density`.
///
/// `density` is how many physical pixels make one logical pixel (a `Dp`), and `font_scale`
/// how much the user enlarges text. Read it with [`LOCAL_DENSITY`]; a window or an embedded
/// interface provides it, and composes again when the scale changes, such as when the window
/// moves to another screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Density {
    /// Physical pixels per logical pixel.
    pub density: f32,
    /// How much text is enlarged; 1 is as designed.
    pub font_scale: f32,
}

impl Density {
    /// A density of `density` physical pixels per logical one, with text at its size.
    pub const fn new(density: f32) -> Self {
        Self {
            density,
            font_scale: 1.0,
        }
    }

    /// `dp` in physical pixels: Compose's `Dp.toPx()`.
    pub fn to_px(&self, dp: Dp) -> f32 {
        dp.0 * self.density
    }

    /// `px` physical pixels as a `Dp`: Compose's `Float.toDp()`.
    pub fn to_dp(&self, px: f32) -> Dp {
        Dp(px / self.density)
    }
}

impl Default for Density {
    /// One physical pixel per logical one.
    fn default() -> Self {
        Self::new(1.0)
    }
}

/// The density of the screen the content is shown on: Compose's `LocalDensity`.
pub static LOCAL_DENSITY: CompositionLocal<Density> = CompositionLocal::new(Density::default);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_dp_and_pixels() {
        let density = Density::new(2.0);
        assert_eq!(density.to_px(Dp(12.0)), 24.0);
        assert_eq!(density.to_dp(24.0), Dp(12.0));
        let mut composition = crate::composition(move || {
            assert_eq!(LOCAL_DENSITY.current(), Density::default());
            assert_eq!(
                LOCAL_DENSITY.provide(density, || LOCAL_DENSITY.current()),
                density
            );
        });
        composition.compose();
    }
}
