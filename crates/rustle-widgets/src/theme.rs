//! The theme widgets are drawn with, as in Compose's `MaterialTheme`: a [`ColorScheme`], a
//! [`Typography`] and [`Shapes`], each read from its own composition local and each replaceable
//! for a subtree.

use rustle_foundation::{LOCAL_TEXT_SELECTION_COLORS, LOCAL_TEXT_STYLE, TextSelectionColors};
use rustle_graphics::{Color, TextWrap};
use rustle_runtime::CompositionLocal;
use rustle_text::{FontWeight, TextStyle};

use crate::shape::Shape;

/// The color roles widgets are drawn with, named and grouped as Compose Material 3's
/// `ColorScheme`. Change a few with struct update syntax:
/// `ColorScheme { primary: brand, ..ColorScheme::dark() }`.
///
/// The built-in schemes are monochrome: surfaces step up in brightness, edges are hairlines,
/// and the primary color is the foreground color itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColorScheme {
    /// The color shown most often across screens and components: prominent buttons and active states.
    pub primary: Color,
    /// Text and icons on `primary`.
    pub on_primary: Color,
    /// A tonal fill for containers related to `primary`.
    pub primary_container: Color,
    /// Text and icons on `primary_container`.
    pub on_primary_container: Color,
    /// A `primary` for places that use the inverse scheme, such as a snackbar's action.
    pub inverse_primary: Color,
    /// Less prominent accents, such as filter chips.
    pub secondary: Color,
    /// Text and icons on `secondary`.
    pub on_secondary: Color,
    /// A tonal fill for secondary containers, inputs and secondary buttons.
    pub secondary_container: Color,
    /// Text and icons on `secondary_container`.
    pub on_secondary_container: Color,
    /// A contrasting accent that balances `primary` and `secondary`.
    pub tertiary: Color,
    /// Text and icons on `tertiary`.
    pub on_tertiary: Color,
    /// A tonal fill for tertiary containers.
    pub tertiary_container: Color,
    /// Text and icons on `tertiary_container`.
    pub on_tertiary_container: Color,
    /// The color of the window behind everything.
    pub background: Color,
    /// Text and icons on `background`.
    pub on_background: Color,
    /// Fill of containers such as cards, sheets and menus.
    pub surface: Color,
    /// Text and icons on `surface`.
    pub on_surface: Color,
    /// A second surface color: hovered and selected rows, and quiet fills.
    pub surface_variant: Color,
    /// Secondary text and icons on `surface`, such as descriptions and placeholders.
    pub on_surface_variant: Color,
    /// The tint tonal elevation adds on top of `surface`.
    pub surface_tint: Color,
    /// A surface that contrasts sharply with `surface`, such as a snackbar.
    pub inverse_surface: Color,
    /// Text and icons on `inverse_surface`.
    pub inverse_on_surface: Color,
    /// Marks errors and destructive actions.
    pub error: Color,
    /// Text and icons on `error`.
    pub on_error: Color,
    /// A tonal fill for error containers.
    pub error_container: Color,
    /// Text and icons on `error_container`.
    pub on_error_container: Color,
    /// Hairlines around containers and between rows.
    pub outline: Color,
    /// Hairlines of decorative elements that need little contrast.
    pub outline_variant: Color,
    /// The color that dims content behind a modal surface.
    pub scrim: Color,
    /// A surface that is brighter than `surface` in both schemes.
    pub surface_bright: Color,
    /// A surface that is dimmer than `surface` in both schemes.
    pub surface_dim: Color,
    /// The lowest-emphasis container surface.
    pub surface_container_lowest: Color,
    /// A container surface below `surface_container`.
    pub surface_container_low: Color,
    /// The default container surface.
    pub surface_container: Color,
    /// A container surface above `surface_container`.
    pub surface_container_high: Color,
    /// The highest-emphasis container surface.
    pub surface_container_highest: Color,
    /// A `primary` that keeps its tone in light and dark.
    pub primary_fixed: Color,
    /// A dimmer `primary_fixed`.
    pub primary_fixed_dim: Color,
    /// Text and icons on `primary_fixed` and `primary_fixed_dim`.
    pub on_primary_fixed: Color,
    /// A lower-emphasis `on_primary_fixed`.
    pub on_primary_fixed_variant: Color,
    /// A `secondary` that keeps its tone in light and dark.
    pub secondary_fixed: Color,
    /// A dimmer `secondary_fixed`.
    pub secondary_fixed_dim: Color,
    /// Text and icons on `secondary_fixed` and `secondary_fixed_dim`.
    pub on_secondary_fixed: Color,
    /// A lower-emphasis `on_secondary_fixed`.
    pub on_secondary_fixed_variant: Color,
    /// A `tertiary` that keeps its tone in light and dark.
    pub tertiary_fixed: Color,
    /// A dimmer `tertiary_fixed`.
    pub tertiary_fixed_dim: Color,
    /// Text and icons on `tertiary_fixed` and `tertiary_fixed_dim`.
    pub on_tertiary_fixed: Color,
    /// A lower-emphasis `on_tertiary_fixed`.
    pub on_tertiary_fixed_variant: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::dark()
    }
}

