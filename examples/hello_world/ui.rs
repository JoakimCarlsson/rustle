//! Small pieces the pages share.

use rustle::prelude::*;

/// How charts look in the current theme.
pub fn chart_style() -> ChartStyle {
    let theme = color_scheme();
    ChartStyle {
        grid: Color::rgba(theme.outline.r, theme.outline.g, theme.outline.b, 150),
        label: theme.on_surface_variant,
        surface: theme.surface,
        ..ChartStyle::default()
    }
}

/// A small series that wanders, so charts have something to show.
pub fn wander(count: usize, seed: f32, base: f32, amplitude: f32) -> Vec<f32> {
    (0..count)
        .map(|i| {
            let t = i as f32;
            base + amplitude
                * (0.55 * (t * 0.31 + seed).sin()
                    + 0.3 * (t * 0.11 + seed * 2.0).sin()
                    + 0.15 * (t * 0.9 + seed * 3.0).sin())
        })
        .collect()
}

/// A card with a title and a description, around `content`.
#[composable]
pub fn panel(heading: &str, description: &str, content: impl FnOnce()) {
    card(Modifier::new().fill_max_width(), || {
        column_with(Modifier::new().fill_max_width().padding(20.0), || {
            if !heading.is_empty() {
                card_header(heading, description);
                spacer(Modifier::new().height(16.0));
            }
            content();
        });
    });
    spacer(Modifier::new().height(16.0));
}

#[composable]
pub fn labelled(label: &str, content: impl FnOnce()) {
    muted_sized(label, 12.0);
    spacer(Modifier::new().height(6.0));
    content();
}

#[composable]
pub fn key_value(label: &str, value: &str, strong: bool) {
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 5.0),
        Arrangement::SpaceBetween,
        Alignment::Center,
        || {
            if strong {
                title(label, 13.0);
                title(value, 13.0);
            } else {
                muted_sized(label, 13.0);
                text(value.to_string());
            }
        },
    );
}

/// A bordered square with an icon that does something when clicked.
#[composable]
pub fn icon_box(glyph: Icon, size: f32, label: &str, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    stack_aligned(
        Modifier::new()
            .size(size, size)
            .semantics(Semantics::new(Role::Button).label(label.to_string()))
            .clickable(on_click)
            .border(1.0, theme.outline, (size / 2.0).min(10.0)),
        BoxAlignment::Center,
        || icon(glyph, size * 0.45, theme.on_surface),
    );
}

/// An icon without a border that does something when clicked.
#[composable]
pub fn icon_action(glyph: Icon, label: &str, on_click: impl Fn() + 'static) {
    let theme = color_scheme();
    stack_aligned(
        Modifier::new()
            .size(28.0, 28.0)
            .semantics(Semantics::new(Role::Button).label(label.to_string()))
            .clickable(on_click),
        BoxAlignment::Center,
        || icon(glyph, 16.0, theme.on_surface_variant),
    );
}

/// A bar `fraction` full, with the rest empty.
#[composable]
pub fn meter(fraction: f32) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new()
            .fill_max_width()
            .height(8.0)
            .background_rounded(theme.surface_variant, 4.0)
            .clip(),
        Arrangement::Start,
        Alignment::Start,
        || {
            spacer(
                Modifier::new()
                    .weight(fraction.clamp(0.0, 1.0).max(0.001))
                    .fill_max_height()
                    .background_rounded(theme.primary, 4.0),
            );
            spacer(Modifier::new().weight((1.0 - fraction).clamp(0.0, 1.0).max(0.001)));
        },
    );
}

/// A tinted box that is a button.
pub fn tinted(modifier: Modifier, on_click: impl Fn() + 'static) -> Modifier {
    let theme = color_scheme();
    modifier
        .clickable(on_click)
        .background_rounded(theme.secondary_container, 10.0)
        .border(1.0, theme.outline, 10.0)
        .padding(14.0)
}

/// Formats an amount of money with thousands separators.
pub fn money(amount: f64) -> String {
    let cents = (amount * 100.0).round() as i64;
    let (whole, fraction) = (cents.abs() / 100, cents.abs() % 100);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!(
        "{}${grouped}.{fraction:02}",
        if cents < 0 { "-" } else { "" }
    )
}

/// Reads an amount typed as `1,200.50`.
pub fn parse_money(text: &str) -> Option<f64> {
    text.replace([',', '$', ' '], "")
        .parse()
        .ok()
        .filter(|v: &f64| *v >= 0.0)
}

/// A chat: messages, a field to write in, and a reply a moment after each message.
#[composable]
pub fn chat(replies: &[&str], opening: &[(&str, bool)]) {
    let theme = color_scheme();
    let messages = remember_state_list(|| {
        opening
            .iter()
            .map(|(text, mine)| (text.to_string(), *mine))
            .collect::<Vec<_>>()
    });
    let draft = remember_text_field_state("");
    let waiting = remember(|| None::<usize>);

    let (reply_to, list, state) = (waiting.clone(), messages.clone(), draft.clone());
    let replies: Vec<String> = replies.iter().map(|r| r.to_string()).collect();
    launched_effect(waiting.get(), move |scope| async move {
        if let Some(count) = reply_to.get() {
            scope.delay(std::time::Duration::from_millis(900)).await;
            list.push((replies[count % replies.len()].clone(), false));
            reply_to.set(None);
        }
    });

    let send = {
        let (messages, draft, waiting) = (messages.clone(), draft.clone(), waiting.clone());
        move || {
            let text = draft.text();
            if text.trim().is_empty() || waiting.get().is_some() {
                return;
            }
            let mine = messages.to_vec().iter().filter(|(_, mine)| *mine).count();
            messages.push((text.trim().to_string(), true));
            draft.clear();
            waiting.set(Some(mine));
        }
    };
    state.on_submit(send.clone());

    for (index, (message, mine)) in messages.to_vec().into_iter().enumerate() {
        key(index, || bubble(&message, mine));
    }
    if waiting.get().is_some() {
        bubble("...", false);
    }
    spacer(Modifier::new().height(12.0));
    row_arranged(
        Modifier::new().fill_max_width(),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            stack_with(Modifier::new().weight(1.0), || {
                text_field(
                    Modifier::new().fill_max_width(),
                    &draft,
                    "Type your message...",
                );
            });
            stack_aligned(
                Modifier::new()
                    .size(36.0, 36.0)
                    .semantics(Semantics::new(Role::Button).label("Send".to_string()))
                    .clickable(send)
                    .background_rounded(theme.surface_variant, 8.0),
                BoxAlignment::Center,
                || icon(phosphor::PAPER_PLANE_TILT, 15.0, theme.on_surface),
            );
        },
    );
}

#[composable]
fn bubble(message: &str, mine: bool) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new().fill_max_width().padding_xy(0.0, 4.0),
        if mine {
            Arrangement::End
        } else {
            Arrangement::Start
        },
        Alignment::Center,
        || {
            stack_with(
                Modifier::new()
                    .width_in(40.0, 220.0)
                    .background_rounded(
                        if mine {
                            theme.primary
                        } else {
                            theme.surface_variant
                        },
                        14.0,
                    )
                    .padding_xy(12.0, 8.0),
                || {
                    let style = LOCAL_TEXT_STYLE.current().color(if mine {
                        theme.on_primary
                    } else {
                        theme.on_surface
                    });
                    text_styled(Modifier::new(), message.to_string(), style);
                },
            );
        },
    );
}
