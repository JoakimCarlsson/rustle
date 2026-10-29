//! Styled, ready-made components such as buttons and surfaces, with their look.
//!
//! Built only from `rustle-foundation`. Colors and shapes come from the
//! current [`Theme`], which a subtree can replace with [`provide_theme`].

mod icons;
mod modern;

pub use icons::{Icon, icon};
pub use modern::{
    BadgeStyle, ButtonStyle, MenuItem, avatar, calendar, card_header, days_in_month, first_weekday,
    menu, muted, muted_sized, pill, segmented_control, select, select_field, separator,
    styled_button, table_cell, table_row, tabs_list, title,
};

use std::rc::Rc;
use std::time::Duration;

use rustle_animation::{
    Easing, animate_color_as_state, animate_float_as_state, animate_value, infinite_float,
    infinite_float_repeating, remember_animatable, tween,
};
use rustle_foundation::{
    Alignment, Arrangement, BoxAlignment, Focus, LOCAL_TEXT_STYLE, Modifier, Role, ScrollState,
    Semantics, TextFieldOptions, TextFieldState, basic_text_field_in, column, column_arranged,
    column_with, floating, is_system_in_dark_theme, remember_focus, remember_scroll_state, reveal,
    row_arranged, row_with, spacer, stack_aligned, stack_with, text, text_styled, text_with,
};
use rustle_graphics::Color;
use rustle_runtime::{CompositionLocal, key, launched_effect, remember};
use rustle_text::FontWeight;

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
fn with_text_color<R>(color: Color, content: impl FnOnce() -> R) -> R {
    LOCAL_TEXT_STYLE.provide(LOCAL_TEXT_STYLE.current().color(color), content)
}

/// Emits a button showing `label` that runs `on_click` when clicked.
#[track_caller]
pub fn button(label: impl Into<String>, on_click: impl Fn() + 'static) {
    button_with(Modifier::new(), label, on_click);
}

/// Emits a button with `modifier`, which applies outside the button's own look.
#[track_caller]
pub fn button_with(modifier: Modifier, label: impl Into<String>, on_click: impl Fn() + 'static) {
    let theme = theme();
    let label = label.into();
    row_with(
        modifier
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.clone())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(theme.primary, theme.corner_radius)
            .padding_xy(16.0, 8.0),
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_primary)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(style, || text(label));
        },
    );
}

/// Emits a container filled with the theme's surface color, around `content`.
#[track_caller]
pub fn surface(content: impl FnOnce()) {
    surface_with(Modifier::new(), content);
}

/// Emits a surface with `modifier`, which applies outside the surface's own look.
#[track_caller]
pub fn surface_with(modifier: Modifier, content: impl FnOnce()) {
    let theme = theme();
    let radius = theme.corner_radius + 4.0;
    stack_with(
        modifier
            .background_rounded(theme.surface, radius)
            .border(1.0, theme.outline, radius),
        || with_text_color(theme.on_surface, content),
    );
}

/// Emits an outlined single-line text field showing `placeholder` while it is empty.
///
/// Size it with `modifier`, for example `Modifier::new().width(320.0)`. The
/// outline takes the theme's primary color while the field has focus.
#[track_caller]
pub fn text_field(modifier: Modifier, state: &TextFieldState, placeholder: &str) {
    let focus = remember_focus();
    text_field_focused(modifier, state, placeholder, &focus);
}

/// Like [`text_field`], with the field's `focus` given, so the caller can ask for it.
#[track_caller]
pub fn text_field_focused(
    modifier: Modifier,
    state: &TextFieldState,
    placeholder: &str,
    focus: &Focus,
) {
    let theme = theme();
    let outline = if focus.is_focused() {
        theme.on_surface_muted
    } else {
        theme.outline
    };
    let empty = state.text().is_empty();
    stack_with(
        modifier
            .background_rounded(theme.secondary, theme.corner_radius)
            .border(1.0, outline, theme.corner_radius),
        || {
            if empty {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_muted);
                LOCAL_TEXT_STYLE.provide(style, || {
                    text_with(
                        Modifier::new().padding_xy(12.0, 8.0),
                        placeholder.to_string(),
                    );
                });
            }
            with_text_color(theme.on_surface, || {
                basic_text_field_in(
                    Modifier::new(),
                    Modifier::new().padding_xy(12.0, 8.0),
                    state,
                    focus,
                    TextFieldOptions::default(),
                );
            });
        },
    );
}