impl ColorScheme {
    /// White surfaces, near-black text and hairline borders.
    pub fn light() -> Self {
        Self {
            primary: Color::rgb(24, 24, 24),
            on_primary: Color::rgb(250, 250, 250),
            primary_container: Color::rgb(228, 228, 228),
            on_primary_container: Color::rgb(9, 9, 9),
            inverse_primary: Color::rgb(24, 24, 24),
            secondary: Color::rgb(82, 82, 82),
            on_secondary: Color::rgb(250, 250, 250),
            secondary_container: Color::rgb(244, 244, 244),
            on_secondary_container: Color::rgb(24, 24, 24),
            tertiary: Color::rgb(63, 63, 63),
            on_tertiary: Color::rgb(250, 250, 250),
            tertiary_container: Color::rgb(228, 228, 228),
            on_tertiary_container: Color::rgb(9, 9, 9),
            background: Color::rgb(250, 250, 250),
            on_background: Color::rgb(9, 9, 9),
            surface: Color::rgb(255, 255, 255),
            on_surface: Color::rgb(9, 9, 9),
            surface_variant: Color::rgb(244, 244, 244),
            on_surface_variant: Color::rgb(113, 113, 113),
            surface_tint: Color::rgb(24, 24, 24),
            inverse_surface: Color::rgb(24, 24, 24),
            inverse_on_surface: Color::rgb(250, 250, 250),
            error: Color::rgb(220, 220, 220),
            on_error: Color::rgb(250, 250, 250),
            error_container: Color::rgb(254, 254, 254),
            on_error_container: Color::rgb(127, 127, 127),
            outline: Color::rgb(228, 228, 228),
            outline_variant: Color::rgb(238, 238, 238),
            scrim: Color::rgb(0, 0, 0),
            surface_bright: Color::rgb(255, 255, 255),
            surface_dim: Color::rgb(236, 236, 236),
            surface_container_lowest: Color::rgb(255, 255, 255),
            surface_container_low: Color::rgb(250, 250, 250),
            surface_container: Color::rgb(246, 246, 246),
            surface_container_high: Color::rgb(240, 240, 240),
            surface_container_highest: Color::rgb(234, 234, 234),
            primary_fixed: Color::rgb(228, 228, 228),
            primary_fixed_dim: Color::rgb(200, 200, 200),
            on_primary_fixed: Color::rgb(9, 9, 9),
            on_primary_fixed_variant: Color::rgb(63, 63, 63),
            secondary_fixed: Color::rgb(228, 228, 228),
            secondary_fixed_dim: Color::rgb(200, 200, 200),
            on_secondary_fixed: Color::rgb(9, 9, 9),
            on_secondary_fixed_variant: Color::rgb(63, 63, 63),
            tertiary_fixed: Color::rgb(228, 228, 228),
            tertiary_fixed_dim: Color::rgb(200, 200, 200),
            on_tertiary_fixed: Color::rgb(9, 9, 9),
            on_tertiary_fixed_variant: Color::rgb(63, 63, 63),
        }
    }

