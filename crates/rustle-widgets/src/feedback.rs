//! Transient hints: tooltips and snackbars.

use rustle_foundation::{
    Alignment, Arrangement, BasicTooltipState, BoxAlignment, LOCAL_TEXT_STYLE, LiveRegion,
    Modifier, PlainTooltipPositionProvider, PopupPositionProvider, Role, Semantics,
    basic_tooltip_box, floating, remember_basic_tooltip_state, row_arranged, stack_with, text,
};
use rustle_graphics::Color;
use rustle_runtime::remember;

use crate::text_button;
use crate::theme::{color_scheme, text_style_of, typography};

/// Emits `content` and, while the pointer is over it, a small label below it.
#[track_caller]
pub fn tooltip(label: &str, content: impl FnOnce()) {
    let theme = color_scheme();
    let hovered = remember(|| false);
    let hover = hovered.clone();
    stack_with(
        Modifier::new().on_hover(move |over| hover.set(over)),
        || {
            content();
            if hovered.get() {
                floating(Modifier::new().align(BoxAlignment::BottomStart), || {
                    stack_with(
                        Modifier::new()
                            .offset(0.0, 6.0)
                            .shadow(4.0, 6.0)
                            .background_rounded(Color::rgb(50, 52, 66), 6.0)
                            .padding_xy(10.0, 6.0),
                        || {
                            let style = text_style_of(typography().body_medium)
                                .color(Color::rgb(240, 240, 245))
                                .max_lines(1);
                            let _ = theme;
                            rustle_foundation::text_styled(
                                Modifier::new(),
                                label.to_string(),
                                style,
                            );
                        },
                    );
                });
            }
        },
    );
}

/// Emits a message bar, optionally with one action labelled `action_label`. Put it in a stack, aligned
/// to the bottom, over the content it should cover.
#[track_caller]
pub fn snackbar(message: &str, action_label: Option<&str>, on_action: impl Fn() + 'static) {
    row_arranged(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Group)
                    .live_region(LiveRegion::Polite)
                    .label(message.to_string()),
            )
            .shadow(6.0, 8.0)
            .background_rounded(Color::rgb(50, 52, 66), 8.0)
            .padding_each(16.0, 10.0, 8.0, 10.0),
        Arrangement::SpacedBy(16.0),
        Alignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE.current().color(Color::rgb(240, 240, 245));
            LOCAL_TEXT_STYLE.provide(style, || text(message.to_string()));
            if let Some(label) = action_label {
                text_button(label, on_action);
            }
        },
    );
}

/// Whether a tooltip of a [`tooltip_box`] shows: Compose's `TooltipState`.
pub type TooltipState = BasicTooltipState;

/// A tooltip state kept across compositions: Compose's
/// `rememberTooltipState(initialIsVisible, isPersistent)`.
#[track_caller]
pub fn remember_tooltip_state(initial_is_visible: bool, is_persistent: bool) -> TooltipState {
    remember_basic_tooltip_state(initial_is_visible, is_persistent)
}

/// Emits `content` with `tooltip` shown over it while `state` is visible, placed by
/// `position_provider`: Compose's `TooltipBox(positionProvider, tooltip, state, modifier,
/// focusable, enableUserInput, content)`. See [`basic_tooltip_box`].
#[track_caller]
pub fn tooltip_box(
    position_provider: impl PopupPositionProvider + 'static,
    tooltip: impl FnOnce(),
    state: &TooltipState,
    modifier: Modifier,
    focusable: bool,
    enable_user_input: bool,
    content: impl FnOnce(),
) {
    basic_tooltip_box(
        position_provider,
        tooltip,
        state,
        modifier,
        focusable,
        enable_user_input,
        content,
    );
}

/// Defaults for tooltips: Compose's `TooltipDefaults`.
pub struct TooltipDefaults;

impl TooltipDefaults {
    /// The gap between a plain tooltip and its anchor, in logical pixels.
    pub const SPACING_BETWEEN_TOOLTIP_AND_ANCHOR: f32 = 4.0;

    /// Places a plain tooltip centred over its anchor, `spacing` above it, or below it
    /// without room above: Compose's `rememberPlainTooltipPositionProvider(spacing)`.
    pub fn plain_tooltip_position_provider(spacing: f32) -> PlainTooltipPositionProvider {
        PlainTooltipPositionProvider { spacing }
    }
}