/// Emits a box that is checked or not, and calls `on_change` with the opposite
/// value when clicked. The caller owns the state.
#[track_caller]
pub fn checkbox(checked: bool, on_change: impl Fn(bool) + 'static) {
    let theme = theme();
    let fill = animate_color_as_state(
        if checked {
            theme.primary
        } else {
            Color::TRANSPARENT
        },
        tween(120),
    );
    let outline = if checked {
        theme.primary
    } else {
        theme.outline
    };
    stack_aligned(
        Modifier::new()
            .size(16.0, 16.0)
            .semantics(Semantics::new(Role::CheckBox).toggled(checked))
            .clickable(move || on_change(!checked))
            .background_rounded(fill.get(), 4.0)
            .border(1.0, outline, 4.0),
        BoxAlignment::Center,
        || {
            if checked {
                let color = theme.on_primary;
                rustle_foundation::canvas(Modifier::new().size(16.0, 16.0), move |canvas| {
                    canvas.polyline(&[(4.0, 8.5), (7.0, 11.5), (12.0, 5.0)], 1.8, color);
                });
            }
        },
    );
}

/// Emits a switch that is on or off and calls `on_change` with the opposite
/// value when clicked. The thumb slides and the track changes color.
#[track_caller]
pub fn switch(checked: bool, on_change: impl Fn(bool) + 'static) {
    let theme = theme();
    let thumb = animate_float_as_state(if checked { 18.0 } else { 2.0 }, tween(140));
    let track = animate_color_as_state(
        if checked { theme.primary } else { theme.accent },
        tween(140),
    );
    let knob = if checked {
        theme.on_primary
    } else {
        theme.on_surface
    };
    stack_aligned(
        Modifier::new()
            .size(36.0, 20.0)
            .semantics(Semantics::new(Role::Switch).toggled(checked))
            .clickable(move || on_change(!checked))
            .background_rounded(track.get(), 10.0),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .offset(thumb.get(), 0.0)
                    .size(16.0, 16.0)
                    .background_rounded(knob, 8.0),
            );
        },
    );
}

/// Emits a radio button that is selected or not and runs `on_select` when clicked.
#[track_caller]
pub fn radio_button(selected: bool, on_select: impl Fn() + 'static) {
    let theme = theme();
    let outline = if selected {
        theme.primary
    } else {
        theme.outline
    };
    stack_aligned(
        Modifier::new()
            .size(16.0, 16.0)
            .semantics(Semantics::new(Role::RadioButton).toggled(selected))
            .clickable(on_select)
            .border(1.0, outline, 8.0),
        BoxAlignment::Center,
        || {
            if selected {
                spacer(
                    Modifier::new()
                        .size(8.0, 8.0)
                        .background_rounded(theme.primary, 4.0),
                );
            }
        },
    );
}

/// Emits a slider `width` wide with `value` from 0 to 1, calling `on_change`
/// with the new value while it is dragged.
#[track_caller]
pub fn slider(width: f32, value: f32, on_change: impl Fn(f32) + 'static) {
    const THUMB: f32 = 16.0;
    let theme = theme();
    let value = value.clamp(0.0, 1.0);
    let travel = (width - THUMB).max(1.0);
    stack_aligned(
        Modifier::new()
            .size(width, 24.0)
            .semantics(Semantics::new(Role::Slider).value(format!("{:.0}%", value * 100.0)))
            .on_drag(move |at| on_change(((at.x - THUMB / 2.0) / travel).clamp(0.0, 1.0))),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .fill_max_width()
                    .height(6.0)
                    .background_rounded(theme.accent, 3.0),
            );
            spacer(
                Modifier::new()
                    .width(value * travel + THUMB / 2.0)
                    .height(6.0)
                    .background_rounded(theme.primary, 3.0),
            );
            spacer(
                Modifier::new()
                    .offset(value * travel, 0.0)
                    .size(THUMB, THUMB)
                    .background_rounded(theme.background, THUMB / 2.0)
                    .border(1.5, theme.primary, THUMB / 2.0),
            );
        },
    );
}