    /// Near-black surfaces, off-white text and hairline borders.
    pub fn dark() -> Self {
        Self {
            primary: Color::rgb(250, 250, 250),
            on_primary: Color::rgb(24, 24, 24),
            primary_container: Color::rgb(39, 39, 39),
            on_primary_container: Color::rgb(250, 250, 250),
            inverse_primary: Color::rgb(24, 24, 24),
            secondary: Color::rgb(161, 161, 161),
            on_secondary: Color::rgb(9, 9, 9),
            secondary_container: Color::rgb(24, 24, 24),
            on_secondary_container: Color::rgb(250, 250, 250),
            tertiary: Color::rgb(212, 212, 212),
            on_tertiary: Color::rgb(9, 9, 9),
            tertiary_container: Color::rgb(39, 39, 39),
            on_tertiary_container: Color::rgb(250, 250, 250),
            background: Color::rgb(9, 9, 9),
            on_background: Color::rgb(250, 250, 250),
            surface: Color::rgb(17, 17, 17),
            on_surface: Color::rgb(250, 250, 250),
            surface_variant: Color::rgb(39, 39, 39),
            on_surface_variant: Color::rgb(161, 161, 161),
            surface_tint: Color::rgb(250, 250, 250),
            inverse_surface: Color::rgb(244, 244, 244),
            inverse_on_surface: Color::rgb(24, 24, 24),
            error: Color::rgb(248, 248, 248),
            on_error: Color::rgb(9, 9, 9),
            error_container: Color::rgb(69, 69, 69),
            on_error_container: Color::rgb(254, 254, 254),
            outline: Color::rgb(39, 39, 39),
            outline_variant: Color::rgb(30, 30, 30),
            scrim: Color::rgb(0, 0, 0),
            surface_bright: Color::rgb(39, 39, 39),
            surface_dim: Color::rgb(9, 9, 9),
            surface_container_lowest: Color::rgb(9, 9, 9),
            surface_container_low: Color::rgb(14, 14, 14),
            surface_container: Color::rgb(17, 17, 17),
            surface_container_high: Color::rgb(24, 24, 24),
            surface_container_highest: Color::rgb(39, 39, 39),
            primary_fixed: Color::rgb(228, 228, 228),
            primary_fixed_dim: Color::rgb(200, 200, 200),
            on_primary_fixed: Color::rgb(9, 9, 9),
            on_primary_fixed_variant: Color::rgb(63, 63, 63),
            secondary_fixed: Color::rgb(228, 228, 228),
            secondary_fixed_dim: Color::rgb(200, 200, 200),
            on_secondary_fixed: Color::rgb(9, 9, 9),
            on_secondary_fixed_variant: Color::rgb(63, 63, 63),
            tertiary_fixed: Color::rgb(228, 228, 228),
            tertiary_fixed_dim: Color::rgb(200, 200, 200),
            on_tertiary_fixed: Color::rgb(9, 9, 9),
            on_tertiary_fixed_variant: Color::rgb(63, 63, 63),
        }
    }

    /// The colors selected text has under this scheme, as Compose's `MaterialTheme` gives them:
    /// `primary` for the handles and `primary` at 40% opacity for the highlight.
    pub fn text_selection_colors(&self) -> TextSelectionColors {
        let primary = self.primary;
        TextSelectionColors {
            handle_color: primary,
            background_color: Color::rgba(
                primary.r,
                primary.g,
                primary.b,
                (f32::from(primary.a) * 0.4) as u8,
            ),
        }
    }

    /// The color of a surface raised by `elevation` logical pixels, as Compose's
    /// `surfaceColorAtElevation`: `surface` with more of `surface_tint` over it the higher it
    /// is.
    pub fn surface_color_at_elevation(&self, elevation: f32) -> Color {
        if elevation <= 0.0 {
            return self.surface;
        }
        let alpha = ((4.5 * (elevation + 1.0).ln() + 2.0) / 100.0).clamp(0.0, 1.0);
        let mix = |tint: u8, base: u8| {
            (f32::from(tint) * alpha + f32::from(base) * (1.0 - alpha)).round() as u8
        };
        Color::rgba(
            mix(self.surface_tint.r, self.surface.r),
            mix(self.surface_tint.g, self.surface.g),
            mix(self.surface_tint.b, self.surface.b),
            self.surface.a,
        )
    }

