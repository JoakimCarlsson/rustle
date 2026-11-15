//! Colors this app needs that Material's color scheme has no role for, in a composition local of
//! the app's own, the way Compose's custom design systems guide extends `MaterialTheme`.

use rustle::prelude::*;

/// Colors that Material has no role for, added the way Compose's guide extends a theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExtendedColors {
    /// Good news: a rise, a success.
    pub success: Color,
    /// Text and icons on `success`.
    pub on_success: Color,
}

impl ExtendedColors {
    /// Green for the light scheme.
    pub const fn light() -> Self {
        Self {
            success: Color::rgb(22, 163, 74),
            on_success: Color::rgb(250, 250, 250),
        }
    }

    /// Green for the dark scheme.
    pub const fn dark() -> Self {
        Self {
            success: Color::rgb(74, 222, 128),
            on_success: Color::rgb(9, 9, 11),
        }
    }
}

/// The extended colors provided above, if any.
static LOCAL_EXTENDED_COLORS: CompositionLocal<Option<ExtendedColors>> =
    CompositionLocal::new(|| None);

/// The extended colors in effect: those given to [`provide_extended_colors`], or green picked
/// for the brightness of the current [`color_scheme`].
pub fn extended_colors() -> ExtendedColors {
    LOCAL_EXTENDED_COLORS.current().unwrap_or_else(|| {
        let surface = color_scheme().surface;
        if u16::from(surface.r) + u16::from(surface.g) + u16::from(surface.b) > 384 {
            ExtendedColors::light()
        } else {
            ExtendedColors::dark()
        }
    })
}

/// Runs `content` with `colors` as the extended colors.
pub fn provide_extended_colors<R>(colors: ExtendedColors, content: impl FnOnce() -> R) -> R {
    LOCAL_EXTENDED_COLORS.provide(Some(colors), content)
}