/// Emits a progress bar `width` wide filled to `progress`, from 0 to 1.
#[track_caller]
pub fn linear_progress_indicator(width: f32, progress: f32) {
    let theme = theme();
    let fill = animate_float_as_state(progress.clamp(0.0, 1.0) * width, tween(200));
    stack_aligned(
        Modifier::new()
            .size(width, 8.0)
            .semantics(Semantics::new(Role::ProgressBar).value(format!("{:.0}%", progress * 100.0)))
            .background_rounded(theme.accent, 4.0),
        BoxAlignment::CenterStart,
        || {
            spacer(
                Modifier::new()
                    .width(fill.get())
                    .height(8.0)
                    .background_rounded(theme.primary, 4.0),
            );
        },
    );
}

/// Emits a thin line across the available width.
#[track_caller]
pub fn divider() {
    let theme = theme();
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
    let theme = theme();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .height(56.0)
            .background(theme.primary)
            .padding_xy(20.0, 0.0),
        Arrangement::Start,
        Alignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_primary)
                .font_size(21.0)
                .bold();
            text_styled(Modifier::new(), title.to_string(), style);
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

/// Emits a surface: a card with a hairline border.
#[track_caller]
pub fn card(modifier: Modifier, content: impl FnOnce()) {
    surface_with(modifier, content);
}

/// Emits a modal dialog: a dimmed layer over the whole area with `content` in
/// a card at its center. Clicking the dimmed area calls `on_dismiss`.
///
/// It fills the space it is given, so emit it last in a stack that fills the
/// window, where it covers what was emitted before it.
#[track_caller]
pub fn dialog(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    stack_aligned(
        Modifier::new()
            .fill_max_size()
            .semantics(Semantics::new(Role::Dialog))
            .clickable_plain(on_dismiss)
            .background(Color::rgba(0, 0, 0, 140)),
        BoxAlignment::Center,
        || {
            // The card takes clicks so they do not reach the dimmed area behind it.
            card(Modifier::new().clickable_plain(|| {}), || {
                stack_with(Modifier::new().padding(24.0), content);
            });
        },
    );
}

/// Emits a row of tabs labelled `labels`, with `selected` underlined, calling
/// `on_select` with the index of the tab clicked.
#[track_caller]
pub fn tab_row(selected: usize, labels: &[&str], on_select: impl Fn(usize) + 'static) {
    let theme = theme();
    let on_select = Rc::new(on_select);
    // Tabs keep their natural width and the row scrolls when they do not all fit.
    let scroll = remember_scroll_state();
    row_with(
        Modifier::new().fill_max_width().horizontal_scroll(&scroll),
        || {
            for (index, label) in labels.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    let color = if active {
                        theme.primary
                    } else {
                        theme.on_surface_muted
                    };
                    let style = LOCAL_TEXT_STYLE
                        .current()
                        .color(color)
                        .font_weight(if active {
                            FontWeight::SEMI_BOLD
                        } else {
                            FontWeight::NORMAL
                        });
                    LOCAL_TEXT_STYLE.provide(style, || {
                        text_with(
                            Modifier::new()
                                .semantics(
                                    Semantics::new(Role::Tab)
                                        .label(label.to_string())
                                        .toggled(active)
                                        .merging_descendants(),
                                )
                                .clickable(move || on_select(index))
                                .border_bottom(
                                    2.0,
                                    if active {
                                        theme.primary
                                    } else {
                                        Color::TRANSPARENT
                                    },
                                )
                                .padding_xy(18.0, 8.0),
                            label.to_string(),
                        );
                    });
                });
            }
        },
    );
}

/// Emits `content` in a layer above everything else, positioned where it is
/// emitted: its top-left corner is at the end of what was emitted before it.
/// Clicking outside `content` calls `on_dismiss`.
#[track_caller]
pub fn popup(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    floating(Modifier::new(), || {
        // An invisible layer, larger than any window, that catches outside clicks.
        spacer(
            Modifier::new()
                .size(20_000.0, 20_000.0)
                .offset(-10_000.0, -10_000.0)
                .clickable_plain(on_dismiss),
        );
        content();
    });
}

/// Emits a menu of `content` below what was emitted before it, while `expanded`.
/// Emit it right after the control that opens it, in the same column or stack.
#[track_caller]
pub fn dropdown_menu(expanded: bool, on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    dropdown_menu_at(expanded, on_dismiss, (0.0, 4.0), None, content);
}