    /// The color for content on top of `background`, as Compose's `contentColorFor`: the matching
    /// `on_` role of any role of this scheme, or `on_surface` for a color that is not one.
    pub fn content_color_for(&self, background: Color) -> Color {
        let pairs = [
            (self.primary, self.on_primary),
            (self.secondary, self.on_secondary),
            (self.tertiary, self.on_tertiary),
            (self.background, self.on_background),
            (self.error, self.on_error),
            (self.primary_container, self.on_primary_container),
            (self.secondary_container, self.on_secondary_container),
            (self.tertiary_container, self.on_tertiary_container),
            (self.error_container, self.on_error_container),
            (self.inverse_surface, self.inverse_on_surface),
            (self.surface_variant, self.on_surface_variant),
            (self.surface, self.on_surface),
        ];
        pairs
            .iter()
            .find(|(role, _)| *role == background)
            .map_or(self.on_surface, |(_, on)| *on)
    }
}

/// [`ColorScheme::light`], as Compose's `lightColorScheme()`.
pub fn light_color_scheme() -> ColorScheme {
    ColorScheme::light()
}

/// [`ColorScheme::dark`], as Compose's `darkColorScheme()`.
pub fn dark_color_scheme() -> ColorScheme {
    ColorScheme::dark()
}

/// The text styles widgets use, by role, as Compose Material 3's `Typography`: display,
/// headline, title, body and label, each large, medium and small. Their color is left to the
/// surrounding text color. Change a few with struct update syntax.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Typography {
    /// Largest, shortest text such as a hero number.
    pub display_large: TextStyle,
    /// Large display text.
    pub display_medium: TextStyle,
    /// Small display text.
    pub display_small: TextStyle,
    /// Large headings.
    pub headline_large: TextStyle,
    /// Section headings.
    pub headline_medium: TextStyle,
    /// Small section headings.
    pub headline_small: TextStyle,
    /// Titles of pages and dialogs.
    pub title_large: TextStyle,
    /// Titles of cards and list headings.
    pub title_medium: TextStyle,
    /// Small titles.
    pub title_small: TextStyle,
    /// Running text.
    pub body_large: TextStyle,
    /// Default text of most widgets.
    pub body_medium: TextStyle,
    /// Captions and descriptions.
    pub body_small: TextStyle,
    /// Text of buttons.
    pub label_large: TextStyle,
    /// Text of chips and tabs.
    pub label_medium: TextStyle,
    /// Text of badges and other small labels.
    pub label_small: TextStyle,
}

impl Default for Typography {
    fn default() -> Self {
        let base = TextStyle::default();
        let style = |size: f32, line: f32, weight: FontWeight, spacing: f32| {
            base.font_size(size)
                .line_height(line)
                .font_weight(weight)
                .letter_spacing(spacing)
        };
        Self {
            display_large: style(57.0, 64.0, FontWeight::NORMAL, -0.25),
            display_medium: style(45.0, 52.0, FontWeight::NORMAL, 0.0),
            display_small: style(36.0, 44.0, FontWeight::NORMAL, 0.0),
            headline_large: style(32.0, 40.0, FontWeight::NORMAL, 0.0),
            headline_medium: style(28.0, 36.0, FontWeight::NORMAL, 0.0),
            headline_small: style(24.0, 32.0, FontWeight::NORMAL, 0.0),
            title_large: style(22.0, 28.0, FontWeight::MEDIUM, 0.0),
            title_medium: style(16.0, 24.0, FontWeight::MEDIUM, 0.15),
            title_small: style(14.0, 20.0, FontWeight::MEDIUM, 0.1),
            body_large: style(16.0, 24.0, FontWeight::NORMAL, 0.5),
            body_medium: style(14.0, 20.0, FontWeight::NORMAL, 0.25),
            body_small: style(12.0, 16.0, FontWeight::NORMAL, 0.4),
            label_large: style(14.0, 20.0, FontWeight::MEDIUM, 0.1).wrap(TextWrap::None),
            label_medium: style(12.0, 16.0, FontWeight::MEDIUM, 0.5).wrap(TextWrap::None),
            label_small: style(11.0, 16.0, FontWeight::MEDIUM, 0.5).wrap(TextWrap::None),
        }
    }
}

