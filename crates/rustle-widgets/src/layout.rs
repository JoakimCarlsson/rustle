//! Surfaces, app bars, dividers and text helpers that structure a screen.

use std::rc::Rc;

use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, LOCAL_TEXT_STYLE, Modifier, Role, Semantics,
    SubcomposeResult, column, column_arranged, column_with, row_arranged, spacer,
    stack_propagating, stack_with, subcompose_layout, text, text_styled,
};
use rustle_geometry::{Offset, Size};
use rustle_graphics::Color;
use rustle_text::FontWeight;
use rustle_unit::Constraints;

use crate::shape::{BorderStroke, Shape, ShapeModifier};
use crate::theme::{
    absolute_tonal_elevation, color_scheme, provide_content_color, provide_tonal_elevation, shapes,
    text_style_of, typography,
};

/// Emits a container filled with the theme's surface color, around `content`.
#[track_caller]
pub fn surface(content: impl FnOnce()) {
    surface_with(Modifier::new(), content);
}

/// Emits a surface with `modifier`, which applies outside the surface's own look.
#[track_caller]
pub fn surface_with(modifier: Modifier, content: impl FnOnce()) {
    let theme = color_scheme();
    let radius = shapes().medium;
    stack_with(
        modifier
            .background_shape(theme.surface, radius)
            .border_shape(1.0, theme.outline, radius),
        || provide_content_color(theme.on_surface, content),
    );
}

/// Emits a surface with every part of its look given, as Compose's
/// `Surface(modifier, shape, color, contentColor, border, content)`: `color` fills `shape`,
/// `border` is drawn along it over what it holds, and text and icons inside take
/// `content_color`. Its content gets the surface's least size as its own, as Compose's
/// surface gives it (`propagateMinConstraints`), so content given no size of its own fills a
/// surface whose `modifier` sets one.
#[track_caller]
pub fn surface_custom(
    modifier: Modifier,
    shape: Shape,
    color: Color,
    content_color: Color,
    border: Option<BorderStroke>,
    content: impl FnOnce(),
) {
    let mut modifier = modifier.background_shape(color, shape);
    if let Some(border) = border {
        modifier = modifier.border_shape(border.width, border.color, shape);
    }
    stack_propagating(modifier, BoxAlignment::TopStart, || {
        provide_content_color(content_color, content)
    });
}

/// The colors of a card, as Compose's `CardColors`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardColors {
    /// The fill of the card.
    pub container_color: Color,
    /// The color of text and icons on it.
    pub content_color: Color,
}

/// The default look of cards, as Compose's `CardDefaults`.
pub struct CardDefaults;

impl CardDefaults {
    /// The colors of an outlined card: the theme's surface and the text on it, as
    /// `CardDefaults.outlinedCardColors()`.
    pub fn outlined_card_colors() -> CardColors {
        let scheme = color_scheme();
        CardColors {
            container_color: scheme.surface,
            content_color: scheme.on_surface,
        }
    }

    /// The border of an outlined card: a hairline in the theme's outline variant, as
    /// `CardDefaults.outlinedCardBorder()`.
    pub fn outlined_card_border() -> BorderStroke {
        BorderStroke::new(1.0, color_scheme().outline_variant)
    }

    /// The shape of a card: the theme's medium shape, as `CardDefaults.shape`.
    pub fn shape() -> Shape {
        shapes().medium
    }
}

/// Emits a card with an outline, as Compose's
/// `OutlinedCard(modifier, shape, colors, border, content)`: a surface of `shape` filled and
/// colored by `colors` with `border` along it, holding what `content` emits one under another.
#[track_caller]
pub fn outlined_card(
    modifier: Modifier,
    shape: Shape,
    colors: CardColors,
    border: BorderStroke,
    content: impl FnOnce(),
) {
    surface_custom(
        modifier,
        shape,
        colors.container_color,
        colors.content_color,
        Some(border),
        || {
            column_arranged(
                Modifier::new(),
                Arrangement::Start,
                Alignment::Start,
                content,
            )
        },
    );
}

/// The default look of dividers, as Compose's `DividerDefaults`.
pub struct DividerDefaults;

impl DividerDefaults {
    /// How thick a divider is: one logical pixel, as `DividerDefaults.Thickness`.
    pub const THICKNESS: f32 = 1.0;

    /// The color of a divider: the theme's outline variant, as `DividerDefaults.color`.
    pub fn color() -> Color {
        color_scheme().outline_variant
    }
}

/// Emits a line `thickness` tall in `color` across the width it is given, as Compose's
/// `HorizontalDivider(modifier, thickness, color)`.
#[track_caller]
pub fn horizontal_divider(modifier: Modifier, thickness: f32, color: Color) {
    spacer(
        modifier
            .fill_max_width()
            .height(thickness)
            .background(color),
    );
}