/// Like [`dropdown_menu`], moved `offset` from where it would start and, if
/// `width` is given, exactly that wide.
#[track_caller]
pub fn dropdown_menu_at(
    expanded: bool,
    on_dismiss: impl Fn() + 'static,
    offset: (f32, f32),
    width: Option<f32>,
    content: impl FnOnce(),
) {
    if !expanded {
        return;
    }
    popup(on_dismiss, || {
        card(
            match width {
                Some(width) => Modifier::new().width(width),
                None => Modifier::new().width_in(180.0, 360.0),
            }
            .offset(offset.0, offset.1)
            // The card takes clicks, so they do not dismiss the menu.
            .clickable_plain(|| {}),
            || {
                column_with(Modifier::new().padding_xy(0.0, 8.0), content);
            },
        );
    });
}

/// Emits one entry of a menu showing `label` that runs `on_click` when clicked.
#[track_caller]
pub fn dropdown_menu_item(label: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_with(
        Modifier::new()
            .fill_max_width()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .padding_xy(16.0, 10.0),
        || with_text_color(theme.on_surface, || text(label.to_string())),
    );
}

/// Emits `content` and, while the pointer is over it, a small label below it.
#[track_caller]
pub fn tooltip(label: &str, content: impl FnOnce()) {
    let theme = theme();
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
                            let style = LOCAL_TEXT_STYLE
                                .current()
                                .color(Color::rgb(240, 240, 245))
                                .font_size(14.0)
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

/// Emits a circle `size` wide that is filled around to `progress`, from 0 to 1.
#[track_caller]
pub fn circular_progress_indicator(size: f32, progress: f32) {
    let theme = theme();
    let thickness = (size / 8.0).max(3.0);
    stack_with(
        Modifier::new()
            .size(size, size)
            .semantics(Semantics::new(Role::ProgressBar).value(format!("{:.0}%", progress * 100.0)))
            // The first ring is drawn last, so the filled part is on top of the track.
            .ring(theme.primary, thickness, 0.0, progress)
            .ring(theme.outline, thickness, 0.0, 1.0),
        || {},
    );
}

/// Emits a spinning circle `size` wide, for work of unknown length.
#[track_caller]
pub fn circular_progress_indicator_indeterminate(size: f32) {
    let theme = theme();
    let thickness = (size / 8.0).max(3.0);
    let spin = infinite_float_repeating(0.0, 360.0, Duration::from_millis(1100), Easing::Linear);
    let grow = infinite_float(0.12, 0.7, Duration::from_millis(700), Easing::EaseInOut);
    stack_with(
        Modifier::new()
            .size(size, size)
            .semantics(Semantics::new(Role::ProgressBar))
            .ring(theme.primary, thickness, spin.get(), grow.get()),
        || {},
    );
}

/// Emits a button with only a label, no fill.
#[track_caller]
pub fn text_button(label: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .padding_xy(14.0, 8.0),
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.primary)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a button with an outline and no fill.
#[track_caller]
pub fn outlined_button(label: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .border(1.5, theme.outline, theme.corner_radius)
            .padding_xy(20.0, 10.0),
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.primary)
                .font_weight(FontWeight::MEDIUM);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a round button showing `glyph`, a short text such as a symbol.
/// `description` is what assistive technology calls it.
#[track_caller]
pub fn icon_button(glyph: &str, description: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .size(40.0, 40.0)
            .semantics(Semantics::new(Role::Button).label(description.to_string()))
            .clickable(on_click)
            .background_rounded(Color::TRANSPARENT, 20.0),
        BoxAlignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_surface)
                .font_size(20.0);
            rustle_foundation::text_styled(Modifier::new(), glyph.to_string(), style);
        },
    );
}

/// Emits an outlined field of several lines, showing `placeholder` while it is empty.
/// Enter starts a new line. Size it with `modifier`, such as `.width(420.0).height(120.0)`.
#[track_caller]
pub fn text_area(modifier: Modifier, state: &TextFieldState, placeholder: &str) {
    let theme = theme();
    let focus = remember_focus();
    let outline = if focus.is_focused() {
        theme.primary
    } else {
        theme.outline
    };
    let empty = state.text().is_empty();
    stack_with(
        modifier
            .background_rounded(theme.surface, theme.corner_radius)
            .border(1.5, outline, theme.corner_radius)
            .clip(),
        || {
            if empty {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_muted);
                LOCAL_TEXT_STYLE.provide(style, || {
                    text_with(
                        Modifier::new().padding_xy(12.0, 8.0),
                        placeholder.to_string(),
                    );
                });
            }
            with_text_color(theme.on_surface, || {
                let options = TextFieldOptions {
                    read_only: false,
                    multiline: true,
                };
                basic_text_field_in(
                    Modifier::new().fill_max_size(),
                    Modifier::new().padding_xy(12.0, 8.0),
                    state,
                    &focus,
                    options,
                );
            });
        },
    );
}