/// The shapes widgets are drawn with, from sharp to round, as Compose Material 3's `Shapes`.
/// Each size is a [`Shape`]: rounded or cut corners, one size or one per corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shapes {
    /// Menus, snackbars and text fields.
    pub extra_small: Shape,
    /// Chips.
    pub small: Shape,
    /// Cards.
    pub medium: Shape,
    /// Floating action buttons and navigation drawers.
    pub large: Shape,
    /// Slightly larger than `large`.
    pub large_increased: Shape,
    /// Large floating action buttons.
    pub extra_large: Shape,
    /// Slightly larger than `extra_large`.
    pub extra_large_increased: Shape,
    /// The roundest size below a full circle.
    pub extra_extra_large: Shape,
}

impl Default for Shapes {
    fn default() -> Self {
        Self {
            extra_small: Shape::rounded(4.0),
            small: Shape::rounded(8.0),
            medium: Shape::rounded(12.0),
            large: Shape::rounded(16.0),
            large_increased: Shape::rounded(20.0),
            extra_large: Shape::rounded(28.0),
            extra_large_increased: Shape::rounded(32.0),
            extra_extra_large: Shape::rounded(48.0),
        }
    }
}

impl Shapes {
    /// Every size rounded by `radius`: `0.0` for square corners.
    pub fn uniform(radius: f32) -> Self {
        Self::all(Shape::rounded(radius))
    }

    /// Every size set to `shape`.
    pub fn all(shape: Shape) -> Self {
        Self {
            extra_small: shape,
            small: shape,
            medium: shape,
            large: shape,
            large_increased: shape,
            extra_large: shape,
            extra_large_increased: shape,
            extra_extra_large: shape,
        }
    }
}

static LOCAL_COLOR_SCHEME: CompositionLocal<ColorScheme> =
    CompositionLocal::new(ColorScheme::default);
static LOCAL_CONTENT_COLOR: CompositionLocal<Color> =
    CompositionLocal::new(|| ColorScheme::default().on_background);
static LOCAL_ABSOLUTE_TONAL_ELEVATION: CompositionLocal<f32> = CompositionLocal::new(|| 0.0);
static LOCAL_TYPOGRAPHY: CompositionLocal<Typography> = CompositionLocal::new(Typography::default);
static LOCAL_SHAPES: CompositionLocal<Shapes> = CompositionLocal::new(Shapes::default);

/// The tonal elevation of the surface the composition is in, as Compose's
/// `LocalAbsoluteTonalElevation`: the sum of the elevations of the surfaces around.
pub fn absolute_tonal_elevation() -> f32 {
    LOCAL_ABSOLUTE_TONAL_ELEVATION.current()
}

/// Runs `content` inside a surface raised by a further `elevation`.
pub(crate) fn provide_tonal_elevation<R>(elevation: f32, content: impl FnOnce() -> R) -> R {
    LOCAL_ABSOLUTE_TONAL_ELEVATION.provide(absolute_tonal_elevation() + elevation, content)
}

/// The typography in effect at this point of the composition, as Compose's
/// `MaterialTheme.typography`.
pub fn typography() -> Typography {
    LOCAL_TYPOGRAPHY.current()
}

/// The shapes in effect at this point of the composition, as Compose's `MaterialTheme.shapes`.
pub fn shapes() -> Shapes {
    LOCAL_SHAPES.current()
}