/// Emits a line `thickness` wide in `color` down the height it is given, as Compose's
/// `VerticalDivider(modifier, thickness, color)`.
#[track_caller]
pub fn vertical_divider(modifier: Modifier, thickness: f32, color: Color) {
    spacer(
        modifier
            .fill_max_height()
            .width(thickness)
            .background(color),
    );
}

/// Emits a surface as Compose's `Surface` does: filled in `shape` with the color of a surface
/// raised by `tonal_elevation` above the surfaces around it, casting a shadow of
/// `shadow_elevation`, and with the text colored for it.
#[track_caller]
pub fn surface_elevated(
    modifier: Modifier,
    shape: Shape,
    tonal_elevation: f32,
    shadow_elevation: f32,
    content: impl FnOnce(),
) {
    let scheme = color_scheme();
    let total = absolute_tonal_elevation() + tonal_elevation;
    let color = scheme.surface_color_at_elevation(total);
    let mut modifier = modifier;
    if shadow_elevation > 0.0 {
        modifier = modifier.shadow(shadow_elevation, shape.radius());
    }
    stack_with(modifier.background_shape(color, shape), || {
        provide_tonal_elevation(tonal_elevation, || {
            provide_content_color(scheme.content_color_for(scheme.surface), content)
        })
    });
}

/// Emits a surface: a card with a hairline border.
#[track_caller]
pub fn card(modifier: Modifier, content: impl FnOnce()) {
    surface_with(modifier, content);
}

/// Emits a card heading: a title with a muted line of description under it.
#[track_caller]
pub fn card_header(heading: &str, description: &str) {
    column(|| {
        title(heading, 14.0);
        if !description.is_empty() {
            muted_sized(description, 13.0);
        }
    });
}

/// Emits a thin line across the available width.
#[track_caller]
pub fn divider() {
    let theme = color_scheme();
    spacer(
        Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(theme.outline),
    );
}

/// Emits a hairline across the available width.
#[track_caller]
pub fn separator() {
    let theme = color_scheme();
    spacer(
        Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(theme.outline),
    );
}

/// Emits a bar across the top showing `title`, in the theme's primary color.
#[track_caller]
pub fn top_app_bar(title: &str) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .height(56.0)
            .background(theme.primary)
            .padding_xy(20.0, 0.0),
        Arrangement::Start,
        Alignment::Center,
        || {
            let style = text_style_of(typography().title_large).color(theme.on_primary);
            text_styled(
                Modifier::new().semantics(Semantics::new(Role::Label).heading(1)),
                title.to_string(),
                style,
            );
        },
    );
}

/// Emits `top_bar` above `content`, with `content` taking the rest of the space.
#[track_caller]
pub fn scaffold(top_bar: impl FnOnce(), content: impl FnOnce()) {
    column_with(Modifier::new().fill_max_size(), || {
        top_bar();
        stack_with(Modifier::new().fill_max_width().weight(1.0), content);
    });
}

/// The space a [`scaffold_with`]'s bars take, for content to keep clear of them.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScaffoldPadding {
    /// The height of the top bar.
    pub top: f32,
    /// The height of the bottom bar.
    pub bottom: f32,
}

/// The slots of a [`scaffold_with`] other than its content. Each is composed
/// while the scaffold measures, so it is a closure that can run again.
#[derive(Clone, Default)]
pub struct ScaffoldSlots {
    top_bar: Option<Rc<dyn Fn()>>,
    bottom_bar: Option<Rc<dyn Fn()>>,
    floating_action_button: Option<Rc<dyn Fn()>>,
    snackbar_host: Option<Rc<dyn Fn()>>,
}

impl ScaffoldSlots {
    /// Slots that are all empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// A bar across the top, over the content.
    pub fn top_bar(mut self, top_bar: impl Fn() + 'static) -> Self {
        self.top_bar = Some(Rc::new(top_bar));
        self
    }

    /// A bar across the bottom, over the content.
    pub fn bottom_bar(mut self, bottom_bar: impl Fn() + 'static) -> Self {
        self.bottom_bar = Some(Rc::new(bottom_bar));
        self
    }

    /// A button floating at the bottom end (the left in a right-to-left layout), above the bottom bar.
    pub fn floating_action_button(mut self, button: impl Fn() + 'static) -> Self {
        self.floating_action_button = Some(Rc::new(button));
        self
    }

    /// Where snackbars show: centered at the bottom, above the bottom bar.
    pub fn snackbar_host(mut self, host: impl Fn() + 'static) -> Self {
        self.snackbar_host = Some(Rc::new(host));
        self
    }
}