/// Emits a prominent round button showing `glyph`.
#[track_caller]
pub fn floating_action_button(glyph: &str, description: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_aligned(
        Modifier::new()
            .size(56.0, 56.0)
            .semantics(Semantics::new(Role::Button).label(description.to_string()))
            .clickable(on_click)
            .shadow(8.0, 28.0)
            .background_rounded(theme.primary, 28.0),
        BoxAlignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(theme.on_primary)
                .font_size(26.0);
            rustle_foundation::text_styled(Modifier::new(), glyph.to_string(), style);
        },
    );
}

/// Emits a small rounded label that can be selected, calling `on_click` when clicked.
#[track_caller]
pub fn chip(label: &str, selected: bool, on_click: impl Fn() + 'static) {
    let theme = theme();
    let fill = animate_color_as_state(
        if selected {
            theme.primary
        } else {
            Color::TRANSPARENT
        },
        tween(150),
    );
    stack_with(
        Modifier::new()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .toggled(selected)
                    .merging_descendants(),
            )
            .clickable(on_click)
            .background_rounded(fill.get(), 8.0)
            .border(
                1.0,
                if selected {
                    theme.primary
                } else {
                    theme.outline
                },
                8.0,
            )
            .padding_xy(14.0, 6.0),
        || {
            let color = if selected {
                theme.on_primary
            } else {
                theme.on_surface
            };
            let style = LOCAL_TEXT_STYLE.current().color(color).font_size(15.0);
            LOCAL_TEXT_STYLE.provide(style, || text(label.to_string()));
        },
    );
}

/// Emits a small red circle showing `count`, to put beside something it counts.
#[track_caller]
pub fn badge(count: u32) {
    stack_aligned(
        Modifier::new()
            .width_in(18.0, 40.0)
            .height(18.0)
            .semantics(Semantics::new(Role::Label).label(format!("{count} notifications")))
            .background_rounded(Color::rgb(214, 64, 64), 9.0)
            .padding_xy(5.0, 0.0),
        BoxAlignment::Center,
        || {
            let style = LOCAL_TEXT_STYLE
                .current()
                .color(Color::rgb(255, 255, 255))
                .font_size(12.0)
                .bold();
            rustle_foundation::text_styled(Modifier::new(), count.to_string(), style);
        },
    );
}

/// Emits a bar of destinations, one per `(glyph, label)`, with `selected`
/// highlighted, calling `on_select` with the index of the one clicked.
#[track_caller]
pub fn navigation_bar(
    selected: usize,
    items: &[(&str, &str)],
    on_select: impl Fn(usize) + 'static,
) {
    let theme = theme();
    let on_select = Rc::new(on_select);
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .background(theme.surface)
            .padding_xy(8.0, 8.0),
        Arrangement::SpaceEvenly,
        Alignment::Center,
        || {
            for (index, (glyph, label)) in items.iter().enumerate() {
                let on_select = Rc::clone(&on_select);
                key(index, || {
                    let active = index == selected;
                    let color = if active {
                        theme.primary
                    } else {
                        theme.on_surface_muted
                    };
                    column_arranged(
                        Modifier::new()
                            .semantics(
                                Semantics::new(Role::Tab)
                                    .label(label.to_string())
                                    .toggled(active)
                                    .merging_descendants(),
                            )
                            .clickable(move || on_select(index))
                            .padding_xy(16.0, 4.0),
                        Arrangement::Start,
                        Alignment::Center,
                        || {
                            let icon = LOCAL_TEXT_STYLE.current().color(color).font_size(22.0);
                            rustle_foundation::text_styled(
                                Modifier::new(),
                                glyph.to_string(),
                                icon,
                            );
                            let caption = LOCAL_TEXT_STYLE.current().color(color).font_size(13.0);
                            rustle_foundation::text_styled(
                                Modifier::new(),
                                label.to_string(),
                                caption,
                            );
                        },
                    );
                });
            }
        },
    );
}