/// The color scheme in effect at this point of the composition, as Compose's
/// `MaterialTheme.colorScheme`.
pub fn color_scheme() -> ColorScheme {
    LOCAL_COLOR_SCHEME.current()
}

/// Runs `content` with a theme in effect, as Compose's `MaterialTheme`: each part given replaces
/// the one around it, and each part left `None` stays as the surrounding theme has it, so a
/// nested call restyles a subtree:
///
/// ```ignore
/// theme(dark_color_scheme(), None, None, || {
///     theme(ColorScheme { primary: brand, ..dark_color_scheme() }, None, Shapes::uniform(0.0), || {
///         filled_button("Square brand button", || {});
///     });
/// });
/// ```
///
/// Text inside starts in the scheme's `on_background`, and selected text takes its `primary`.
///
/// Colors Material has no role for go in a composition local of your own, provided by a theme
/// function of your own around this one, as Compose's custom design systems guide does:
///
/// ```ignore
/// #[derive(Clone, Copy)]
/// struct BrandColors { warning: Color }
///
/// static LOCAL_BRAND_COLORS: CompositionLocal<BrandColors> =
///     CompositionLocal::new(|| BrandColors { warning: Color::rgb(220, 160, 40) });
///
/// fn brand_colors() -> BrandColors {
///     LOCAL_BRAND_COLORS.current()
/// }
///
/// fn brand_theme<R>(dark: bool, content: impl FnOnce() -> R) -> R {
///     let (scheme, brand) = if dark {
///         (dark_color_scheme(), BrandColors { warning: Color::rgb(240, 190, 80) })
///     } else {
///         (light_color_scheme(), BrandColors { warning: Color::rgb(170, 110, 0) })
///     };
///     LOCAL_BRAND_COLORS.provide(brand, || theme(scheme, None, None, content))
/// }
/// ```
pub fn theme<R>(
    color_scheme: impl Into<Option<ColorScheme>>,
    typography: impl Into<Option<Typography>>,
    shapes: impl Into<Option<Shapes>>,
    content: impl FnOnce() -> R,
) -> R {
    let scheme = color_scheme.into().unwrap_or_else(self::color_scheme);
    let typography = typography.into().unwrap_or_else(self::typography);
    let shapes = shapes.into().unwrap_or_else(self::shapes);
    LOCAL_SHAPES.provide(shapes, || {
        LOCAL_TYPOGRAPHY.provide(typography, || {
            LOCAL_COLOR_SCHEME.provide(scheme, || {
                LOCAL_TEXT_SELECTION_COLORS.provide(scheme.text_selection_colors(), || {
                    provide_content_color(scheme.on_background, content)
                })
            })
        })
    })
}

/// The color text and icons take where nothing else sets one, as Compose's
/// `LocalContentColor`: surfaces and containers set it for what they hold.
pub fn content_color() -> Color {
    LOCAL_CONTENT_COLOR.current()
}

/// Runs `content` with `color` as the content color, which text inside takes unless its own
/// style says otherwise.
pub fn provide_content_color<R>(color: Color, content: impl FnOnce() -> R) -> R {
    LOCAL_CONTENT_COLOR.provide(color, || {
        LOCAL_TEXT_STYLE.provide(LOCAL_TEXT_STYLE.current().color(color), content)
    })
}

/// The color for content on `background` under the current scheme, as Compose's
/// `contentColorFor`: the matching `on_` role when `background` is one of the scheme's roles,
/// and the current [`content_color`] otherwise.
pub fn content_color_for(background: Color) -> Color {
    let scheme = color_scheme();
    let found = scheme.content_color_for(background);
    if found == scheme.on_surface && background != scheme.surface {
        content_color()
    } else {
        found
    }
}