/// Emits a screen with `slots` around `content`: a top bar, a bottom bar, a
/// floating action button and a snackbar host, laid out like Compose's
/// `Scaffold`.
///
/// `content` fills the whole area, under the bars, and is told how much they
/// cover so it can pad its own contents; it is composed after the bars have
/// been measured. The scaffold fills the space it is given, with `modifier`
/// applied outside.
#[track_caller]
pub fn scaffold_with(
    modifier: Modifier,
    slots: ScaffoldSlots,
    content: impl Fn(ScaffoldPadding) + 'static,
) {
    const MARGIN: f32 = 16.0;
    let content = Rc::new(content);
    let rtl = rustle_foundation::is_rtl();
    subcompose_layout(modifier.fill_max_size(), move |scope, constraints| {
        let width = if constraints.max_width.is_finite() {
            constraints.max_width
        } else {
            constraints.min_width
        };
        let height = if constraints.max_height.is_finite() {
            constraints.max_height
        } else {
            constraints.min_height
        };
        let bar = Constraints {
            min_width: width,
            max_width: width,
            min_height: 0.0,
            max_height: height,
        };
        let loose = Constraints {
            min_width: 0.0,
            max_width: width,
            min_height: 0.0,
            max_height: height,
        };
        let mut placements = Vec::new();
        let measure_slot = |scope: &mut rustle_foundation::SubcomposeScope,
                            id: u8,
                            slot: &Option<Rc<dyn Fn()>>,
                            constraints: Constraints| {
            let slot = slot.as_ref()?;
            let slot = Rc::clone(slot);
            let index = scope.subcompose(id, move || slot()).into_iter().next()?;
            let size = scope.measure(index, constraints);
            Some((index, size))
        };
        let top = measure_slot(scope, 1, &slots.top_bar, bar);
        let bottom = measure_slot(scope, 2, &slots.bottom_bar, bar);
        let padding = ScaffoldPadding {
            top: top.map_or(0.0, |(_, size)| size.height),
            bottom: bottom.map_or(0.0, |(_, size)| size.height),
        };
        let body = {
            let content = Rc::clone(&content);
            scope.subcompose(0_u8, move || content(padding))
        };
        for index in body {
            scope.measure(index, Constraints::tight(Size::new(width, height)));
            placements.push((index, Offset::default()));
        }
        if let Some((index, _)) = top {
            placements.push((index, Offset::default()));
        }
        if let Some((index, size)) = bottom {
            placements.push((index, Offset::new(0.0, height - size.height)));
        }
        let above = height - padding.bottom;
        if let Some((index, size)) = measure_slot(scope, 3, &slots.snackbar_host, loose) {
            placements.push((
                index,
                Offset::new((width - size.width) / 2.0, above - size.height - MARGIN),
            ));
        }
        if let Some((index, size)) = measure_slot(scope, 4, &slots.floating_action_button, loose) {
            let x = if rtl {
                MARGIN
            } else {
                width - size.width - MARGIN
            };
            placements.push((index, Offset::new(x, above - size.height - MARGIN)));
        }
        SubcomposeResult {
            size: constraints.constrain(Size::new(width, height)),
            placements,
        }
    });
}

/// Emits text in the muted color, for descriptions and captions.
#[track_caller]
pub fn muted(content: &str) {
    let text_style =
        text_style_of(typography().body_medium).color(color_scheme().on_surface_variant);
    text_styled(Modifier::new(), content.to_string(), text_style);
}

/// Emits muted text of a given size.
#[track_caller]
pub fn muted_sized(content: &str, size: f32) {
    let theme = color_scheme();
    let text_style = LOCAL_TEXT_STYLE
        .current()
        .color(theme.on_surface_variant)
        .font_size(size);
    text_styled(Modifier::new(), content.to_string(), text_style);
}

/// Emits text in a semibold weight, for titles.
#[track_caller]
pub fn title(content: &str, size: f32) {
    let text_style = LOCAL_TEXT_STYLE
        .current()
        .font_size(size)
        .font_weight(FontWeight::SEMI_BOLD);
    text_styled(
        Modifier::new().semantics(Semantics::new(Role::Label).heading(2)),
        content.to_string(),
        text_style,
    );
}

/// Emits a row with a `headline`, an optional line of `supporting` text under
/// it and optional `trailing` text at the end.
#[track_caller]
pub fn list_item(headline: &str, supporting: Option<&str>, trailing: Option<&str>) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(16.0, 10.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            column(|| {
                text(headline.to_string());
                if let Some(supporting) = supporting {
                    let style =
                        text_style_of(typography().body_medium).color(theme.on_surface_variant);
                    rustle_foundation::text_styled(Modifier::new(), supporting.to_string(), style);
                }
            });
            if let Some(trailing) = trailing {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_variant);
                rustle_foundation::text_styled(Modifier::new(), trailing.to_string(), style);
            }
        },
    );
}