/// Emits a row with a `headline`, an optional line of `supporting` text under
/// it and optional `trailing` text at the end.
#[track_caller]
pub fn list_item(headline: &str, supporting: Option<&str>, trailing: Option<&str>) {
    let theme = theme();
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(16.0, 10.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            column(|| {
                text(headline.to_string());
                if let Some(supporting) = supporting {
                    let style = LOCAL_TEXT_STYLE
                        .current()
                        .color(theme.on_surface_muted)
                        .font_size(14.0);
                    rustle_foundation::text_styled(Modifier::new(), supporting.to_string(), style);
                }
            });
            if let Some(trailing) = trailing {
                let style = LOCAL_TEXT_STYLE.current().color(theme.on_surface_muted);
                rustle_foundation::text_styled(Modifier::new(), trailing.to_string(), style);
            }
        },
    );
}

/// Emits `content` while `visible`, fading and expanding it in and collapsing
/// and fading it out. The content leaves the composition, and forgets its
/// state, once it has finished disappearing.
#[track_caller]
pub fn animated_visibility(visible: bool, content: impl FnOnce()) {
    let progress = animate_float_as_state(if visible { 1.0 } else { 0.0 }, tween(220)).get();
    if visible || progress > 0.001 {
        reveal(progress, Modifier::new().alpha(progress), content);
    }
}

/// Emits `content` for `target`, and when `target` changes fades the old
/// content out as the new fades in.
#[track_caller]
pub fn crossfade<T: Clone + PartialEq + std::hash::Hash + 'static>(
    target: T,
    content: impl Fn(&T),
) {
    let current = remember(|| target.clone());
    let previous = remember(|| None::<T>);
    let progress = remember_animatable(1.0_f32);
    if current.get() != target {
        previous.set(Some(current.get()));
        current.set(target.clone());
        progress.snap_to(0.0);
    }

    let (fade, outgoing) = (progress.clone(), previous.clone());
    launched_effect(target.clone(), move |scope| async move {
        fade.animate_to(&scope, 1.0, tween(250)).await;
        outgoing.set(None);
    });

    let faded = progress.value();
    let old = previous.get();
    stack_with(Modifier::new(), || {
        if let Some(old) = &old {
            stack_with(Modifier::new().alpha(1.0 - faded), || content(old));
        }
        let alpha = if old.is_some() { faded } else { 1.0 };
        stack_with(Modifier::new().alpha(alpha), || content(&current.get()));
    });
}

/// Scrolls `state` to `target` over time. Run it from a `launched_effect`.
pub async fn animate_scroll_to(
    scope: &rustle_runtime::TaskScope,
    state: &ScrollState,
    target: f32,
    spec: rustle_animation::AnimationSpec,
) {
    let from = state.offset();
    let state = state.clone();
    animate_value(scope, from, target, spec, move |offset| {
        state.scroll_to(offset)
    })
    .await;
}

/// What a pager remembers: which page it is on, and where it is scrolled.
#[derive(Clone)]
pub struct PagerState {
    scroll: ScrollState,
    /// Where the last drag event was, to turn positions into movement.
    last_x: std::rc::Rc<std::cell::Cell<Option<f32>>>,
    /// The page to settle on, and a count that changes to ask for a settle.
    settle: rustle_runtime::State<(usize, u64)>,
}

impl PagerState {
    /// How wide a page is: the pager's width as of the latest layout.
    fn page_width(&self) -> f32 {
        match self.scroll.viewport_length() {
            width if width > 0.0 => width,
            _ => 400.0,
        }
    }

    /// The page nearest to what is showing.
    pub fn current_page(&self) -> usize {
        (self.scroll.offset() / self.page_width()).round().max(0.0) as usize
    }

    /// Slides to `page`.
    pub fn animate_to_page(&self, page: usize) {
        self.settle
            .update(|(target, requests)| (*target, *requests) = (page, *requests + 1));
    }
}

/// Remembers a [`PagerState`] at this call site.
#[track_caller]
pub fn remember_pager_state() -> PagerState {
    rustle_runtime::group(|| PagerState {
        scroll: rustle_foundation::remember_scroll_state(),
        last_x: remember(std::rc::Rc::default).get(),
        settle: remember(|| (0, 0)),
    })
}

