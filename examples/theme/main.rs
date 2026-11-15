//! The theme as Compose's `MaterialTheme` has it: a color scheme made from one seed color, the
//! typography and shapes, tonal elevation, ripples and per-corner and cut-corner shapes.
//! Run it with `cargo run --release --example theme`.

use rustle::prelude::*;
use rustle::{
    ButtonColors, ButtonDefaults, ColorScheme, RippleConfiguration, Shape, ShapeModifier, Shapes,
    Typography, button_custom, color_scheme, provide_ripple_configuration, run, shapes,
    surface_elevated, theme, typography,
};
use rustle_text::TextStyle;

const SEEDS: [Color; 4] = [
    Color::rgb(0x67, 0x50, 0xA4),
    Color::rgb(0x00, 0x7A, 0x5E),
    Color::rgb(0xC2, 0x4A, 0x12),
    Color::rgb(0x1D, 0x4E, 0xD8),
];

/// A label in one of the typography styles.
#[composable]
fn styled(label: &str, style: TextStyle) {
    text_styled(Modifier::new(), label.to_string(), style);
}

/// A swatch of a color role with its name.
#[composable]
fn swatch(name: &str, color: Color, on: Color) {
    stack_aligned(
        Modifier::new()
            .size(112.0, 44.0)
            .background_shape(color, Shape::rounded(8.0)),
        BoxAlignment::Center,
        || styled(name, typography().label_small.color(on)),
    );
}

/// The color roles of the scheme in effect.
#[composable]
fn palette() {
    let c = color_scheme();
    flow_row_with(Modifier::new(), FlowOptions::new(8.0, 8.0), || {
        swatch("primary", c.primary, c.on_primary);
        swatch("container", c.primary_container, c.on_primary_container);
        swatch("secondary", c.secondary, c.on_secondary);
        swatch("container", c.secondary_container, c.on_secondary_container);
        swatch("tertiary", c.tertiary, c.on_tertiary);
        swatch("container", c.tertiary_container, c.on_tertiary_container);
        swatch("error", c.error, c.on_error);
        swatch("surface", c.surface, c.on_surface);
        swatch("variant", c.surface_variant, c.on_surface_variant);
    });
}

/// A block drawn in `shape`.
#[composable]
fn shape_tile(name: &str, shape: Shape) {
    let c = color_scheme();
    stack_aligned(
        Modifier::new()
            .size(112.0, 64.0)
            .background_shape(c.secondary_container, shape)
            .border_shape(2.0, c.primary, shape),
        BoxAlignment::Center,
        || {
            styled(
                name,
                typography().label_medium.color(c.on_secondary_container),
            )
        },
    );
}

/// The shapes: the scale's sizes and the shapes Material has beyond rounded corners.
#[composable]
fn shape_row() {
    let s = shapes();
    flow_row_with(Modifier::new(), FlowOptions::new(8.0, 8.0), || {
        shape_tile("small", s.small);
        shape_tile("large", s.large);
        shape_tile("extra large", s.extra_large);
        shape_tile("rectangle", Shape::Rectangle);
        shape_tile("circle", Shape::Circle);
        shape_tile("per corner", Shape::rounded_corners(24.0, 4.0, 24.0, 4.0));
        shape_tile("cut", Shape::cut(14.0));
        shape_tile("cut corners", Shape::cut_corners(20.0, 0.0, 20.0, 0.0));
    });
}

/// Surfaces raised by 0, 1, 3, 5 and 8 of tonal elevation, one inside another at the end.
#[composable]
fn elevation_row() {
    let c = color_scheme();
    row_with(Modifier::new(), || {
        for level in [0.0, 1.0, 3.0, 5.0, 8.0, 12.0] {
            spacer(Modifier::new().width(8.0));
            surface_elevated(
                Modifier::new().size(80.0, 56.0),
                Shape::rounded(12.0),
                level,
                0.0,
                || {
                    styled(
                        &format!("{level}"),
                        typography().label_large.color(c.on_surface),
                    )
                },
            );
        }
    });
}

/// Every kind of button, one with a ripple configuration of its own and one disabled.
#[composable]
fn buttons() {
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            let shape = ButtonDefaults::shape();
            let padding = ButtonDefaults::content_padding();
            button_custom(
                Modifier::new(),
                "Filled",
                true,
                ButtonDefaults::colors(),
                shape,
                padding,
                None,
                || {},
            );
            button_custom(
                Modifier::new(),
                "Tonal",
                true,
                ButtonDefaults::secondary_colors(),
                shape,
                padding,
                None,
                || {},
            );
            button_custom(
                Modifier::new(),
                "Outlined",
                true,
                ButtonDefaults::outlined_colors(),
                shape,
                padding,
                Some(ButtonDefaults::outlined_border()),
                || {},
            );
            button_custom(
                Modifier::new(),
                "Disabled",
                false,
                ButtonDefaults::colors(),
                shape,
                padding,
                None,
                || {},
            );
            provide_ripple_configuration(
                Some(RippleConfiguration {
                    color: Some(color_scheme().tertiary),
                    pressed_alpha: 0.4,
                    hovered_alpha: 0.2,
                }),
                || {
                    button_custom(
                        Modifier::new(),
                        "Tertiary ripple",
                        true,
                        ButtonColors {
                            container: color_scheme().secondary_container,
                            content: color_scheme().on_secondary_container,
                            ..ButtonDefaults::colors()
                        },
                        Shape::cut(10.0),
                        padding,
                        None,
                        || {},
                    );
                },
            );
        },
    );
}

/// The type scale, one line per role used by widgets.
#[composable]
fn type_scale() {
    let t = typography();
    column(|| {
        styled("Headline medium", t.headline_medium);
        styled("Title large", t.title_large);
        styled(
            "Body large: the quick brown fox jumps over the lazy dog",
            t.body_large,
        );
        styled("Label large", t.label_large);
    });
}

/// The whole page in a theme made from one of the seeds.
#[composable]
fn page(seed: usize, dark: bool) {
    let colors = ColorScheme::from_seed(SEEDS[seed], dark);
    let typography = Typography::default();
    let shapes = Shapes::default();
    theme(colors, typography, shapes, || {
        let scheme = color_scheme();
        column_with(
            Modifier::new()
                .fill_max_size()
                .background(scheme.background)
                .padding(24.0),
            || {
                type_scale();
                spacer(Modifier::new().height(16.0));
                palette();
                spacer(Modifier::new().height(16.0));
                shape_row();
                spacer(Modifier::new().height(16.0));
                elevation_row();
                spacer(Modifier::new().height(16.0));
                buttons();
            },
        );
    });
}

fn main() {
    run(|| {
        let seed = std::env::var("THEME_SEED")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0)
            .min(SEEDS.len() - 1);
        let dark = std::env::var_os("THEME_LIGHT").is_none();
        page(seed, dark);
    });
}