/// `role`, one of the [`Typography`] styles, in the current [`content_color`].
pub(crate) fn text_style_of(role: TextStyle) -> TextStyle {
    role.color(content_color())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rustle_ui::composition;

    use super::*;
    use crate::ButtonDefaults;

    /// Composes `content` once and returns what it made.
    fn compose<T: 'static>(content: impl Fn() -> T + 'static) -> T {
        let out = Rc::new(RefCell::new(None));
        let kept = Rc::clone(&out);
        let mut composition = composition(move || {
            *kept.borrow_mut() = Some(content());
        });
        composition.compose();
        out.take().expect("the root composed")
    }

    /// A nested `theme` replaces all three parts for its subtree only, and the
    /// button defaults follow it.
    #[test]
    fn material_theme_overrides_a_subtree() {
        let (outer, inner, after) = compose(|| {
            let brand = Color::rgb(1, 2, 3);
            let outer = (color_scheme().primary, shapes().medium);
            let colors = ColorScheme {
                primary: brand,
                ..ColorScheme::light()
            };
            let type_scale = Typography {
                body_medium: TextStyle::default().font_size(30.0),
                ..Typography::default()
            };
            let inner = theme(colors, type_scale, Shapes::uniform(0.0), || {
                (
                    color_scheme().primary,
                    shapes().medium,
                    typography().body_medium.font_size,
                    ButtonDefaults::colors().container,
                    ButtonDefaults::colors().content,
                )
            });
            (outer, inner, color_scheme().primary)
        });
        assert_eq!(outer, (ColorScheme::dark().primary, Shape::rounded(12.0)));
        assert_eq!(
            inner,
            (
                Color::rgb(1, 2, 3),
                Shape::rounded(0.0),
                30.0,
                Color::rgb(1, 2, 3),
                ColorScheme::light().on_primary
            )
        );
        assert_eq!(after, ColorScheme::dark().primary);
    }

    /// A nested `theme` given only a color scheme keeps the typography and shapes around it, and
    /// surfaces inside set the content color for what they hold.
    #[test]
    fn a_partial_theme_keeps_the_rest_and_sets_content_color() {
        let (shape, body, content, on_primary) = compose(|| {
            theme(None, None, Shapes::uniform(2.0), || {
                theme(ColorScheme::light(), None, None, || {
                    (
                        shapes().medium,
                        typography().body_medium.font_size,
                        content_color(),
                        provide_content_color(content_color_for(color_scheme().primary), || {
                            LOCAL_TEXT_STYLE.current().color
                        }),
                    )
                })
            })
        });
        assert_eq!(shape, Shape::rounded(2.0));
        assert_eq!(body, 14.0);
        assert_eq!(content, ColorScheme::light().on_background);
        assert_eq!(on_primary, ColorScheme::light().on_primary);
    }

    /// The default type scale has the sizes and line heights of the Material 3 table.
    #[test]
    fn typography_follows_the_material_scale() {
        let t = Typography::default();
        let sizes = [
            (t.display_large, 57.0, 64.0),
            (t.display_medium, 45.0, 52.0),
            (t.display_small, 36.0, 44.0),
            (t.headline_large, 32.0, 40.0),
            (t.headline_medium, 28.0, 36.0),
            (t.headline_small, 24.0, 32.0),
            (t.title_large, 22.0, 28.0),
            (t.title_medium, 16.0, 24.0),
            (t.title_small, 14.0, 20.0),
            (t.body_large, 16.0, 24.0),
            (t.body_medium, 14.0, 20.0),
            (t.body_small, 12.0, 16.0),
            (t.label_large, 14.0, 20.0),
            (t.label_medium, 12.0, 16.0),
            (t.label_small, 11.0, 16.0),
        ];
        for (style, size, line) in sizes {
            assert_eq!((style.font_size, style.line_height), (size, line));
        }
    }

    /// `content_color_for` pairs each container role with its `on_` role.
    #[test]
    fn content_color_pairs_roles() {
        let scheme = ColorScheme::light();
        assert_eq!(scheme.content_color_for(scheme.primary), scheme.on_primary);
        assert_eq!(
            scheme.content_color_for(scheme.error_container),
            scheme.on_error_container
        );
        assert_eq!(
            scheme.content_color_for(Color::rgb(1, 2, 3)),
            scheme.on_surface
        );
    }

    /// A disabled button takes the washed-out colors of the defaults.
    #[test]
    fn disabled_button_colors_are_faded_on_surface() {
        let (colors, on_surface) =
            compose(|| (ButtonDefaults::colors(), color_scheme().on_surface));
        assert_eq!(
            colors.disabled_content,
            Color::rgba(on_surface.r, on_surface.g, on_surface.b, 96)
        );
        assert_eq!(
            colors.disabled_container,
            Color::rgba(on_surface.r, on_surface.g, on_surface.b, 30)
        );
    }

    /// Extra theme values go in a composition local of their own and a wrapper that provides
    /// both, as Compose's guide extends `MaterialTheme`; nesting the wrapper nests the values.
    #[test]
    fn a_theme_extends_through_its_own_composition_local() {
        static LOCAL_BRAND: CompositionLocal<Color> = CompositionLocal::new(|| Color::rgb(9, 9, 9));

        fn brand_theme<R>(brand: Color, content: impl FnOnce() -> R) -> R {
            LOCAL_BRAND.provide(brand, || {
                theme(
                    ColorScheme::light(),
                    Typography::default(),
                    Shapes::default(),
                    content,
                )
            })
        }

        let seen = compose(|| {
            let red = Color::rgb(200, 0, 0);
            let blue = Color::rgb(0, 0, 200);
            let outside = LOCAL_BRAND.current();
            let (a, b, light) = brand_theme(red, || {
                let inner = brand_theme(blue, || LOCAL_BRAND.current());
                (LOCAL_BRAND.current(), inner, color_scheme().surface)
            });
            (outside, a, b, light)
        });
        assert_eq!(
            seen,
            (
                Color::rgb(9, 9, 9),
                Color::rgb(200, 0, 0),
                Color::rgb(0, 0, 200),
                ColorScheme::light().surface
            )
        );
    }

    /// A surface raised by an elevation takes more of `surface_tint`, by Compose's formula, and
    /// none at zero.
    #[test]
    fn tonal_elevation_tints_the_surface() {
        let scheme = ColorScheme::light();
        assert_eq!(scheme.surface_color_at_elevation(0.0), scheme.surface);
        let raised = scheme.surface_color_at_elevation(5.0);
        let alpha = (4.5 * 6.0_f32.ln() + 2.0) / 100.0;
        let expected = (f32::from(scheme.surface_tint.r) * alpha
            + f32::from(scheme.surface.r) * (1.0 - alpha))
            .round() as u8;
        assert_eq!(raised.r, expected);
        assert!(scheme.surface_color_at_elevation(12.0).r < raised.r);
    }

    /// Nested surfaces add their elevations, as Compose's absolute tonal elevation does.
    #[test]
    fn nested_surfaces_add_their_elevation() {
        let (outside, inside, deeper) = compose(|| {
            (
                absolute_tonal_elevation(),
                provide_tonal_elevation(3.0, absolute_tonal_elevation),
                provide_tonal_elevation(3.0, || {
                    provide_tonal_elevation(2.0, absolute_tonal_elevation)
                }),
            )
        });
        assert_eq!((outside, inside, deeper), (0.0, 3.0, 5.0));
    }

    /// The theme gives selected text the primary color at 40% opacity, and a subtree can
    /// recolor it.
    #[test]
    fn the_theme_colors_the_selection() {
        let (themed, custom) = compose(|| {
            let themed = theme(ColorScheme::light(), None, None, || {
                LOCAL_TEXT_SELECTION_COLORS.current()
            });
            let own = TextSelectionColors {
                handle_color: Color::rgb(1, 2, 3),
                background_color: Color::rgba(1, 2, 3, 4),
            };
            let custom = theme(ColorScheme::light(), None, None, || {
                LOCAL_TEXT_SELECTION_COLORS.provide(own, || LOCAL_TEXT_SELECTION_COLORS.current())
            });
            (themed, custom)
        });
        let primary = ColorScheme::light().primary;
        assert_eq!(themed.handle_color, primary);
        assert_eq!(
            themed.background_color.a,
            (f32::from(primary.a) * 0.4) as u8
        );
        assert_eq!(custom.background_color, Color::rgba(1, 2, 3, 4));
    }
}