/// Emits `count` pages side by side, each as wide as the pager, that can be
/// swiped between. `page` emits the page at an index. Size it with `modifier`.
#[track_caller]
pub fn horizontal_pager(
    modifier: Modifier,
    state: &PagerState,
    count: usize,
    page: impl Fn(usize),
) {
    rustle_runtime::group(|| {
        let width = state.page_width();
        let max_page = count.saturating_sub(1);

        // Settling on a page slides there, and a swipe that ends asks for it.
        let (settle, scroll) = (state.settle.get(), state.scroll.clone());
        launched_effect(settle.1, move |scope| async move {
            let target = settle.0.min(max_page) as f32 * width;
            animate_scroll_to(&scope, &scroll, target, tween(260)).await;
        });

        let (drag, release) = (state.clone(), state.clone());
        row_with(
            // The drag handlers sit outside the scrolling, so positions are
            // measured from a corner that stays put while the pages move.
            modifier
                .clip()
                .on_drag_end(move || {
                    release.last_x.set(None);
                    let nearest = release.current_page().min(max_page);
                    release.animate_to_page(nearest);
                })
                .on_drag(move |at| {
                    if let Some(last) = drag.last_x.get() {
                        drag.scroll.scroll_by(last - at.x);
                    }
                    drag.last_x.set(Some(at.x));
                })
                .horizontal_scroll(&state.scroll),
            || {
                for index in 0..count {
                    key(index, || {
                        stack_with(Modifier::new().width(width).fill_max_height(), || {
                            page(index)
                        });
                    });
                }
            },
        );
    });
}

/// Emits a row of dots, `count` of them, with the one at `current` filled.
#[track_caller]
pub fn page_indicator(count: usize, current: usize) {
    let theme = theme();
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            for index in 0..count {
                key(index, || {
                    let color = if index == current {
                        theme.primary
                    } else {
                        theme.outline
                    };
                    spacer(
                        Modifier::new()
                            .size(8.0, 8.0)
                            .background_rounded(color, 4.0),
                    );
                });
            }
        },
    );
}

/// Emits `content` that can be swiped sideways away; past a third of its width it
/// is dismissed, calling `on_dismiss`, and otherwise it slides back.
#[track_caller]
pub fn swipe_to_dismiss(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    rustle_runtime::group(|| {
        let offset = remember(|| 0.0_f32);
        let last_x = remember(std::rc::Rc::<std::cell::Cell<Option<f32>>>::default).get();
        let width = remember(|| std::rc::Rc::new(std::cell::Cell::new(300.0_f32))).get();
        let release = remember(|| 0_u64);

        let (settle, position, size) = (release.get(), offset.clone(), width.get());
        let on_dismiss = std::rc::Rc::new(on_dismiss);
        let dismiss = std::rc::Rc::clone(&on_dismiss);
        launched_effect(settle, move |scope| async move {
            if settle == 0 {
                return;
            }
            let from = position.get();
            let gone = from.abs() > size / 3.0;
            let to = if gone { from.signum() * size } else { 0.0 };
            let moving = position.clone();
            animate_value(&scope, from, to, tween(180), move |value| moving.set(value)).await;
            if gone {
                dismiss();
            }
        });

        let (drag, end) = (
            (offset.clone(), std::rc::Rc::clone(&last_x)),
            (release.clone(), last_x),
        );
        let measured = std::rc::Rc::clone(&width);
        let fade = 1.0 - (offset.get().abs() / width.get()).min(1.0);
        stack_with(
            // The drag handlers sit outside the offset, so the pointer is
            // measured from a corner that stays put while the content moves.
            Modifier::new()
                .on_size_changed(move |size| measured.set(size.width.max(1.0)))
                .on_drag_end(move || {
                    end.1.set(None);
                    end.0.update(|requests| *requests += 1);
                })
                .on_drag(move |at| {
                    if let Some(last) = drag.1.get() {
                        drag.0.update(|offset| *offset += at.x - last);
                    }
                    drag.1.set(Some(at.x));
                })
                .offset(offset.get(), 0.0)
                .alpha(fade),
            content,
        );
    });
}

/// Emits a dialog with a `title`, a `message` and two buttons: "Cancel", which
/// calls `on_dismiss`, and `confirm_label`, which calls `on_confirm`. Clicking
/// outside it also calls `on_dismiss`. Emit it last in a stack that fills the window.
#[track_caller]
pub fn alert_dialog(
    title: &str,
    message: &str,
    confirm_label: &str,
    on_confirm: impl Fn() + 'static,
    on_dismiss: impl Fn() + 'static,
) {
    let on_dismiss = Rc::new(on_dismiss);
    let cancel = Rc::clone(&on_dismiss);
    dialog(
        move || on_dismiss(),
        || {
            column_with(Modifier::new().width_in(260.0, 380.0), || {
                let heading = LOCAL_TEXT_STYLE.current().font_size(22.0).bold();
                rustle_foundation::text_styled(Modifier::new(), title.to_string(), heading);
                spacer(Modifier::new().height(8.0));
                text(message.to_string());
                spacer(Modifier::new().height(20.0));
                row_arranged(
                    Modifier::new().fill_max_width(),
                    Arrangement::End,
                    Alignment::Center,
                    || {
                        text_button("Cancel", move || cancel());
                        text_button(confirm_label, on_confirm);
                    },
                );
            });
        },
    );
}

