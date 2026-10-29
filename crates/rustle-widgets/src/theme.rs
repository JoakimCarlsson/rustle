//! The [`Theme`] widgets are drawn with, and how a subtree replaces it.

use rustle_foundation::{LOCAL_TEXT_STYLE, is_system_in_dark_theme};
use rustle_graphics::Color;
use rustle_runtime::CompositionLocal;

/// The colors and corner radius widgets are drawn with.
///
/// The palette is monochrome: surfaces step up in brightness, edges are
/// hairlines, and the primary color is the foreground color itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    /// The color of the window behind everything.
    pub background: Color,
    /// Text on [`Theme::background`].
    pub on_background: Color,
    /// Fill of prominent widgets such as buttons: the foreground color.
    pub primary: Color,
    /// Text on [`Theme::primary`].
    pub on_primary: Color,
    /// Fill of containers such as cards.
    pub surface: Color,
    /// Text on [`Theme::surface`].
    pub on_surface: Color,
    /// Secondary text, such as descriptions and placeholders.
    pub on_surface_muted: Color,
    /// Hairlines around containers and between rows.
    pub outline: Color,
    /// Fill of inputs and of secondary buttons.
    pub secondary: Color,
    /// Fill of something hovered or selected in a list.
    pub accent: Color,
    /// Good news: a rise, a success.
    pub success: Color,
    /// Bad news: a fall, an error.
    pub danger: Color,
    pub corner_radius: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// White surfaces, near-black text and hairline borders.
    pub fn light() -> Self {
        Self {
            background: Color::rgb(250, 250, 250),
            on_background: Color::rgb(9, 9, 11),
            primary: Color::rgb(24, 24, 27),
            on_primary: Color::rgb(250, 250, 250),
            surface: Color::rgb(255, 255, 255),
            on_surface: Color::rgb(9, 9, 11),
            on_surface_muted: Color::rgb(113, 113, 122),
            outline: Color::rgb(228, 228, 231),
            secondary: Color::rgb(244, 244, 245),
            accent: Color::rgb(244, 244, 245),
            success: Color::rgb(22, 163, 74),
            danger: Color::rgb(220, 38, 38),
            corner_radius: 8.0,
        }
    }

    /// Near-black surfaces, off-white text and hairline borders.
    pub fn dark() -> Self {
        Self {
            background: Color::rgb(9, 9, 11),
            on_background: Color::rgb(250, 250, 250),
            primary: Color::rgb(250, 250, 250),
            on_primary: Color::rgb(24, 24, 27),
            surface: Color::rgb(17, 17, 19),
            on_surface: Color::rgb(250, 250, 250),
            on_surface_muted: Color::rgb(161, 161, 170),
            outline: Color::rgb(39, 39, 42),
            secondary: Color::rgb(24, 24, 27),
            accent: Color::rgb(39, 39, 42),
            success: Color::rgb(74, 222, 128),
            danger: Color::rgb(248, 113, 113),
            corner_radius: 8.0,
        }
    }
}

static LOCAL_THEME: CompositionLocal<Theme> = CompositionLocal::new(Theme::default);

/// The theme in effect at this point of the composition.
pub fn theme() -> Theme {
    LOCAL_THEME.current()
}

/// The light or dark theme, whichever the system is set to.
pub fn system_theme() -> Theme {
    if is_system_in_dark_theme() {
        Theme::dark()
    } else {
        Theme::light()
    }
}

/// Runs `content` with `theme` in effect, and with text colored for its background.
pub fn provide_theme<R>(theme: Theme, content: impl FnOnce() -> R) -> R {
    LOCAL_THEME.provide(theme, || with_text_color(theme.on_background, content))
}

/// Runs `content` with text colored `color`, keeping the rest of the text style.
pub(crate) fn with_text_color<R>(color: Color, content: impl FnOnce() -> R) -> R {
    LOCAL_TEXT_STYLE.provide(LOCAL_TEXT_STYLE.current().color(color), content)
}