/// Emits a slider `width` wide with two thumbs, for a range between `start`
/// and `end` (each 0 to 1, `start` first). `on_change` gets the new pair while
/// the thumb nearer to where the drag began is dragged.
#[track_caller]
pub fn range_slider(width: f32, start: f32, end: f32, on_change: impl Fn(f32, f32) + 'static) {
    const THUMB: f32 = 20.0;
    rustle_runtime::group(|| {
        let theme = theme();
        let travel = (width - THUMB).max(1.0);
        let (start, end) = (start.clamp(0.0, 1.0), end.clamp(0.0, 1.0));
        // Which thumb the current drag moves: 0 not decided yet, 1 the start, 2 the end.
        let active = remember(|| std::rc::Rc::new(std::cell::Cell::new(0_u8))).get();
        let release = std::rc::Rc::clone(&active);

        stack_aligned(
            Modifier::new()
                .size(width, 28.0)
                .semantics(Semantics::new(Role::Slider).value(format!(
                    "{:.0}% to {:.0}%",
                    start * 100.0,
                    end * 100.0
                )))
                .on_drag_end(move || release.set(0))
                .on_drag(move |at| {
                    let value = ((at.x - THUMB / 2.0) / travel).clamp(0.0, 1.0);
                    if active.get() == 0 {
                        let nearer_start = (value - start).abs() <= (value - end).abs();
                        active.set(if nearer_start { 1 } else { 2 });
                    }
                    if active.get() == 1 {
                        on_change(value.min(end), end);
                    } else {
                        on_change(start, value.max(start));
                    }
                }),
            BoxAlignment::CenterStart,
            || {
                spacer(
                    Modifier::new()
                        .fill_max_width()
                        .height(4.0)
                        .background_rounded(theme.outline, 2.0),
                );
                spacer(
                    Modifier::new()
                        .offset(start * travel + THUMB / 2.0, 0.0)
                        .width((end - start) * travel)
                        .height(4.0)
                        .background(theme.primary),
                );
                for value in [start, end] {
                    spacer(
                        Modifier::new()
                            .offset(value * travel, 0.0)
                            .size(THUMB, THUMB)
                            .background_rounded(theme.primary, THUMB / 2.0)
                            .shadow(3.0, THUMB / 2.0),
                    );
                }
            },
        );
    });
}

/// Emits a sheet that slides up from the bottom with `content` while `visible`,
/// over a dimmed layer that calls `on_dismiss` when clicked. Emit it last in a
/// stack that fills the window.
#[track_caller]
pub fn bottom_sheet(visible: bool, on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    rustle_runtime::group(|| {
        let theme = theme();
        let progress = animate_float_as_state(if visible { 1.0 } else { 0.0 }, tween(260)).get();
        let height = remember(|| std::rc::Rc::new(std::cell::Cell::new(300.0_f32))).get();
        if !visible && progress < 0.001 {
            return;
        }
        let measured = std::rc::Rc::clone(&height);
        stack_aligned(
            Modifier::new().fill_max_size().z_index(1),
            BoxAlignment::BottomCenter,
            || {
                spacer(
                    Modifier::new()
                        .fill_max_size()
                        .alpha(progress)
                        .background(Color::rgba(0, 0, 0, 140))
                        .clickable_plain(on_dismiss),
                );
                stack_with(
                    Modifier::new()
                        .fill_max_width()
                        .offset(0.0, (1.0 - progress) * height.get())
                        .on_size_changed(move |size| measured.set(size.height))
                        .shadow(10.0, 16.0)
                        .background_rounded(theme.surface, 16.0)
                        // Takes clicks, so they do not dismiss the sheet.
                        .clickable_plain(|| {})
                        .padding(24.0),
                    || with_text_color(theme.on_surface, content),
                );
            },
        );
    });
}
